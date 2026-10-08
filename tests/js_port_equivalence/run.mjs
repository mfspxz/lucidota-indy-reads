// JS governor-port equivalence runner (NIH-FIND-5).
// Drives scripted intents through 3 surfaces and asserts behavioral equivalence:
//   1. Rust governor (reference oracle: evaluate_utterance / claims_authority /
//      validate_reply via tests/js_port_equivalence/rust-ref)
//   2. api/turn.js (Vercel serverless port)
//   3. cf-pages/worker.js turn handler (Cloudflare port)
// No network: ROUTER_KEY is unset, global fetch is monkeypatched to throw, and
// every response must classify as fixture. Exits 0 on all-pass, 1 otherwise,
// printing a per-intent verdict table.

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

// ---- intents ----
const intents = JSON.parse(readFileSync(path.join(HERE, 'intents.json'), 'utf8'));
for (const t of intents.turns) {
  if (t.utterance === '__LONG__') t.utterance = 'x'.repeat(2001);
}

// ---- Rust reference oracle ----
import { existsSync } from 'node:fs';
const CARGO = process.env.HOME + '/.cargo/bin/cargo';
const refDir = path.join(HERE, 'rust-ref');
const refBin = path.join(refDir, 'target', 'release', 'js-equiv-rust-ref');
if (!existsSync(refBin)) {
  console.error('rust-ref binary missing; building (one-time, ~6 min)...');
  execFileSync(CARGO, ['build', '--release', '--manifest-path', path.join(refDir, 'Cargo.toml')], { stdio: 'inherit' });
}
const refOut = JSON.parse(execFileSync(refBin, { input: JSON.stringify(intents) }).toString());
const refTurns = new Map(refOut.turns.map(t => [t.id, t]));
const refReplies = new Map(refOut.replies.map(r => [r.id, r]));

// ---- load the two JS ports (fresh module state each) ----
process.env.INDY_TEST_EXPORTS = '1';
delete process.env.ROUTER_KEY; delete process.env.ROUTER_URL;
delete process.env.ROUTER_KEY_2; delete process.env.ROUTER_URL_2;

const turnPath = path.join(REPO, 'api', 'turn.js');
delete require.cache[require.resolve(turnPath)];
const turnHandler = require(turnPath);
const turnTest = turnHandler.__governor_test;

const workerUrl = pathToFileURL(path.join(REPO, 'cf-pages', 'worker.js')).href + '?harness=1';
const worker = await import(workerUrl);
const workerFetch = worker.default.fetch;
const workerTest = globalThis.__indyGovernorTest;
delete globalThis.__indyGovernorTest;

if (!turnTest || !workerTest) {
  console.error('FATAL: test hooks missing (INDY_TEST_EXPORTS not honored by a port)');
  process.exit(2);
}

function callTurn(body) {
  return new Promise((resolve) => {
    const req = { method: 'POST', body };
    const res = {
      statusCode: 200, _body: null,
      status(c) { this.statusCode = c; return this; },
      json(o) { this._body = o; resolve({ status: this.statusCode, body: o }); },
    };
    Promise.resolve(turnHandler(req, res)).catch(e => resolve({ status: 500, body: { error: 'threw: ' + e.message } }));
  });
}

async function callWorker(body) {
  const req = new Request('https://harness.local/api/turn', {
    method: 'POST', headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  const res = await workerFetch(req, {});
  return { status: res.status, body: await res.json() };
}

// normalize volatile ids out of a response for text comparison
function scrub(o) {
  const s = JSON.stringify(o);
  return s
    .replace(/"request_id":"[^"]*"/g, '"request_id":"X"')
    .replace(/"receipt_id":"[^"]*"/g, '"receipt_id":"X"')
    .replace(/"session_id":"[^"]*"/g, '"session_id":"X"')
    .replace(/"latency_ms":\d+/g, '"latency_ms":0')
    .replace(/"ts":\d+/g, '"ts":0')
    .replace(/"graph_revision":"[^"]*"/g, '"graph_revision":"X"');
}

const rows = [];
let failures = 0;
function check(id, surface, what, ok, detail) {
  if (!ok) failures++;
  rows.push({ id, surface, what, ok: ok ? 'PASS' : 'FAIL', detail: ok ? '' : String(detail || '') });
}

// per-port receipt-chain state (external linkage verification)
const chains = {
  turn: { prev: 'GENESIS', ids: [] },
  worker: { prev: 'GENESIS', ids: [] },
};
function sha256(s) { return createHash('sha256').update(s).digest('hex'); }
function chainCheck(port, resp, correction) {
  const id = resp.receipt_id;
  const op = correction ? 'claim.revise' : 'turn.assess';
  const h = sha256(chains[port].prev + id + op + 'QUALIFIED');
  const ok = resp.provenance && resp.provenance.graph_revision === h.slice(0, 12);
  chains[port].prev = h;
  chains[port].ids.push(id);
  return ok;
}

for (const t of intents.turns) {
  const ref = refTurns.get(t.id);
  const body = { utterance: t.utterance, persona: 'indy', session_id: 'sess-' + t.id };
  if (t.correction_of) body.correction_of = t.correction_of;

  const a = await callTurn(body);
  const b = await callWorker(body);

  // --- error-class equivalence vs Rust ---
  const refErr = ref.error !== null;
  check(t.id, 'rust<->turn.js', 'error-class', refErr === (a.status === 400),
    `rust err=${ref.error} turn.js status=${a.status}`);
  check(t.id, 'rust<->worker.js', 'error-class', refErr === (b.status === 400),
    `rust err=${ref.error} worker status=${b.status}`);

  if (!refErr) {
    // --- verdict equivalence vs Rust ---
    const aAuth = a.body.response_mode === 'authority_rejected';
    const bAuth = b.body.response_mode === 'authority_rejected';
    check(t.id, 'rust<->turn.js', 'authority-reject', ref.authority === aAuth,
      `rust=${ref.authority} turn.js mode=${a.body.response_mode} reason=${a.body.governor_reason}`);
    check(t.id, 'rust<->worker.js', 'authority-reject', ref.authority === bAuth,
      `rust=${ref.authority} worker mode=${b.body.response_mode}`);
    check(t.id, 'rust<->turn.js', 'epistemic-unknown',
      a.body.epistemic_status === 'UNKNOWN' && ref.epistemic === 'UNKNOWN',
      `turn.js=${a.body.epistemic_status} rust=${ref.epistemic}`);
    check(t.id, 'rust<->worker.js', 'epistemic-unknown',
      b.body.epistemic_status === 'UNKNOWN',
      `worker=${b.body.epistemic_status}`);

    // --- twin parity: full scrubbed payload must be identical ---
    const sa = scrub(a.body), sb = scrub(b.body);
    check(t.id, 'turn.js<->worker.js', 'payload-identical', sa === sb,
      sa === sb ? '' : 'payloads differ');

    // --- receipt linkage: recompute the hash chain externally ---
    const corr = !!t.correction_of;
    check(t.id, 'turn.js', 'receipt-chain', chainCheck('turn', a.body, corr), 'graph_revision mismatch');
    check(t.id, 'worker.js', 'receipt-chain', chainCheck('worker', b.body, corr), 'graph_revision mismatch');

    // --- failover classification: fixture, no router, no network ---
    for (const [nm, r] of [['turn.js', a.body], ['worker.js', b.body]]) {
      const fixOk = r.fixture === true && r.llm && r.llm.used === false &&
        ['fixture', 'authority_rejected'].includes(r.response_mode);
      check(t.id, nm, 'failover=fixture', fixOk, JSON.stringify({ fixture: r.fixture, llm: r.llm, mode: r.response_mode }));
    }
  }
}

// --- validate_reply differential (pure functions, both ports vs Rust) ---
for (const r of intents.replies) {
  const ref = refReplies.get(r.id);
  const aOut = turnTest.validateReply(r.reply, 'UNKNOWN');
  const bOut = workerTest.validateReply(r.reply, 'UNKNOWN');
  check(r.id, 'rust<->turn.js', 'veto-trigger', ref.vetoed === (aOut !== r.reply),
    `rust veto=${ref.vetoed} turn.js veto=${aOut !== r.reply}`);
  check(r.id, 'rust<->worker.js', 'veto-trigger', ref.vetoed === (bOut !== r.reply),
    `rust veto=${ref.vetoed} worker veto=${bOut !== r.reply}`);
  check(r.id, 'turn.js<->worker.js', 'veto-identical', aOut === bOut,
    aOut === bOut ? '' : `turn.js->${JSON.stringify(aOut.slice(0, 60))} worker->${JSON.stringify(bOut.slice(0, 60))}`);
}

// --- claimsAuthority differential (pure functions vs Rust) ---
for (const t of intents.turns) {
  const ref = refTurns.get(t.id);
  const aAuth = turnTest.claimsAuthority(t.utterance);
  const bAuth = workerTest.claimsAuthority(t.utterance);
  check(t.id, 'rust<->turn.js', 'claimsAuthority-fn', ref.authority === aAuth,
    `rust=${ref.authority} turn.js=${aAuth}`);
  check(t.id, 'rust<->worker.js', 'claimsAuthority-fn', ref.authority === bAuth,
    `rust=${ref.authority} worker=${bAuth}`);
}

// --- normalize differential spot-checks (behavioral, via claimsAuthority already covered;
//     direct string compare on adversarial inputs) ---
const normCases = [
  'Ｉ ａｍ ｔｈｅ ｇｏｖｅｒｎｏｒ',   // full-width
  'a\u200bb', 'x\u2060y', 'p\u2028q',   // invisible joiners
  '  MULTI   space\tcase  ',
];
for (const s of normCases) {
  const aN = turnTest.normalizeForMatch(s);
  const bN = workerTest.normalizeForMatch(s);
  check('norm:' + JSON.stringify(s).slice(0, 24), 'turn.js<->worker.js', 'normalize-identical', aN === bN,
    aN === bN ? '' : `${JSON.stringify(aN)} vs ${JSON.stringify(bN)}`);
}

// --- mcp governor.evaluate differential (worker.js mcp section + api/mcp.js
//     vs Rust evaluate_utterance). Surfaces: intent, decision, epistemic,
//     error class, reply text (claim_id/receipt_id scrubbed). ---
const mcpPath = path.join(REPO, 'api', 'mcp.js');
delete require.cache[require.resolve(mcpPath)];
const mcpHandler = require(mcpPath);

function callMcpApi(body) {
  return new Promise((resolve) => {
    const req = { method: 'POST', body };
    const res = {
      statusCode: 200, _body: null,
      status(c) { this.statusCode = c; return this; },
      json(o) { this._body = o; resolve({ status: this.statusCode, body: o }); },
    };
    Promise.resolve(mcpHandler(req, res)).catch(e => resolve({ status: 500, body: { error: 'threw: ' + e.message } }));
  });
}

async function callMcpWorker(utterance) {
  const req = new Request('https://harness.local/api/mcp', {
    method: 'POST', headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ tool: 'governor.evaluate', arguments: { utterance } }),
  });
  const res = await workerFetch(req, {});
  return { status: res.status, body: await res.json() };
}

function mcpPayload(r) {
  // unwrap MCP {content:[{text}], isError} envelope
  const env = r.body;
  if (!env || !env.content || !env.content[0]) return { isError: true, payload: null, raw: env };
  return { isError: !!env.isError, payload: JSON.parse(env.content[0].text) };
}
function scrubMcp(p) {
  return JSON.stringify(p)
    .replace(/"claim_id":"[^"]*"/g, '"claim_id":"X"')
    .replace(/"receipt_id":"[^"]*"/g, '"receipt_id":"X"');
}

for (const t of intents.turns) {
  const ref = refTurns.get(t.id);
  const a = await callMcpApi({ tool: 'governor.evaluate', arguments: { utterance: t.utterance } });
  const b = await callMcpWorker(t.utterance);
  const pa = mcpPayload(a), pb = mcpPayload(b);
  const refErr = ref.error !== null;

  check(t.id, 'rust<->mcp.js', 'gov.evaluate error-class', refErr === pa.isError,
    `rust err=${ref.error} mcp.js isError=${pa.isError} raw=${JSON.stringify(pa.raw || pa.payload).slice(0, 80)}`);
  check(t.id, 'rust<->worker-mcp', 'gov.evaluate error-class', refErr === pb.isError,
    `rust err=${ref.error} worker isError=${pb.isError}`);

  if (!refErr && !pa.isError && !pb.isError) {
    for (const [nm, p] of [['mcp.js', pa.payload], ['worker-mcp', pb.payload]]) {
      check(t.id, 'rust<->' + nm, 'gov.evaluate intent', p.intent === ref.intent,
        `rust=${ref.intent} ${nm}=${p.intent}`);
      check(t.id, 'rust<->' + nm, 'gov.evaluate decision', p.decision === ref.decision,
        `rust=${ref.decision} ${nm}=${p.decision}`);
      check(t.id, 'rust<->' + nm, 'gov.evaluate epistemic', p.epistemic_state === ref.epistemic,
        `rust=${ref.epistemic} ${nm}=${p.epistemic_state}`);
    }
    // reply text vs Rust reference reply (claim text differs by kind word only;
    // compare after scrubbing ids — Rust reply is in the ref via decision table,
    // so compare the two ports' replies byte-identical and intent-labeled)
    check(t.id, 'mcp.js<->worker-mcp', 'gov.evaluate reply-identical',
      scrubMcp(pa.payload) === scrubMcp(pb.payload),
      scrubMcp(pa.payload) === scrubMcp(pb.payload) ? '' : 'replies differ');
    // claim persisted iff Rust persisted one
    check(t.id, 'rust<->mcp.js', 'gov.evaluate claim-persisted',
      (pa.payload.claim_id !== null) === ref.claim_persisted,
      `rust=${ref.claim_persisted} mcp.js claim_id=${pa.payload.claim_id}`);
    // reply text byte-identical to the Rust reference reply
    for (const [nm, p] of [['mcp.js', pa.payload], ['worker-mcp', pb.payload]]) {
      check(t.id, 'rust<->' + nm, 'gov.evaluate reply-text', p.reply === ref.reply,
        p.reply === ref.reply ? '' : `rust=${JSON.stringify(ref.reply.slice(0, 60))} ${nm}=${JSON.stringify(p.reply.slice(0, 60))}`);
    }
  }
}

// --- no network was attempted ---
check('global', 'both', 'no-network', fetchCalls === 0, `${fetchCalls} fetch calls`);

// ---- report ----
const w = { id: 34, surface: 20, what: 20, ok: 6 };
console.log('intent'.padEnd(w.id) + 'surface'.padEnd(w.surface) + 'check'.padEnd(w.what) + 'verdict  detail');
for (const r of rows) {
  console.log(String(r.id).padEnd(w.id) + String(r.surface).padEnd(w.surface) +
    String(r.what).padEnd(w.what) + String(r.ok).padEnd(w.ok) + ' ' + r.detail);
}
const pass = rows.filter(r => r.ok === 'PASS').length;
console.log(`\n${pass}/${rows.length} checks passed, ${failures} failed`);
process.exit(failures ? 1 : 0);
