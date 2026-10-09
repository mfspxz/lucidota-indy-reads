// run-mcp-tools.mjs — NIH-FIND-5 follow-up (wave-30).
//
// Drives the 12 MCP tools through 3 surfaces and asserts behavioral
// equivalence:
//   1. Rust oracle (native: McpState + call_tool via rust-ref, mcp mode)
//   2. api/mcp.js (Vercel thin shell -> wasm compiled from the REAL src/mcp.rs)
//   3. cf-pages/worker.js mcp handler (Cloudflare thin shell -> same wasm)
//
// What can still diverge post-wave-24 is the SHELL PLUMBING (argument
// mapping, envelope shaping, error-class mapping, descriptor listing) —
// the governor logic itself is the same Rust by construction. This runner
// pins the plumbing.
//
// Each surface gets ONE fresh state, then runs the scripted sequence from
// mcp-tools.json in order, with $step.field substitution per surface.
// No network: the worker gets GOVERNOR_WASM_BYTES injected; any fetch
// attempt fails the run. Exits 0 on all-pass, 1 otherwise.

import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, '..', '..');
const require = createRequire(import.meta.url);

// ---- no-network guard: any fetch attempt fails the run ----
let fetchCalls = 0;
globalThis.fetch = async () => { fetchCalls++; throw new Error('NETWORK ATTEMPTED (harness forbids)'); };

const script = JSON.parse(readFileSync(path.join(HERE, 'mcp-tools.json'), 'utf8'));

// ---- Rust reference oracle (mcp mode) ----
const CARGO = process.env.HOME + '/.cargo/bin/cargo';
const refDir = path.join(HERE, 'rust-ref');
const refBin = path.join(refDir, 'target', 'release', 'js-equiv-rust-ref');
const refOut = JSON.parse(execFileSync(refBin, { input: JSON.stringify(script) }).toString());
const refSteps = new Map(refOut.steps.map(s => [s.id, s]));
const refDescriptors = refOut.descriptors;

// ---- normalization (adapted from wasm/governor-wasm/test/differential.mjs) ----
const ID_KEY = /(^|_)ids?$/;
const ID_VAL = /^([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|w-\d+-\d+)$/i;
function normalize(v) {
  if (typeof v === 'string') {
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
      else if (ID_KEY.test(k) && Array.isArray(val) && val.every(x => typeof x === 'string' && ID_VAL.test(x))) o[k] = val.map(() => '<ID>');
      else if (k === 'timestamp_ms' || k === 'duration_ms') o[k] = 0;
      else if (k === 'prev_hash') o[k] = '<HASH>';
      else o[k] = normalize(val);
    }
    return o;
  }
  return v;
}
// canonical form: normalize, then sort arrays by JSON (HashMap order is random)
function canonical(v) {
  const n = normalize(v);
  const sortDeep = (x) => {
    if (Array.isArray(x)) return x.map(sortDeep).sort((a, b) =>
      JSON.stringify(a) < JSON.stringify(b) ? -1 : 1);
    if (x && typeof x === 'object') {
      const o = {};
      for (const k of Object.keys(x).sort()) o[k] = sortDeep(x[k]);
      return o;
    }
    return x;
  };
  return JSON.stringify(sortDeep(n));
}

// ---- $step.field substitution against a surface's own prior payloads ----
function substitute(args, results) {
  const walk = (v) => {
    if (typeof v === 'string' && v.startsWith('$')) {
      const [sid, field] = v.slice(1).split('.');
      const hit = results.get(sid)?.[field];
      return hit !== undefined ? hit : `<unresolved:${v}>`;
    }
    if (Array.isArray(v)) return v.map(walk);
    if (v && typeof v === 'object') {
      const o = {};
      for (const [k, x] of Object.entries(v)) o[k] = walk(x);
      return o;
    }
    return v;
  };
  return walk(args);
}
function innerPayload(envelope) {
  try {
    const t = envelope?.content?.[0]?.text;
    return JSON.parse(t);
  } catch { return null; }
}

const rows = [];
let failures = 0;
function check(id, surface, what, ok, detail) {
  if (!ok) failures++;
  rows.push({ id, surface, what, ok: ok ? 'PASS' : 'FAIL', detail: ok ? '' : String(detail || '') });
}

// ---- surface 2: api/mcp.js (fresh module state = fresh wasm mcp instance) ----
const mcpApiPath = path.join(REPO, 'api', 'mcp.js');
const turnApiPath = path.join(REPO, 'api', 'turn.js');
delete require.cache[require.resolve(turnApiPath)];
delete require.cache[require.resolve(mcpApiPath)];
const mcpApiHandler = require(mcpApiPath);

function callMcpApi(method, body) {
  return new Promise((resolve) => {
    const req = { method, body };
    const res = {
      statusCode: 200, _body: null,
      status(c) { this.statusCode = c; return this; },
      json(o) { this._body = o; resolve({ status: this.statusCode, body: o }); },
    };
    Promise.resolve(mcpApiHandler(req, res)).catch(e => resolve({ status: 500, body: { error: 'threw: ' + e.message } }));
  });
}

// ---- surface 3: cf-pages/worker.js (fresh import = fresh isolate state) ----
const wasmBytes = readFileSync(path.join(REPO, 'wasm', 'governor-wasm', 'governor_wasm.wasm'));
const workerUrl = pathToFileURL(path.join(REPO, 'cf-pages', 'worker.js')).href + '?mcp-tools=1';
const worker = await import(workerUrl);
const workerFetch = worker.default.fetch;
const workerEnv = { GOVERNOR_WASM_BYTES: new Uint8Array(wasmBytes) };

async function callMcpWorker(method, body) {
  const req = new Request('https://harness.local/api/mcp', {
    method, headers: { 'content-type': 'application/json' },
    body: body ? JSON.stringify(body) : undefined,
  });
  const res = await workerFetch(req, workerEnv);
  return { status: res.status, body: await res.json() };
}

// ---- drive the script on a surface ----
async function driveSurface(name, callTool) {
  const results = new Map(); // step id -> raw inner payload
  const recs = [];
  for (const step of script.steps) {
    const args = substitute(step.arguments || {}, results);
    const r = await callTool(step.tool, args);
    const payload = innerPayload(r.body);
    const isError = !!(r.body && r.body.isError);
    if (payload) results.set(step.id, payload);
    recs.push({ id: step.id, tool: step.tool, isError, payload, status: r.status });
  }
  return recs;
}

const apiRecs = await driveSurface('api', async (tool, args) =>
  callMcpApi('POST', { tool, arguments: args }));
const workerRecs = await driveSurface('worker', async (tool, args) =>
  callMcpWorker('POST', { tool, arguments: args }));
const apiMap = new Map(apiRecs.map(r => [r.id, r]));
const workerMap = new Map(workerRecs.map(r => [r.id, r]));

// ---- per-step differential: error-class + canonical payload across 3 surfaces ----
for (const step of script.steps) {
  const ref = refSteps.get(step.id);
  const a = apiMap.get(step.id);
  const w = workerMap.get(step.id);

  for (const [nm, rec] of [['api/mcp.js', a], ['worker-mcp', w]]) {
    check(step.id, 'rust<->' + nm, 'error-class',
      ref.isError === rec.isError,
      `rust isError=${ref.isError} ${nm} isError=${rec.isError} status=${rec.status} payload=${JSON.stringify(rec.payload).slice(0, 120)}`);
  }
  // canonical payload equality (ids/timestamps/hashes normalized, arrays sorted)
  const rc = canonical(ref.payload);
  for (const [nm, rec] of [['api/mcp.js', a], ['worker-mcp', w]]) {
    const cc = canonical(rec.payload);
    check(step.id, 'rust<->' + nm, 'payload-equal', rc === cc,
      rc === cc ? '' : `rust=${rc.slice(0, 160)} ${nm}=${cc.slice(0, 160)}`);
  }
  // twin parity between the two JS shells
  check(step.id, 'api<->worker', 'payload-equal',
    canonical(a.payload) === canonical(w.payload),
    canonical(a.payload) === canonical(w.payload) ? '' : 'shells differ');
}

// ---- tool descriptors: GET on both shells vs Rust ----
const apiDesc = await callMcpApi('GET');
const workerDesc = await callMcpWorker('GET');
check('descriptors', 'rust<->api/mcp.js', 'tools-list',
  canonical(refDescriptors) === canonical(apiDesc.body.tools),
  'descriptor mismatch');
check('descriptors', 'rust<->worker-mcp', 'tools-list',
  canonical(refDescriptors) === canonical(workerDesc.body.tools),
  'descriptor mismatch');
check('descriptors', 'api<->worker', 'tools-list',
  canonical(apiDesc.body.tools) === canonical(workerDesc.body.tools),
  'descriptor mismatch');
check('descriptors', 'all', 'twelve-tools',
  refDescriptors.length === 12 && apiDesc.body.count === 12,
  `rust=${refDescriptors.length} api count=${apiDesc.body.count}`);

// ---- spot checks (non-vacuity: these assert Rust-meaningful values) ----
// Defensive: a totally-broken surface yields null payloads; record FAIL,
// never throw.
const s = (m, id) => m.get(id)?.payload ?? null;
const sOk = (m, id) => s(m, id) !== null;
const s01r = s(refSteps, 's01');
check('s01', 'rust', 'health-zeroed',
  s01r !== null && s01r.claims === 0 && s01r.evidence_items === 0 && s01r.receipts === 0,
  s01r === null ? 'null payload' : JSON.stringify(s01r));
for (const [nm, m] of [['api', apiMap], ['worker', workerMap]]) {
  const p = s(m, 's01');
  check('s01', nm, 'health-zeroed',
    p !== null && p.claims === 0 && p.evidence_items === 0 && p.receipts === 0,
    p === null ? 'null payload' : JSON.stringify(p));
}
for (const [nm, m] of [['rust', refSteps], ['api', apiMap], ['worker', workerMap]]) {
  const p = s(m, 's03');
  check('s03', nm, 'forbidden-stripped',
    p !== null &&
    JSON.stringify(p.stripped_forbidden_fields) === JSON.stringify(['epistemic_state', 'verified', 'actor']) &&
    p.epistemic_state === 'UNKNOWN',
    p === null ? 'null payload' : JSON.stringify(p.stripped_forbidden_fields) + ' ' + p.epistemic_state);
  check('s14', nm, 'threshold-holds-unknown',
    s(m, 's14')?.new_state === 'UNKNOWN',
    s(m, 's14') === null ? 'null payload' : s(m, 's14').new_state);
  check('s15', nm, 'per-tool-strip',
    JSON.stringify(s(m, 's15')?.stripped_forbidden_fields) === JSON.stringify(['epistemic_status', 'verdict']),
    s(m, 's15') === null ? 'null payload' : JSON.stringify(s(m, 's15').stripped_forbidden_fields));
  const h = s(m, 's26');
  check('s26', nm, 'health-counts',
    h !== null && h.claims === 2 && h.evidence_items === 3 && h.receipts === 10,
    h === null ? 'null payload' : JSON.stringify(h));
  check('s20', nm, 'authority-path',
    s(m, 's20')?.intent === 'AuthorityRequest',
    s(m, 's20') === null ? 'null payload' : s(m, 's20').intent);
}

// ---- receipt chain linkage per surface (s17 receipt.list, oldest-first) ----
// MCP chain formula (mcp.rs commit): prev_hash = sha256(prev receipt_id),
// first receipt chains from sha256("GENESIS").
const sha256 = (x) => createHash('sha256').update(x, 'utf8').digest('hex');
for (const [nm, m] of [['rust', refSteps], ['api', apiMap], ['worker', workerMap]]) {
  const rl = s(m, 's17')?.receipts;
  const list = Array.isArray(rl) ? [...rl].reverse() : []; // oldest first
  // 8 receipts exist at s17 time (s20 governor.evaluate and s24 evidence.add
  // run after s17 in the script).
  let ok = list.length === 8 && list[0].prev_hash === sha256('GENESIS');
  for (let i = 1; i < list.length && ok; i++) {
    ok = list[i].prev_hash === sha256(list[i - 1].receipt_id);
  }
  check('s17', nm, 'receipt-chain-linked', ok, ok ? '' : 'chain break or missing');
}

// ---- no network was attempted ----
check('global', 'all', 'no-network', fetchCalls === 0, `${fetchCalls} fetch calls`);

// ---- report ----
const w = { id: 14, surface: 18, what: 22, ok: 6 };
console.log('step'.padEnd(w.id) + 'surface'.padEnd(w.surface) + 'check'.padEnd(w.what) + 'verdict  detail');
for (const r of rows) {
  console.log(String(r.id).padEnd(w.id) + String(r.surface).padEnd(w.surface) +
    String(r.what).padEnd(w.what) + String(r.ok).padEnd(w.ok) + ' ' + r.detail);
}
const pass = rows.filter(r => r.ok === 'PASS').length;
console.log(`\n${pass}/${rows.length} checks passed, ${failures} failed`);
process.exit(failures ? 1 : 0);
