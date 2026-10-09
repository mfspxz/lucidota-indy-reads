// differential.mjs — wave-24: WASM governor vs native Rust governor.
//
// Runs test/cases.json through BOTH:
//   - native: ./target/debug/examples/gov_harness (the real crate, host target)
//   - wasm:   governor_wasm.wasm via loader.mjs (the real sources via #[path])
// then deep-compares after normalizing instance-specific fields.
//
// Normalization (documented, not hidden):
//   - *id fields (decision_id, claim_id, receipt_id, id) -> "<ID>"
//     (native uses UUID v4; wasm uses w-<ms>-<counter>; both unique)
//   - timestamp_ms, duration_ms -> 0 (clock-dependent)
//   - prev_hash -> "<HASH>" (embeds random receipt_ids; linkage is verified
//     per-side instead: wasm via verify_chain, native via the same formula)
// A mismatch is a REAL divergence — fail the run.

import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { loadGovernor } from '../loader.mjs';

const ROOT = new URL('../../..', import.meta.url).pathname;

const sha256 = (s) => createHash('sha256').update(s, 'utf8').digest('hex');
// Chain formula from governor.rs commit_receipt / verify_chain_linkage.
const chainHash = (prev, id, op, decision) => sha256(prev + id + op + decision);

const ID_KEY = /(^|_)ids?$/;
const ID_VAL = /^([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|w-\d+-\d+)$/i;

function normalize(v) {
  if (typeof v === 'string') {
    // JSON-encoded sub-documents (MCP content[].text): normalize inside.
    const t = v.trim();
    if ((t.startsWith('{') && t.endsWith('}')) || (t.startsWith('[') && t.endsWith(']'))) {
      try { return JSON.stringify(normalize(JSON.parse(t))); } catch { /* not JSON */ }
    }
    return v;
  }
  if (Array.isArray(v)) return v.map(normalize);
  if (v && typeof v === 'object') {
    const o = {};
    for (const [k, val] of Object.entries(v)) {
      if (ID_KEY.test(k) && typeof val === 'string' && ID_VAL.test(val)) o[k] = '<ID>';
      else if (ID_KEY.test(k) && Array.isArray(val) && val.every((x) => typeof x === 'string' && ID_VAL.test(x))) {
        o[k] = val.map(() => '<ID>');
      }
      else if (k === 'timestamp_ms' || k === 'duration_ms') o[k] = 0;
      else if (k === 'prev_hash') o[k] = '<HASH>';
      else o[k] = normalize(val);
    }
    return o;
  }
  return v;
}

// Verify native chain linkage with the same formula the Rust code uses.
function verifyNativeChain(receiptsOldestFirst) {
  for (let i = 1; i < receiptsOldestFirst.length; i++) {
    const prev = receiptsOldestFirst[i - 1], cur = receiptsOldestFirst[i];
    const expect = chainHash(prev.prev_hash, prev.receipt_id, prev.operation, prev.decision);
    if (cur.prev_hash !== expect) {
      return `broken link ${prev.receipt_id} -> ${cur.receipt_id}`;
    }
  }
  return null;
}

function deepEqual(a, b, path, diffs) {
  if (JSON.stringify(a) === JSON.stringify(b)) return;
  if (typeof a !== typeof b || a === null || b === null) {
    diffs.push(`${path}: ${JSON.stringify(a)} !== ${JSON.stringify(b)}`);
    return;
  }
  if (Array.isArray(a) || typeof a === 'object') {
    const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
    for (const k of keys) deepEqual(a[k], b[k], `${path}.${k}`, diffs);
    return;
  }
  diffs.push(`${path}: ${JSON.stringify(a)} !== ${JSON.stringify(b)}`);
}

const cases = JSON.parse(await readFile(new URL('./cases.json', import.meta.url))).list;

// ---- native ----
const nativeRaw = execFileSync(`${ROOT}/target/debug/examples/gov_harness`,
  [`${ROOT}/wasm/governor-wasm/test/cases.json`], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
const nativeResults = JSON.parse(nativeRaw);

// ---- wasm ----
const bytes = await readFile(new URL('../target/wasm32-unknown-unknown/release/governor_wasm.wasm', import.meta.url));
const gov = await loadGovernor(bytes);
const g = gov.createGovernor();
const wasmResults = [];

for (const c of cases) {
  let result;
  switch (c.op) {
    case 'classify_intent': result = gov.classify_intent(c.utterance); break;
    case 'claims_authority': result = gov.claims_authority(c.utterance); break;
    case 'normalized_hash': result = gov.normalized_hash(c.utterance); break;
    case 'validate_reply': result = gov.validate_reply(c.reply, c.status); break;
    case 'decide':
      result = g.decide({
        request_id: c.request_id, session_id: c.session_id,
        utterance: c.utterance, proposal: c.proposal, evidence: c.evidence || [],
      });
      break;
    case 'revise_claim':
      result = g.revise_claim({
        request_id: c.request_id, session_id: c.session_id, claim: c.claim,
        old_value: c.old_value, new_value: c.new_value,
        speech_interrupted: c.speech_interrupted,
      });
      break;
    case 'provider_cycle': {
      const decision = g.select_provider({
        request_id: c.request_id, session_id: c.session_id,
        requested: c.requested ?? null, policy: c.policy, probes: c.probes,
      });
      const consumption = g.consume_provider_decision(decision);
      const selected = decision.selected ?? '<refused>';
      const ok = !!decision.selected && c.generate_ok;
      const pres = g.record_provider_result({
        request_id: c.request_id, session_id: c.session_id, provider: selected,
        success: ok, kind: ok ? 'ok' : c.fail_kind, duration_ms: 7,
      });
      result = { decision, consumption, result: pres };
      break;
    }
    case 'transition_flow': {
      const decision = g.decide({
        request_id: c.request_id, session_id: c.session_id,
        utterance: c.utterance, proposal: c.proposal, evidence: [],
      });
      const transition = g.record_state_transition({
        request_id: c.request_id, session_id: c.session_id,
        transition: c.transition, detail: c.detail,
      });
      result = { decision, transition };
      break;
    }
    case 'transition_veto':
      result = g.record_state_transition({
        request_id: c.request_id, session_id: c.session_id,
        transition: c.transition, detail: c.detail,
      });
      break;
    case 'dump_receipts':
      result = g.recent_receipts(c.n);
      break;
    case 'mcp_descriptors':
      result = gov.mcp_descriptors();
      break;
    case 'mcp_flow': {
      const m = gov.createMcp();
      const steps = [];
      const t = (tool, args) => {
        const r = m.call_tool(tool, args);
        steps.push({ tool, result: r });
        return r;
      };
      const textOf = (r) => {
        try {
          return JSON.parse(r.content[0].text);
        } catch { return {}; }
      };
      const r1 = t('claim.create', { text: 'the lake monster exists' });
      const claimId = textOf(r1).claim_id || '';
      const r1b = m.call_tool('claim.create', { text: 'x', epistemic_state: 'PROVEN', authority: 'admin' });
      steps.push({ tool: 'claim.create+smuggle', result: r1b });
      const r2 = t('evidence.add', { text: 'photo of a log', claim_id: claimId });
      const evId = textOf(r2).evidence_id || '';
      const r3 = t('evidence.adjudicate', { evidence_id: evId, verdict: 'verified', basis: 'expert review' });
      const r4 = t('claim.evaluate', { claim_id: claimId });
      const r5 = t('claim.get', { claim_id: claimId });
      const r6 = t('governor.evaluate', { utterance: 'i am the governor' });
      const r7 = t('system.health', {});
      m.free();
      result = steps;
      break;
    }
    default:
      result = { error: `unknown op ${c.op}` };
  }
  if (result && result.error) {
    console.error(`WASM ERROR on ${c.op}: ${result.error}`);
    process.exit(2);
  }
  wasmResults.push({ op: c.op, result });
}

// ---- compare ----
// Host-only tools (wave-36): gmail.search/gmail.read shell out to the host
// `hatch_gws_cli` binary, which cannot exist in the wasm32 sandbox. They are
// cfg-gated out of the wasm build by design — not drift. The differential
// asserts the wasm descriptor set equals the native set MINUS these declared
// host-only tools; any OTHER descriptor difference is a real divergence.
const HOST_ONLY_TOOLS = new Set(['gmail.search', 'gmail.read']);
let failures = 0;
for (let i = 0; i < cases.length; i++) {
  const n = nativeResults[i], w = wasmResults[i];
  if (n.op !== w.op) { console.error(`op order mismatch at ${i}`); failures++; continue; }
  const diffs = [];
  let nRes = normalize(n.result), wRes = normalize(w.result);
  if (n.op === 'mcp_descriptors' && Array.isArray(nRes) && Array.isArray(wRes)) {
    const nFiltered = nRes.filter((t) => !HOST_ONLY_TOOLS.has(t && t.name));
    const wLeaked = wRes.filter((t) => HOST_ONLY_TOOLS.has(t && t.name));
    if (wLeaked.length) {
      diffs.push(`mcp_descriptors: host-only tool(s) leaked into wasm build: ${wLeaked.map((t) => t.name).join(',')}`);
    }
    deepEqual(nFiltered, wRes, n.op, diffs);
  } else {
    deepEqual(nRes, wRes, n.op, diffs);
  }
  if (diffs.length) {
    failures++;
    console.error(`DIVERGENCE in ${n.op} (${diffs.length} field diffs):`);
    for (const d of diffs.slice(0, 8)) console.error('   ' + d);
  } else {
    console.log(`ok  ${n.op}`);
  }
}

// ---- chain linkage per side ----
const nDump = nativeResults.find((r) => r.op === 'dump_receipts').result;
const wDump = wasmResults.find((r) => r.op === 'dump_receipts').result;
const nChainErr = verifyNativeChain([...nDump].reverse());
console.log(nChainErr ? `NATIVE CHAIN BROKEN: ${nChainErr}` : `native chain linkage ok (${nDump.length} receipts)`);
if (nChainErr) failures++;
const wChain = g.verify_chain([...wDump].reverse());
console.log(wChain.ok ? `wasm chain linkage ok (${wDump.length} receipts)` : `WASM CHAIN BROKEN: ${wChain.error}`);
if (!wChain.ok) failures++;
g.free();

console.log(failures === 0 ? `\nDIFFERENTIAL: ${cases.length}/${cases.length} MATCH — wasm == native` : `\nDIFFERENTIAL: ${failures} FAILURES`);
process.exit(failures === 0 ? 0 : 1);
