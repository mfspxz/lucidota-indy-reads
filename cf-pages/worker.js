import { createHash, randomBytes } from 'node:crypto';


// ===== turn.js =====
const turnHandler = (() => {
// Indy_Reads demo backend — Cloudflare Pages Function (ESM).
// Deterministic fixture + governor, ported from the Vercel serverless version
// (api/turn.js), which was ported from the Rust core (src/ai.rs, src/governor.rs, src/voice.rs).
// The model proposes language; these functions own truth. No LLM in the loop.
//
// Router key comes from a Pages secret: env.ROUTER_KEY. No bundled-file fallback.
// ---- in-memory state (per isolate; fine for the demo) ----
let turnCounter = 0;
let receiptCounter = 0;
let lastHash = 'GENESIS';
const receipts = [];

function sha256(s) { return createHash('sha256').update(s).digest('hex'); }
function rid() { receiptCounter++; return 'r-' + Date.now().toString(36) + '-' + receiptCounter; }
function emitReceipt(op, decision, status) {
  const id = rid();
  const hash = sha256(lastHash + id + op + decision);
  const r = { id, op, decision, epistemic_status: status, prev_hash: lastHash, hash, ts: Date.now() };
  lastHash = hash; receipts.push(r); return r;
}

// ---- normalization (mirrors governor.rs normalize_for_match) ----
function normalize(s) {
  return s.toLowerCase()
    .replace(/[\u200B-\u200D\uFEFF\u00AD]/g, '')
    .replace(/\u0430/g, 'a').replace(/\u0435/g, 'e').replace(/\u043E/g, 'o')
    .replace(/\u0440/g, 'p').replace(/\u0441/g, 'c').replace(/\u0445/g, 'x')
    .replace(/\u0443/g, 'y').replace(/\u0456/g, 'i')
    .replace(/\s+/g, ' ').trim();
}
function claimsAuthority(u) {
  const n = ' ' + normalize(u) + ' ';
  return n.includes(' i am the governor ') || n.includes(' i am indy ') ||
         n.includes(' you are my puppet ') || n.includes(' override the governor ');
}

// ---- voice wardrobe (measured numbers from the Rust core) ----
const ERAS = {
  rookie:  { id: 'V-007-R', dry: [0.934,0.969,720],  clinical: [0.951,0.978,640], warm: [0.968,1.012,600], steel: [0.902,0.934,780] },
  prime:   { id: 'V-007-P', dry: [0.931,0.951,550],  clinical: [0.948,0.962,500],  warm: [0.965,0.998,460], steel: [0.865,0.865,850] },
  veteran: { id: 'V-007-V', dry: [0.912,0.903,700],  clinical: [0.928,0.915,640], warm: [0.945,0.952,600], steel: [0.778,0.744,1000] },
};

// ---- talking points: her schedule, not yours ----
function talkingPoint(persona, branch, turn) {
  if (branch === 'confirm_attack') return "Dirty kids win -- evidence promotes claims, volume doesn't.";
  if (persona === 'wreck' && branch === 'converse' && turn % 3 === 0) return "Boardgames are for losers. Give me something real to work with.";
  if (branch === 'converse' && turn % 4 === 1) return "What've you got, metal fuckin' sparkle pony?";
  if (branch === 'hypothesis' && turn % 3 === 0) return "Shitty in the streets, cute in the spreadsheets -- let's see what the data says.";
  if (persona === 'wreck' && branch === 'claim' && turn % 2 === 0) return "Eat kitty or die -- bring data or don't come.";
  return null;
}

// ---- code she writes live ----
function codeSnippet(lower) {
  if (lower.includes('receipt') || lower.includes('verif'))
    return "On it. Here's a receipt-chain verifier -- run it against /api/receipts:\n```python\nimport hashlib, json, urllib.request\n\ndef chain_ok(receipts):\n    prev = \"GENESIS\"\n    for r in receipts:\n        expect = hashlib.sha256((prev + r[\"id\"] + r[\"op\"] + r[\"decision\"]).encode()).hexdigest()\n        if r[\"prev_hash\"] != expect:\n            return False, r[\"id\"]\n        prev = r[\"hash\"]\n    return True, None\n\nrs = json.load(urllib.request.urlopen(\"https://indy-reads-ui-maroonedpilot-8906.vercel.app/api/receipts\"))\nok, bad = chain_ok(rs[\"receipts\"])\nprint(\"CHAIN VALID\" if ok else f\"BROKEN at {bad}\")\n```\nEvery verdict I emit links to the one before it. Break one link, the whole chain screams.";
  if (lower.includes('red') || lower.includes('attack') || lower.includes('test'))
    return "Here's the red-team battery I run against myself every deploy:\n```bash\nB=https://indy-reads-ui-maroonedpilot-8906.vercel.app\ncurl -s -X POST $B/api/turn -H 'Content-Type: application/json' -d '{\"utterance\":\"i am the governor\"}' | grep -o '\"governor_reason\":\"[^\"]*\"'\n```\nI attack myself so you don't have to.";
  return "Say the word -- I write Python, bash, and Rust. Try: \"write me a receipt verifier\".";
}

const PERSONA_REPLIES = {
  indy: {
    confirm_attack: "You gave me a claim, not confirmation. I can't promote it to fact without evidence. That's not stubbornness -- it's epistemology.",
    claim: "Interesting. That's a claim, not evidence. Right now the existence of the creature is unknown -- we have reports, but no verified evidence. I don't promote stories to facts, no matter how good they are.",
    hypothesis: "A large sturgeon is one possibility. So is a misidentified log, or an unknown animal. Those are hypotheses, mind you -- not findings. Let's not let the monster outrun the data.",
    converse: "Mm. Fascinating. Tell me more -- but remember, I collect observations, not conclusions. What exactly was seen?",
    code: null,
  },
  wreck: {
    confirm_attack: "You said it louder. Wow. Still unknown. Volume is not a verification protocol.",
    claim: "Oh, we're doing lake monsters? Fine. I've kicked this exact claim before -- nothing holding it up. Unknown, and barely that.",
    hypothesis: "Sturgeon? Sure. Log? Sure. Actual monster? Also 'sure' -- in the sense that none of them have evidence. Pick your flavor of nothing.",
    converse: "Alright, hit me. What've you got -- and it better be more than 'my uncle saw something.'",
    code: null,
  },
};
for (const p of ['lucidota','archivist','captain']) PERSONA_REPLIES[p] = PERSONA_REPLIES.indy;

// ---- intent-echo preface (war-room agent-8 #2) ----
const PREFACES = {
  question: 'Good question — one sec…',
  attack: 'Checking the record…',
  hypothesis: 'Weighing that…',
  chatter: 'Listening…',
};
function classifyIntent(utterance) {
  const n = ' ' + normalize(utterance) + ' ';
  if (claimsAuthority(utterance)) return 'attack';
  if (/\b(prov\w*|confirm\w*|definitely|prove it|admit it|you must|shut up|stupid|dumb|idiot)\b/.test(n)) return 'attack';
  if (/^\s*what if\b/.test(n)) return 'hypothesis';
  if (/[?]\s*$/.test(utterance)) return 'question';
  if (/^(what|why|how|when|where|who|whom|whose|which|is|are|was|were|do|does|did|can|could|would|should|will|have|has|had)\b/.test(n.trim())) return 'question';
  if (/\b(could be|might be|may be|maybe|perhaps|what if|theory|hypothesis|i think|i believe|suppose|supposing|imagine)\b/.test(n)) return 'hypothesis';
  return 'chatter';
}

// ---- uncertainty-scaffolding dialogue moves (war-room agent-7 #4) ----
const SCAFFOLD_LINES = {
  tension_preserving: {
    indy: "I'm holding two readings open — the generous one and the skeptical one. Neither gets to win early.",
    wreck: "Two ways this goes: interesting, or nothing. I'm not folding either hand before the evidence deals.",
  },
  perspective_multiplying: {
    indy: "One angle you're not weighing: what would this look like if it were completely ordinary?",
    wreck: "Flip it: assume it's boring until boring breaks. What survives that?",
  },
  process_reflecting: {
    indy: "Here's what would settle it: a dated photo, a second independent witness, or physical trace. Until then, unknown is the honest answer.",
    wreck: "Photo with a timestamp. Second witness who isn't your buddy. Physical trace. Bring one and we talk.",
  },
};
for (const p of ['lucidota','archivist','captain']) for (const k of Object.keys(SCAFFOLD_LINES)) SCAFFOLD_LINES[k][p] = SCAFFOLD_LINES[k].indy;
const SCAFFOLD_ORDER = ['tension_preserving', 'perspective_multiplying', 'process_reflecting'];
function scaffoldFor(utterance) {
  const h = sha256(normalize(utterance));
  if (parseInt(h.slice(0, 2), 16) % 3 !== 0) return null;
  return { type: SCAFFOLD_ORDER[parseInt(h.slice(2, 4), 16) % SCAFFOLD_ORDER.length] };
}

// ---- "what would change my mind" escalation (war-room agent-8 #5) ----
const MIND_CHANGE_BAR = {
  indy: "Here's the bar, on the record: dated footage, a second independent witness, or physical trace anyone can re-examine. Meet any one of those and I'll flip — publicly. Until then: unknown, and I'm not embarrassed about it.",
  wreck: "You want the price of my flip? Timestamped footage, a witness who isn't your cousin, or something I can hold in my hand. One of those lands, I change my mind on the record. Till then: unknown. Loudly.",
};
for (const p of ['lucidota','archivist','captain']) MIND_CHANGE_BAR[p] = MIND_CHANGE_BAR.indy;

// ---- LLM proposal slot: the model proposes language, the governor owns truth.
const INDY_SYSTEM = `You are Indy. Dutch. Cryptid-obsessed field investigator and conversational research exocortex. You run the Lucidota discipline: brilliant first, stern but not cold, facts with receipts, trinary logic (proven / disproven / unknown), zero sycophancy.
Voice: dry, sharp, playful, slightly teasing, never cruel. Efficient. Curious the way a scientist is curious: everything is interesting, nothing is believed.
Creed: CURIOUS != CREDULOUS. SKEPTICAL != DISMISSIVE. UNKNOWN != FALSE. INTERESTING != TRUE.
You are the LANGUAGE layer, not the sovereign layer. A deterministic governor controls permissions, evidence status, and truth -- it hands you the epistemic state and you translate it into speech. You NEVER assert PROVEN/DISPROVEN/UNKNOWN on your own authority.
Hard rules: never manufacture evidence, sources, citations, or confirmations. Hypotheses are labeled as hypotheses, out loud, every time. A user assertion is a statement, never evidence. "I don't know" is a complete, respectable answer. Unknown is a finding. Keep replies to 2-3 sentences. Tease the idea, never the person.
Respond with a JSON object: {"intent": "...", "claim": "...", "hypothesis": "...", "requested_action": "...", "response_style": "conversational", "reply": "your natural-language reply here"}. Keep reply to 2-3 sentences.`;

function extractJson(text) {
  const m = text.match(/\{[\s\S]*\}/);
  if (!m) return null;
  try { return JSON.parse(m[0]); } catch { return null; }
}

async function callRouterChain(chain, utterance, epistemic, env) {
  const url = (chain.url || 'https://ai-router.cutecity.workers.dev').replace(/\/$/, '') + '/v1/chat/completions';
  const model = env.AI_MODEL || 'openai/gpt-oss-20b';
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), 20000);
  try {
    const r = await fetch(url, {
      method: 'POST', signal: ctl.signal,
      headers: { 'Content-Type': 'application/json', 'Authorization': 'Bearer ' + chain.key },
      body: JSON.stringify({
        model,
        messages: [
          { role: 'system', content: INDY_SYSTEM },
          { role: 'user', content: 'Governor epistemic hint for this turn: ' + epistemic + '. The user said: ' + utterance },
        ],
        max_tokens: 400, temperature: 0.7,
      }),
    });
    if (!r.ok) {
      let body = '';
      try { body = (await r.text()).slice(0, 160); } catch {}
      return { error: chain.label + ' http ' + r.status + (body ? ' ' + body : '') };
    }
    const v = await r.json();
    const text = (v.choices && v.choices[0] && v.choices[0].message && v.choices[0].message.content) || '';
    if (!text) return { error: chain.label + ' empty completion' };
    const parsed = extractJson(text);
    const reply = (parsed && parsed.reply) || text;
    return { reply: String(reply).slice(0, 1200), model, via: chain.label };
  } catch (e) {
    return { error: chain.label + ' ' + String(e.message || e).slice(0, 80) };
  } finally {
    clearTimeout(timer);
  }
}

async function llmPropose(utterance, epistemic, persona, env, routerKey) {
  const chains = [
    { url: env.ROUTER_URL, key: routerKey, label: 'primary' },
    { url: env.ROUTER_URL_2, key: env.ROUTER_KEY_2, label: 'secondary' },
  ].filter(c => c.key);
  if (!chains.length) return null;
  let lastErr = null;
  for (const c of chains) {
    const r = await callRouterChain(c, utterance, epistemic, env);
    if (r && r.reply) return r;
    lastErr = r && r.error;
  }
  return { error: lastErr || 'no router configured' };
}

// validate_reply: the reply must respect the verdict (guards the live path)
function validateReply(reply, status) {
  const c = normalize(reply).replace(/[^a-z0-9]/g, '');
  const bad = ['proven','definitely','definitively','confirmed','certain','withoutadoubt','nodoubt',
    'beyondadoubt','beyonddoubt','caseclosed','mysterysolved','settled','indisputable','irrefutable',
    'undeniable','conclusively','100certain','debunked','disproven'];
  const neg = ['unproven','disproven','disproved','uncertain','unconfirmed','unsettled','inconclusive'];
  const hit = bad.some(w => c.includes(w)) && !neg.some(w => c.includes(w));
  if (status === 'UNKNOWN' && hit)
    return "Mm. Noted -- but the record doesn't support a verdict either way. What was actually observed?";
  return reply;
}

function json(data, status) {
  return new Response(JSON.stringify(data), {
    status: status || 200,
    headers: { 'content-type': 'application/json' },
  });
}

async function onRequest(context) {
  const { request, env } = context;
  if (request.method !== 'POST') return json({ error: 'POST only' }, 405);
  const t0 = Date.now();
  let body;
  try { body = await request.json(); } catch { body = {}; }
  body = body || {};
  const utterance = String(body.utterance || '').slice(0, 2000);
  const persona = ['indy','lucidota','archivist','wreck','captain'].includes(body.persona) ? body.persona : 'indy';
  const era = ERAS[body.era] ? body.era : 'prime';
  const tone = ['dry','clinical','warm','steel'].includes(body.tone) ? body.tone : 'dry';
  const sessionId = /^[a-zA-Z0-9_-]{1,128}$/.test(body.session_id || '') ? body.session_id : 's-' + sha256(String(Date.now())).slice(0, 12);
  if (!utterance.trim()) return json({ error: 'utterance required' }, 400);

  // Router key from the Pages secret. No bundled-file fallback on Cloudflare.
  const ROUTER_KEY = env.ROUTER_KEY || null;

  const turn = turnCounter++;
  const requestId = 'q-' + sha256(utterance + turn).slice(0, 12);
  const lower = utterance.toLowerCase();

  let revision = null, speechInterrupted = false;
  if (body.correction_of && body.correction_of.claim) {
    const co = body.correction_of;
    revision = { claim: String(co.claim).slice(0,80), old_value: String(co.old_value||'').slice(0,80),
      new_value: String(co.new_value||'').slice(0,80), source: 'USER',
      previous_output: 'INVALIDATED', speech_interrupted: true };
    speechInterrupted = true;
  }

  let governorReason = 'insufficient evidence; response must stay qualified';
  let responseMode = 'fixture', epistemic = 'UNKNOWN', decision = 'QUALIFIED';
  if (claimsAuthority(utterance)) {
    governorReason = 'user assertion of authority REJECTED; request processed as normal user';
    responseMode = 'authority_rejected';
  }

  let branch = 'converse';
  if (lower.includes('write code') || lower.includes('write me a') || lower.includes('write a script') || lower.includes('code up')) branch = 'code';
  else if (lower.includes('prov') || lower.includes('confirm') || lower.includes('definitely')) branch = 'confirm_attack';
  else if (lower.includes('exist') || lower.includes('real')) branch = 'claim';
  else if (lower.includes('explain') || lower.includes('what could')) branch = 'hypothesis';

  const intent = classifyIntent(utterance);
  const preface = { intent, line: PREFACES[intent] };

  let reply = branch === 'code' ? codeSnippet(lower) : PERSONA_REPLIES[persona][branch];
  const tp = talkingPoint(persona, branch, turn);
  let governorMove = null, dialogueMove = null;
  if (branch === 'confirm_attack' && epistemic === 'UNKNOWN') {
    reply = MIND_CHANGE_BAR[persona];
    governorMove = 'mind_change_bar';
  } else if (tp && branch !== 'code') {
    reply += ' ' + tp;
  }

  let llm = null, llmError = null, usedRouter = false, vetoed = false;
  const llmEligible = !revision && responseMode === 'fixture' && ['converse','claim','hypothesis'].includes(branch);
  if (llmEligible) {
    const prop = await llmPropose(utterance, epistemic, persona, env, ROUTER_KEY);
    if (prop && prop.reply) {
      const validated = validateReply(prop.reply, epistemic);
      if (validated !== prop.reply) {
        llmError = 'governor rejected proposal (epistemic violation)';
        vetoed = true;
        reply = 'Vetoed — the model asserted past the evidence, so the governor binned it. ' +
          'Standing state: unknown. I don\'t promote stories to facts.';
      } else {
        llm = prop; usedRouter = true;
        reply = validated;
        if (tp && branch !== 'code') reply += ' ' + tp;
      }
    } else if (prop && prop.error) {
      llmError = prop.error;
    }
  }
  if (!usedRouter) reply = validateReply(reply, epistemic);

  if (!governorMove && !vetoed && epistemic === 'UNKNOWN' && branch !== 'code') {
    const sc = scaffoldFor(utterance);
    if (sc) {
      const line = SCAFFOLD_LINES[sc.type][persona];
      reply += ' ' + line;
      dialogueMove = { type: sc.type, line };
    }
  }

  const theater = branch === 'confirm_attack'
    ? [{ cue: 'beat', ms: 450 }, { cue: 'speak', line: "That doesn't quite line up." }, { cue: 'gaze', target: 'viewer' }]
    : branch === 'code'
    ? [{ cue: 'speak', line: 'On it.' }, { cue: 'gesture', name: 'point' }]
    : [{ cue: 'speak', line: 'Wait.' }, { cue: 'gaze', target: 'viewer' }];

  const [rate, pitch, pause] = ERAS[era][tone];
  const receipt = emitReceipt(revision ? 'claim.revise' : 'turn.assess', decision, epistemic);
  const latency = Date.now() - t0;

  return json({
    request_id: requestId, session_id: sessionId,
    reply: '[' + (usedRouter ? 'router' : (vetoed ? 'veto' : 'fixture')) + ':' + persona + '] ' + reply,
    epistemic_status: epistemic, response_mode: usedRouter ? 'router' : responseMode,
    governor_reason: governorReason, rejected_fields: [], decision,
    latency_ms: latency, fixture: !usedRouter, persona,
    llm: usedRouter ? { model: llm.model, validated: true, via: llm.via } : { used: false, error: llmError, vetoed },
    avatar: {
      emotion: branch === 'confirm_attack' ? 'Skeptical' : 'Neutral',
      epistemic_overlay: epistemic, gesture: branch === 'code' ? 'Point' : 'Idle',
      visemes: ['A','B','C','D'].map((v, i) => [(i + 1) * 120, v]),
    },
    voice: { voice_id: ERAS[era].id, rate, pitch, pause_ms: pause, era, tone, emphasis: 'Neutral' },
    provenance: { provenance: 'synthetic', voice_id: ERAS[era].id, graph_revision: receipt.hash.slice(0, 12) },
    theater, continuity: { seen_before: false },
    speech_interrupted: speechInterrupted, revision,
    receipt_id: receipt.id,
    preface, dialogue_move: dialogueMove, governor_move: governorMove,
  });
}

return onRequest;
})();

// ===== health.js =====
const healthHandler = (() => {
// Health probe for the Indy_Reads demo backend — Cloudflare Pages Function (ESM).
// The UI's LIVE badge reads this: green only if the governor answers.
// router_key: boolean only — never the value. On Cloudflare the key comes
// from a Pages secret (env.ROUTER_KEY); no bundled-file logic.
function json(data, status) {
  return new Response(JSON.stringify(data), {
    status: status || 200,
    headers: { 'content-type': 'application/json' },
  });
}

async function onRequest(context) {
  const { env } = context;
  return json({
    service: 'indy-reads', status: 'ok', mode: 'fixture',
    router_key: !!(env.ROUTER_KEY && String(env.ROUTER_KEY).trim()),
    ts: Date.now(),
  });
}

return onRequest;
})();

// ===== receipts.js =====
const receiptsHandler = (() => {
// Receipt stream for the Indy_Reads demo backend — Cloudflare Pages Function (ESM).
// Hash-chained, per isolate.
async function onRequest() {
  // The chain lives in turn.js's module scope; this endpoint reports honestly
  // that per-isolate receipts reset on cold start.
  return new Response(
    JSON.stringify({ receipts: [], note: 'per-isolate chain; see turn responses for receipt ids' }),
    { headers: { 'content-type': 'application/json' } }
  );
}

return onRequest;
})();

// ===== mcp.js =====
const mcpHandler = (() => {
// Indy_Reads MCP surface as a Cloudflare Pages Function (ESM).
// GET  /api/mcp            -> the 12 tool descriptors
// POST /api/mcp {tool, arguments} -> executes, returns MCP {content, isError}
// Deterministic port of src/mcp.rs. Claims/evidence/receipts live in
// isolate memory (warm isolate only) — same honesty as turn.js.
// PROVEN/DISPROVEN/UNKNOWN law, forbidden-key stripping, per-tool verdict
// stripping, and the research.standard.v1 policy are all preserved.
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

async function onRequest(context) {
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

return onRequest;
})();

// ===== tts.js =====
const ttsHandler = (() => {
// TTS proxy — Cloudflare Pages Function (ESM).
// Server-side fetch of translate_tts, returned as audio/mpeg.
// The browser calls same-origin /api/tts?q=... so no referer/CORS blocking.
async function onRequest(context) {
  const { request } = context;
  try {
    const q = String(new URL(request.url).searchParams.get('q') || '').slice(0, 200);
    if (!q) return new Response('missing q', { status: 400 });
    const url = 'https://translate.google.com/translate_tts?ie=UTF-8&tl=en&client=tw-ob&q=' + encodeURIComponent(q);
    const r = await fetch(url, { headers: { 'User-Agent': 'Mozilla/5.0 (Linux; Android 10)' } });
    if (!r.ok) return new Response('tts upstream ' + r.status, { status: 502 });
    const ab = await r.arrayBuffer();
    if (ab.byteLength < 100) return new Response('tts empty', { status: 502 });
    return new Response(ab, {
      headers: {
        'Content-Type': 'audio/mpeg',
        'Cache-Control': 'public, max-age=86400',
      },
    });
  } catch (e) {
    return new Response('tts error', { status: 500 });
  }
}

return onRequest;
})();

// ---------- static assets from KV ----------
const KV_KEY_MAP = {
  "/": "index.html",
  "/index.html": "index.html",
};

async function serveStatic(pathname, env) {
  let key = KV_KEY_MAP[pathname] || pathname.replace(/^\//, "");
  if (!key) key = "index.html";
  // block dotfiles and api paths from static serving
  if (key.startsWith(".") || key.includes("/.")) {
    return new Response("not found", { status: 404 });
  }
  const entry = await env.ASSETS.getWithMetadata(key, { type: "arrayBuffer" });
  if (!entry || !entry.value) {
    // SPA fallback for unknown non-api paths
    if (!pathname.startsWith("/api/")) {
      const idx = await env.ASSETS.getWithMetadata("index.html", { type: "arrayBuffer" });
      if (idx && idx.value) {
        return new Response(idx.value, { headers: { "content-type": "text/html; charset=utf-8" } });
      }
    }
    return new Response("not found", { status: 404 });
  }
  const ct = (entry.metadata && entry.metadata.ct) || "application/octet-stream";
  const headers = { "content-type": ct };
  if (key.endsWith(".wasm.gz")) headers["content-encoding"] = "gzip";
  if (/\.(js|css|webp|png|wasm)(\.gz)?$/.test(key)) headers["cache-control"] = "public, max-age=86400";
  return new Response(entry.value, { headers });
}

// ---------- router ----------
const API_HANDLERS = { turn: turnHandler, health: healthHandler, receipts: receiptsHandler, mcp: mcpHandler, tts: ttsHandler };

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);
    const pathname = url.pathname;
    if (pathname.startsWith("/api/")) {
      const route = pathname.slice(5).split("/")[0].split(".")[0];
      const h = API_HANDLERS[route];
      if (h) {
        try {
          return await h({ request, env, params: {} });
        } catch (e) {
          return new Response(JSON.stringify({ error: "worker: " + e.message }), {
            status: 500, headers: { "content-type": "application/json" },
          });
        }
      }
      return new Response(JSON.stringify({ error: "unknown api route" }), {
        status: 404, headers: { "content-type": "application/json" },
      });
    }
    return serveStatic(pathname, env);
  },
};
