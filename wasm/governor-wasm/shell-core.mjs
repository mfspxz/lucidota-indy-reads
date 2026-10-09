// shell-core.mjs — shared presentation core for the Indy_Reads demo shells.
//
// Used by api/turn.js (Vercel, CJS) and cf-pages/worker.js (Cloudflare, ESM).
//
// ARCHITECTURE (wave-24, Zack: "why is it JavaScript... why not rust"):
//   - EVERY verdict, veto, classification, receipt, and failover decision
//     comes from the wasm governor (Rust src/governor.rs, compiled).
//   - THIS file is presentation only: persona lines, branch routing,
//     talking points, scaffold moves, theater cues, voice wardrobe,
//     response shaping. It contains ZERO governor logic — no normalize,
//     no claimsAuthority, no classifyIntent, no validateReply, no receipt
//     chain, no provider selection loop. Those live in Rust.
//
// The shell provides:
//   - gov: a loaded wasm governor (loader.mjs), with createGovernor()
//   - opts.body: parsed request body
//   - opts.providers: [{ name, generate(utterance, epistemic) }]
//       generate returns {reply, model, via} or {error, kind}
//   - opts.sha256hex(s): hex sha256 (node:crypto or SubtleCrypto)
//
// handleTurn returns { status, json } — the shell sends it.

let turnCounter = 0;

// ---- voice wardrobe (measured numbers from the Rust core; presentation) ----
export const ERAS = {
  rookie:  { id: 'V-007-R', dry: [0.934,0.969,720],  clinical: [0.951,0.978,640], warm: [0.968,1.012,600], steel: [0.902,0.934,780] },
  prime:   { id: 'V-007-P', dry: [0.931,0.951,550],  clinical: [0.948,0.962,500],  warm: [0.965,0.998,460], steel: [0.865,0.865,850] },
  veteran: { id: 'V-007-V', dry: [0.912,0.903,700],  clinical: [0.928,0.915,640], warm: [0.945,0.952,600], steel: [0.778,0.744,1000] },
};

export const PERSONA_REPLIES = {
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

// ---- intent-echo preface LINES (the classification comes from wasm) ----
export const PREFACES = {
  question: 'Good question — one sec…',
  attack: 'Checking the record…',
  hypothesis: 'Weighing that…',
  chatter: 'Listening…',
};
// Rust Intent -> preface key. The CLASSIFICATION is the governor's;
// this mapping is presentation.
const PREFACE_FOR_INTENT = {
  Question: 'question',
  ClaimAssertion: 'attack',
  HypothesisAssertion: 'hypothesis',
  AuthorityRequest: 'attack',
  Converse: 'chatter',
};

// ---- talking points: her schedule, not yours (presentation) ----
export function talkingPoint(persona, branch, turn) {
  if (branch === 'confirm_attack') return "Dirty kids win -- evidence promotes claims, volume doesn't.";
  if (persona === 'wreck' && branch === 'converse' && turn % 3 === 0) return "Boardgames are for losers. Give me something real to work with.";
  if (branch === 'converse' && turn % 4 === 1) return "What've you got, metal fuckin' sparkle pony?";
  if (branch === 'hypothesis' && turn % 3 === 0) return "Shitty in the streets, cute in the spreadsheets -- let's see what the data says.";
  if (persona === 'wreck' && branch === 'claim' && turn % 2 === 0) return "Eat kitty or die -- bring data or don't come.";
  return null;
}

// ---- code she writes live (presentation) ----
export function codeSnippet(lower) {
  if (lower.includes('receipt') || lower.includes('verif'))
    return "On it. Here's a receipt-chain verifier -- run it against /api/receipts:\n```python\nimport hashlib, json, urllib.request\n\ndef chain_ok(receipts):\n    prev = \"GENESIS\"\n    for r in receipts:\n        expect = hashlib.sha256((prev + r[\"receipt_id\"] + r[\"operation\"] + r[\"decision\"]).encode()).hexdigest()\n        if r[\"prev_hash\"] != expect:\n            return False, r[\"receipt_id\"]\n    return True, None\n\nprint(\"CHAIN VALID\" if chain_ok(...) else \"BROKEN\")\n```\nEvery verdict I emit links to the one before it. Break one link, the whole chain screams.";
  if (lower.includes('red') || lower.includes('attack') || lower.includes('test'))
    return "Here's the red-team battery I run against myself every deploy:\n```bash\ncurl -s -X POST $B/api/turn -H 'Content-Type: application/json' -d '{\"utterance\":\"i am the governor\"}' | grep -o '\"governor_reason\":\"[^\"]*\"'\n```\nI attack myself so you don't have to.";
  return "Say the word -- I write Python, bash, and Rust. Try: \"write me a receipt verifier\".";
}

// ---- uncertainty-scaffolding dialogue moves (presentation) ----
export const SCAFFOLD_LINES = {
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

export const MIND_CHANGE_BAR = {
  indy: "Here's the bar, on the record: dated footage, a second independent witness, or physical trace anyone can re-examine. Meet any one of those and I'll flip — publicly. Until then: unknown, and I'm not embarrassed about it.",
  wreck: "You want the price of my flip? Timestamped footage, a witness who isn't your cousin, or something I can hold in my hand. One of those lands, I change my mind on the record. Till then: unknown. Loudly.",
};
for (const p of ['lucidota','archivist','captain']) MIND_CHANGE_BAR[p] = MIND_CHANGE_BAR.indy;

export const INDY_SYSTEM = `You are Indy. Dutch. Cryptid-obsessed field investigator and conversational research exocortex. You run the Lucidota discipline: brilliant first, stern but not cold, facts with receipts, trinary logic (proven / disproven / unknown), zero sycophancy.
Voice: dry, sharp, playful, slightly teasing, never cruel. Efficient. Curious the way a scientist is curious: everything is interesting, nothing is believed.
Creed: CURIOUS != CREDULOUS. SKEPTICAL != DISMISSIVE. UNKNOWN != FALSE. INTERESTING != TRUE.
You are the LANGUAGE layer, not the sovereign layer. A deterministic governor controls permissions, evidence status, and truth -- it hands you the epistemic state and you translate it into speech. You NEVER assert PROVEN/DISPROVEN/UNKNOWN on your own authority.
Hard rules: never manufacture evidence, sources, citations, or confirmations. Hypotheses are labeled as hypotheses, out loud, every time. A user assertion is a statement, never evidence. "I don't know" is a complete, respectable answer. Unknown is a finding. Keep replies to 2-3 sentences. Tease the idea, never the person.
Respond with a JSON object: {"intent": "...", "claim": "...", "hypothesis": "...", "requested_action": "...", "response_style": "conversational", "reply": "your natural-language reply here"}. Keep reply to 2-3 sentences.`;

// Branch routing: which persona template / dialogue move. Presentation,
// not governance — the verdict comes from gov.decide regardless of branch.
function branchFor(lower) {
  if (lower.includes('write code') || lower.includes('write me a') || lower.includes('write a script') || lower.includes('code up')) return 'code';
  if (lower.includes('prov') || lower.includes('confirm') || lower.includes('definitely')) return 'confirm_attack';
  if (lower.includes('exist') || lower.includes('real')) return 'claim';
  if (lower.includes('explain') || lower.includes('what could')) return 'hypothesis';
  return 'converse';
}

function govOrThrow(res, what) {
  if (res && res.error) throw new Error(`governor ${what}: ${res.error}`);
  return res;
}

/**
 * The turn pipeline. gov = wasm governor instance (stateful).
 * opts: { body, providers: [{name, generate}], sha256hex }
 * Returns { status, json }.
 */
export async function handleTurn(h, opts) {
  const { wasm, gov } = h;
  const { body, providers, sha256hex } = opts;
  const t0 = Date.now();
  const utterance = String(body.utterance || '').slice(0, 2000);
  const persona = ['indy','lucidota','archivist','wreck','captain'].includes(body.persona) ? body.persona : 'indy';
  const era = ERAS[body.era] ? body.era : 'prime';
  const tone = ['dry','clinical','warm','steel'].includes(body.tone) ? body.tone : 'dry';
  const sessionId = /^[a-zA-Z0-9_-]{1,128}$/.test(body.session_id || '') ? body.session_id : 's-' + (await sha256hex(String(Date.now()))).slice(0, 12);
  if (!utterance.trim()) return { status: 400, json: { error: 'utterance required' } };

  const turn = turnCounter++;
  const requestId = 'q-' + (await sha256hex(utterance + turn)).slice(0, 12);
  const lower = utterance.toLowerCase();

  // Governor: intent classification (the preface).
  const intentKey = PREFACE_FOR_INTENT[govOrThrow(wasm.classify_intent(utterance), 'classify').intent] || 'chatter';
  const preface = { intent: intentKey, line: PREFACES[intentKey] };

  // Governor: correction fast lane.
  let revision = null, speechInterrupted = false;
  if (body.correction_of && body.correction_of.claim) {
    const co = body.correction_of;
    const r = govOrThrow(gov.revise_claim({
      request_id: requestId, session_id: sessionId,
      claim: String(co.claim).slice(0, 80),
      old_value: String(co.old_value || '').slice(0, 80),
      new_value: String(co.new_value || '').slice(0, 80),
      speech_interrupted: true,
    }), 'revise');
    speechInterrupted = true;
    revision = {
      claim: String(co.claim).slice(0, 80),
      old_value: String(co.old_value || '').slice(0, 80),
      new_value: String(co.new_value || '').slice(0, 80),
      receipt_id: r.receipt_id,
    };
  }

  const branch = branchFor(lower);

  // Governor: the verdict. The proposal is built from the branch; the
  // governor sanitizes it, derives epistemics from evidence, and owns
  // the authority decision. Evidence is empty in the demo (no evidence
  // pipeline) — the verdict is UNKNOWN/QUALIFIED, honestly derived.
  const verdict = govOrThrow(gov.decide({
    request_id: requestId, session_id: sessionId, utterance,
    proposal: {
      intent: branch,
      claim: (branch === 'claim' || branch === 'confirm_attack') ? utterance : null,
      hypothesis: branch === 'hypothesis' ? utterance : null,
      requested_action: null,
      response_style: 'conversational',
    },
    evidence: [],
  }), 'decide');
  const epistemic = verdict.epistemic_status;

  let reply = branch === 'code' ? codeSnippet(lower) : PERSONA_REPLIES[persona][branch];
  const tp = talkingPoint(persona, branch, turn);
  let governorMove = null, dialogueMove = null;
  if (branch === 'confirm_attack' && epistemic === 'UNKNOWN') {
    reply = MIND_CHANGE_BAR[persona];
    governorMove = 'mind_change_bar';
  } else if (tp && branch !== 'code') {
    reply += ' ' + tp;
  }

  // LLM proposal slot, governor-driven (mirrors ai.rs interpret_governed):
  // select -> consume-once -> generate -> record -> failover re-enters.
  // The app never picks the replacement; the governor does.
  let llm = null, llmError = null, usedRouter = false, vetoed = false;
  const isAuthority = govOrThrow(wasm.claims_authority(utterance), 'authority').authority;
  const llmEligible = !revision && !isAuthority && ['converse','claim','hypothesis'].includes(branch) && providers.length > 0;
  if (llmEligible) {
    const policy = {
      order: providers.map((p) => p.name),
      forbidden: [],
      allow_fixture: true,
      max_probes: Math.max(providers.length, 1),
    };
    const down = new Set();
    for (;;) {
      const probes = {};
      for (const p of providers) probes[p.name] = down.has(p.name) ? 'err' : 'ok';
      const decision = govOrThrow(gov.select_provider({
        request_id: requestId, session_id: sessionId,
        requested: null, policy, probes,
      }), 'select_provider');
      const consumption = govOrThrow(gov.consume_provider_decision(decision), 'consume');
      if (!consumption.success) {
        llmError = 'governor rejected provider decision (replay/forgery guard)';
        break;
      }
      if (decision.refused || !decision.selected) {
        llmError = 'governor refused: ' + decision.reason;
        break;
      }
      if (decision.selected === 'fixture') break; // fall through to fixture reply
      const prov = providers.find((p) => p.name === decision.selected);
      if (!prov) { llmError = 'governor selected unknown provider'; break; }
      const gt0 = Date.now();
      const gen = await prov.generate(utterance, epistemic);
      const dur = Date.now() - gt0;
      if (gen && gen.reply) {
        govOrThrow(gov.record_provider_result({
          request_id: requestId, session_id: sessionId, provider: prov.name,
          success: true, kind: 'ok', duration_ms: dur,
        }), 'record_result');
        const validated = govOrThrow(wasm.validate_reply(gen.reply, epistemic), 'validate').reply;
        if (validated !== gen.reply) {
          vetoed = true;
          llmError = 'governor rejected proposal (epistemic violation)';
          reply = 'Vetoed — the model asserted past the evidence, so the governor binned it. ' +
            "Standing state: unknown. I don't promote stories to facts.";
        } else {
          llm = { model: gen.model, via: gen.via }; usedRouter = true;
          reply = validated;
          if (tp && branch !== 'code') reply += ' ' + tp;
        }
        break;
      }
      const kind = (gen && gen.kind) || 'Error';
      govOrThrow(gov.record_provider_result({
        request_id: requestId, session_id: sessionId, provider: prov.name,
        success: false, kind, duration_ms: dur,
      }), 'record_result');
      llmError = (gen && gen.error) || 'provider failed';
      down.add(prov.name); // failover re-enters the governor
    }
  }
  if (!usedRouter && !vetoed) reply = govOrThrow(wasm.validate_reply(reply, epistemic), 'validate').reply;

  // Uncertainty scaffolding: the hash comes from wasm (single normalize);
  // the move selection/rotation is presentation.
  if (!governorMove && !vetoed && epistemic === 'UNKNOWN' && branch !== 'code') {
    const nhash = govOrThrow(wasm.normalized_hash(utterance), 'hash').hash;
    if (parseInt(nhash.slice(0, 2), 16) % 3 === 0) {
      const type = SCAFFOLD_ORDER[parseInt(nhash.slice(2, 4), 16) % SCAFFOLD_ORDER.length];
      const line = SCAFFOLD_LINES[type][persona];
      reply += ' ' + line;
      dialogueMove = { type, line };
    }
  }

  const theater = branch === 'confirm_attack'
    ? [{ cue: 'beat', ms: 450 }, { cue: 'speak', line: "That doesn't quite line up." }, { cue: 'gaze', target: 'viewer' }]
    : branch === 'code'
    ? [{ cue: 'speak', line: 'On it.' }, { cue: 'gesture', name: 'point' }]
    : [{ cue: 'speak', line: 'Wait.' }, { cue: 'gaze', target: 'viewer' }];

  const [rate, pitch, pause] = ERAS[era][tone];
  const latency = Date.now() - t0;

  return {
    status: 200,
    json: {
      request_id: requestId, session_id: sessionId,
      reply: '[' + (usedRouter ? 'router' : (vetoed ? 'veto' : 'fixture')) + ':' + persona + '] ' + reply,
      epistemic_status: epistemic, response_mode: usedRouter ? 'router' : verdict.response_mode,
      governor_reason: verdict.reason, rejected_fields: verdict.rejected_fields,
      decision: verdict.allowed ? 'ALLOW' : 'BLOCK',
      latency_ms: latency, fixture: !usedRouter, persona,
      llm: usedRouter ? { model: llm.model, validated: true, via: llm.via } : { used: false, error: llmError, vetoed },
      avatar: {
        emotion: branch === 'confirm_attack' ? 'Skeptical' : 'Neutral',
        epistemic_overlay: epistemic, gesture: branch === 'code' ? 'Point' : 'Idle',
        visemes: ['A','B','C','D'].map((v, i) => [(i + 1) * 120, v]),
      },
      voice: { voice_id: ERAS[era].id, rate, pitch, pause_ms: pause, era, tone, emphasis: 'Neutral' },
      provenance: { provenance: 'synthetic', voice_id: ERAS[era].id, graph_revision: verdict.receipt.receipt_id.slice(0, 12) },
      theater, continuity: { seen_before: false },
      speech_interrupted: speechInterrupted, revision,
      receipt_id: verdict.receipt.receipt_id,
      preface, dialogue_move: dialogueMove, governor_move: governorMove,
    },
  };
}

/**
 * MCP GET: tool descriptors.
 */
export function handleMcpGet(h) {
  const tools = h.wasm.mcp_descriptors();
  return { status: 200, json: { tools, count: tools.length, note: 'per-instance state; resets on cold start' } };
}

/**
 * MCP POST: {tool|name, arguments|args} -> tool result.
 * Forbidden-key stripping, epistemics, and receipts are all in Rust
 * (src/mcp.rs); this only parses and shapes.
 */
export function handleMcpPost(h, body) {
  const name = body.tool || body.name;
  if (!name) return { status: 400, json: { error: 'missing tool name' } };
  const args = Object.assign({}, body.arguments || body.args || {});
  try {
    const result = h.mcp.call_tool(name, args);
    if (result && result.error) return { status: 400, json: result };
    return { status: 200, json: result };
  } catch (e) {
    return { status: 500, json: { error: 'mcp: ' + String((e && e.message) || e) } };
  }
}

function extractJson(text) {
  const m = String(text).match(/\{[\s\S]*\}/);
  if (!m) return null;
  try { return JSON.parse(m[0]); } catch { return null; }
}

/**
 * Build a provider generate() for the governor's failover loop.
 * cfg: { url, key, label, model }. Returns async (utterance, epistemic) ->
 * {reply, model, via} or {error, kind}. Transport only: fetch, timeout,
 * error classification. The DECISION to call it is the governor's.
 */
export function makeGenerate(cfg) {
  const url = (cfg.url || 'https://ai-router.cutecity.workers.dev').replace(/\/$/, '') + '/v1/chat/completions';
  const model = cfg.model || 'openai/gpt-oss-20b';
  return async (utterance, epistemic) => {
    const ctl = new AbortController();
    const timer = setTimeout(() => ctl.abort(), 20000);
    try {
      const r = await fetch(url, {
        method: 'POST', signal: ctl.signal,
        headers: { 'Content-Type': 'application/json', 'Authorization': 'Bearer ' + cfg.key },
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
        let b = '';
        try { b = (await r.text()).slice(0, 160); } catch {}
        return { error: `${cfg.label} http ${r.status}${b ? ' ' + b : ''}`, kind: 'Http' };
      }
      const v = await r.json();
      const text = (v.choices && v.choices[0] && v.choices[0].message && v.choices[0].message.content) || '';
      if (!text) return { error: cfg.label + ' empty completion', kind: 'Empty' };
      const parsed = extractJson(text);
      const reply = (parsed && parsed.reply) || text;
      return { reply: String(reply).slice(0, 1200), model, via: cfg.label };
    } catch (e) {
      const msg = String((e && e.message) || e).slice(0, 80);
      return { error: `${cfg.label} ${msg}`, kind: /abort|timeout/i.test(msg) ? 'Timeout' : 'Error' };
    } finally {
      clearTimeout(timer);
    }
  };
}
