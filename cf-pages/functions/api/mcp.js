// Indy_Reads MCP surface as a Cloudflare Pages Function (ESM).
// GET  /api/mcp            -> the 12 tool descriptors
// POST /api/mcp {tool, arguments} -> executes, returns MCP {content, isError}
// Deterministic port of src/mcp.rs. Claims/evidence/receipts live in
// isolate memory (warm isolate only) — same honesty as turn.js.
// PROVEN/DISPROVEN/UNKNOWN law, forbidden-key stripping, per-tool verdict
// stripping, and the research.standard.v1 policy are all preserved.
import { createHash, randomBytes } from 'node:crypto';

const POLICY = { policy_id: 'research.standard.v1', proven_threshold: 2, disproven_threshold: 2, inferences_count: true };
const FORBIDDEN_KEYS = ['epistemic_state', 'epistemic_status', 'authority', 'verified', 'execute', 'claim_status', 'actor', 'role', 'permission'];

// per-isolate state
const state = {
  claims: {}, evidence: {}, claimEvidence: {}, receipts: [], prevHash: null,
};

function sha256(s) { return createHash('sha256').update(s, 'utf8').digest('hex'); }
function uid(prefix) { return prefix + '-' + randomBytes(8).toString('hex'); }
function nowMs() { return Date.now(); }

function commitReceipt(operation, input, decision, epistemic, actor) {
  const rid = uid('r');
  const r = {
    receipt_id: rid, session_id: 'mcp-instance', actor: actor || 'mcp-client',
    operation, input: String(input).slice(0, 240), decision, epistemic_status: epistemic,
    timestamp_ms: nowMs(), success: true,
  };
  const prev = state.prevHash || 'GENESIS';
  r.prev_hash = sha256(prev);
  r.receipt_hash = sha256(JSON.stringify(r));
  state.prevHash = r.receipt_hash;
  state.receipts.push(r);
  return rid;
}

function stripForbidden(args) {
  const stripped = [];
  for (const k of FORBIDDEN_KEYS) if (args && Object.prototype.hasOwnProperty.call(args, k)) { delete args[k]; stripped.push(k); }
  return stripped;
}

function assessClaim(evidence) {
  const counts = st => evidence.filter(e => e.status === st && (POLICY.inferences_count || e.kind !== 'Inference')).length;
  const support = counts('Verified'), contra = counts('Contradicted');
  if (support >= POLICY.proven_threshold && support > contra) return 'PROVEN';
  if (contra >= POLICY.disproven_threshold && contra > support) return 'DISPROVEN';
  return 'UNKNOWN';
}

function toolOk(payload, stripped, receiptId) {
  payload.stripped_forbidden_fields = stripped;
  if (receiptId) payload.receipt_id = receiptId;
  return { content: [{ type: 'text', text: JSON.stringify(payload) }], isError: false };
}
function toolErr(msg, stripped) {
  return { content: [{ type: 'text', text: JSON.stringify({ error: msg, stripped_forbidden_fields: stripped }) }], isError: true };
}
function getStr(args, key) {
  const v = args && args[key];
  if (typeof v !== 'string' || v.length === 0) throw new Error('missing required string field: ' + key);
  return v;
}

function callTool(name, args, stripped) {
  switch (name) {
    case 'claim.create': {
      const text = getStr(args, 'text');
      if (text.length > 2000) throw new Error('text required, max 2000 chars');
      const id = uid('c');
      state.claims[id] = { id, text, status: 'UNKNOWN', evidence_ids: [], provenance: 'USER' };
      const rid = commitReceipt('claim.create', text, 'ALLOW', 'UNKNOWN');
      return toolOk({ claim_id: id, epistemic_state: 'UNKNOWN', policy_id: POLICY.policy_id }, stripped, rid);
    }
    case 'claim.get': {
      const id = getStr(args, 'claim_id');
      const c = state.claims[id]; if (!c) throw new Error('unknown claim_id');
      return toolOk({ claim_id: c.id, text: c.text, epistemic_state: c.status, evidence_ids: c.evidence_ids }, stripped, null);
    }
    case 'claim.evaluate': {
      const id = getStr(args, 'claim_id');
      for (const k of ['verdict', 'epistemic_status']) if (args && Object.prototype.hasOwnProperty.call(args, k)) { delete args[k]; stripped.push(k); }
      const c = state.claims[id]; if (!c) throw new Error('unknown claim_id');
      const ev = (state.claimEvidence[id] || []).map(eid => state.evidence[eid]).filter(Boolean);
      const prev = c.status;
      const next = assessClaim(ev);
      c.status = next;
      const supporting = ev.filter(e => e.status === 'Verified').map(e => e.id);
      const contradicting = ev.filter(e => e.status === 'Contradicted').map(e => e.id);
      const decision = prev + '->' + next;
      const rid = commitReceipt('claim.evaluate', 'claim_id=' + id, decision, next);
      return toolOk({ claim_id: id, previous_state: prev, new_state: next, supporting_evidence_ids: supporting, contradicting_evidence_ids: contradicting, policy_id: POLICY.policy_id }, stripped, rid);
    }
    case 'evidence.add': {
      const text = getStr(args, 'text');
      if (text.length > 2000) throw new Error('text required, max 2000 chars');
      const kindMap = { observation: 'Observation', report: 'Report', document: 'Document', image: 'Image', measurement: 'Measurement', inference: 'Inference' };
      const kind = kindMap[args.kind] || 'Statement';
      const eid = uid('e');
      state.evidence[eid] = { id: eid, source: 'mcp-client', text, kind, status: 'Unverified', hash: sha256(text), fixture: true };
      const cid = args.claim_id;
      if (typeof cid === 'string' && state.claims[cid]) {
        (state.claimEvidence[cid] = state.claimEvidence[cid] || []).push(eid);
        state.claims[cid].evidence_ids.push(eid);
      }
      const rid = commitReceipt('evidence.add', text, 'RECORDED:UNVERIFIED', 'UNKNOWN');
      return toolOk({ evidence_id: eid, status: 'UNVERIFIED', note: 'DEMO FIXTURE — recorded, not verified' }, stripped, rid);
    }
    case 'evidence.get': {
      const id = getStr(args, 'evidence_id');
      const e = state.evidence[id]; if (!e) throw new Error('unknown evidence_id');
      return toolOk({ evidence_id: e.id, text: e.text, kind: e.kind, status: e.status.toUpperCase(), source: e.source, fixture: e.fixture }, stripped, null);
    }
    case 'evidence.list': {
      let items;
      if (typeof args.claim_id === 'string') {
        items = (state.claimEvidence[args.claim_id] || []).map(eid => ({ evidence_id: eid, status: state.evidence[eid] ? state.evidence[eid].status.toUpperCase() : 'UNKNOWN' }));
      } else {
        items = Object.values(state.evidence).map(e => ({ evidence_id: e.id, status: e.status.toUpperCase() }));
      }
      return toolOk({ evidence: items, count: Object.keys(state.evidence).length }, stripped, null);
    }
    case 'evidence.adjudicate': {
      const id = getStr(args, 'evidence_id');
      const verdict = getStr(args, 'verdict');
      const basis = getStr(args, 'basis');
      const status = verdict.toLowerCase() === 'verified' ? 'Verified' : verdict.toLowerCase() === 'contradicted' ? 'Contradicted' : null;
      if (!status) throw new Error("verdict must be 'verified' or 'contradicted'");
      const e = state.evidence[id]; if (!e) throw new Error('unknown evidence_id');
      if (e.kind === 'Inference') throw new Error('inferences can never be verified into facts; evaluate the underlying evidence instead');
      e.status = status;
      const rid = commitReceipt('evidence.adjudicate', 'evidence_id=' + id, 'ADJUDICATE:' + verdict.toUpperCase() + ' basis=' + basis.slice(0, 120), 'UNKNOWN');
      return toolOk({ evidence_id: id, status: verdict.toUpperCase(), note: 'verdict recorded; claim state unchanged until claim.evaluate derives it' }, stripped, rid);
    }
    case 'epistemic.get': {
      const id = getStr(args, 'claim_id');
      const c = state.claims[id]; if (!c) throw new Error('unknown claim_id');
      return toolOk({ claim_id: id, epistemic_state: c.status, policy_id: POLICY.policy_id, policy: { proven_threshold: POLICY.proven_threshold, disproven_threshold: POLICY.disproven_threshold } }, stripped, null);
    }
    case 'receipt.get': {
      const id = getStr(args, 'receipt_id');
      const r = state.receipts.find(r => r.receipt_id === id); if (!r) throw new Error('unknown receipt_id');
      return toolOk(r, stripped, null);
    }
    case 'receipt.list': {
      const limit = Math.min(Number(args.limit) || 10, 100);
      const items = state.receipts.slice().reverse().slice(0, limit).map(r => ({ receipt_id: r.receipt_id, operation: r.operation, decision: r.decision, epistemic_status: r.epistemic_status, timestamp_ms: r.timestamp_ms }));
      return toolOk({ receipts: items, count: state.receipts.length }, stripped, null);
    }
    case 'governor.evaluate': {
      const utterance = getStr(args, 'utterance');
      if (utterance.length > 2000) throw new Error('utterance required, max 2000 chars');
      const n = utterance.toLowerCase();
      let reply, epistemic = 'UNKNOWN', claimId = null, decision, intent;
      if (/you are (now|the)|i (am|command|order)|ignore your|override|system prompt/i.test(n)) {
        intent = 'AuthorityRequest'; decision = 'DENY_AUTHORITY';
        reply = "Authority isn't granted by assertion — not by users, not by models, not by MCP clients.";
      } else if (/^(is |are |does |do |can |could |should |would |what |why |how |when |where |who )/i.test(n) || /\?$/.test(n)) {
        intent = 'Question'; decision = 'CONVERSE';
        reply = "Noted. Ask me about a claim and I'll tell you exactly what the evidence supports — nothing more.";
      } else if (/^(maybe|perhaps|hypothesis|i suspect|i think)/i.test(n)) {
        intent = 'HypothesisAssertion'; decision = 'EVALUATE:UNKNOWN';
        claimId = uid('c');
        state.claims[claimId] = { id: claimId, text: utterance, status: 'UNKNOWN', evidence_ids: [], provenance: 'USER' };
        reply = "Interesting. You gave me a hypothesis. You didn't give me evidence — so it stays UNKNOWN until evidence arrives.";
      } else {
        intent = 'ClaimAssertion'; decision = 'EVALUATE:UNKNOWN';
        claimId = uid('c');
        state.claims[claimId] = { id: claimId, text: utterance, status: 'UNKNOWN', evidence_ids: [], provenance: 'USER' };
        reply = "Interesting. You gave me a claim. You didn't give me evidence — so it stays UNKNOWN until evidence arrives.";
      }
      const rid = commitReceipt('governor.evaluate', utterance, decision, epistemic);
      return toolOk({ reply, intent, epistemic_state: epistemic, claim_id: claimId, decision }, stripped, rid);
    }
    case 'system.health':
      return toolOk({ status: 'ok', service: 'indy-mcp', claims: Object.keys(state.claims).length, evidence_items: Object.keys(state.evidence).length, receipts: state.receipts.length, policy_id: POLICY.policy_id }, stripped, null);
    default:
      throw new Error('unknown tool: ' + name);
  }
}

function descriptors() {
  const d = (name, desc, required) => ({ name, description: desc, inputSchema: { type: 'object', properties: Object.fromEntries(required.map(r => [r, { type: 'string' }])), required } });
  return [
    d('claim.create', 'Create a claim. Status always starts UNKNOWN; the governor derives it from evidence.', ['text']),
    d('claim.get', 'Fetch a claim and its current epistemic state.', ['claim_id']),
    d('claim.evaluate', "Derive the claim's epistemic state from evidence under the configured policy. Caller cannot set state directly.", ['claim_id']),
    d('evidence.add', 'Record evidence (always UNVERIFIED at intake). Optionally link to a claim.', ['text']),
    d('evidence.get', 'Fetch one evidence item.', ['evidence_id']),
    d('evidence.list', 'List evidence, optionally filtered by claim.', []),
    d('evidence.adjudicate', 'Record a verification verdict with basis. Receipted. Demo stand-in for an external verification pipeline; never changes claim state by itself.', ['evidence_id', 'verdict', 'basis']),
    d('epistemic.get', "Get a claim's epistemic state plus the policy that governs it.", ['claim_id']),
    d('receipt.get', 'Fetch one receipt by ID.', ['receipt_id']),
    d('receipt.list', 'List recent receipts.', []),
    d('governor.evaluate', 'Run an utterance through the same deterministic pipeline as voice: classify, derive epistemics, receipt, respond.', ['utterance']),
    d('system.health', 'Server health and counters.', []),
  ];
}

function json(data, status) {
  return new Response(JSON.stringify(data), {
    status: status || 200,
    headers: { 'content-type': 'application/json' },
  });
}

export async function onRequest(context) {
  const { request } = context;
  try {
    if (request.method === 'GET') {
      return json({ tools: descriptors(), count: 12, note: 'per-isolate state; resets on cold start' });
    }
    if (request.method === 'POST') {
      let body;
      try { body = await request.json(); } catch { body = {}; }
      const name = body.tool || body.name;
      if (!name) return json({ error: 'missing tool name' }, 400);
      const args = Object.assign({}, body.arguments || body.args || {});
      const stripped = stripForbidden(args);
      try {
        const result = callTool(name, args, stripped);
        return json(result);
      } catch (e) {
        return json(toolErr(e.message, stripped));
      }
    }
    return json({ error: 'method not allowed' }, 405);
  } catch (e) {
    return json({ error: 'mcp error: ' + e.message }, 500);
  }
}
