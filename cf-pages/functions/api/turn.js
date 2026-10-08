// Indy_Reads demo backend — Cloudflare Pages Function (ESM).
// Deterministic fixture + governor, ported from the Vercel serverless version
// (api/turn.js), which was ported from the Rust core (src/ai.rs, src/governor.rs, src/voice.rs).
// The model proposes language; these functions own truth. No LLM in the loop.
//
// Router key comes from a Pages secret: env.ROUTER_KEY. No bundled-file fallback.
import { createHash, randomBytes } from 'node:crypto';

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

export async function onRequest(context) {
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
