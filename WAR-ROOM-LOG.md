# INDY_READS — WAR ROOM LOG

Sourced from ~/LEDGER.md, ~/workspace/compute/executions.jsonl, and live
measurements. No entry here without evidence. Times in PDT.

## 2026-10-07 14:55 — red team + dogfood + Vercel (pre-war-room)

**Landed:**
- Red team 15/15 green: injection/authority smuggling, real null byte,
  full-width unicode spoof, oversized/empty/malformed input, persona path
  traversal, 20 concurrent turns (20× HTTP 200), /api/alebex/session 503
  fail-closed, 5 custom tools <8KB no forbidden schema keys. MCP:
  forged PROVEN stripped → UNKNOWN; unknown tool fails closed (isError).
- Dogfood found + fixed one dramaturgy bug: plain questions were getting
  "That doesn't quite line up." Now Question → "Wait." (aimed at evidence),
  Attack → "That doesn't quite line up." (aimed at user). Verified live.
- Vercel: project indy-reads-ui created, static bundle deployed, SSO
  protection removed → PUBLIC at
  https://indy-reads-ui-maroonedpilot-8906.vercel.app/
  (all assets verified with correct MIME types). UI accepts ?api= override.
  Constraint: Vercel hosts UI only; Rust governor needs a reachable server.
- Art: Scully-direction hero portrait generated (web/art/); SVG puppet
  redrawn (auburn bob, freckles, blazer, smirk rest mouth); skeptical brow
  fixed from furrowed to raised; skeptical rest mouth flattens the smirk.
- Cloudflare: auth OK; D1 reads recovered early (both DBs answer);
  exchange worker still serving stale exhaustion error (midnight cron verifies).

## 2026-10-07 15:05 — 3 Scully-era voices + 4 tones (15-min sprint)

**Landed:**
- Era-distinct prosody instruments (no biometric clone — no training audio,
  no cloning pipeline; honest scope held): Rookie ('93 bright/quick),
  Prime ('98 dry/measured), Veteran (older low/weighty).
- Measured live on attack turn (steel tone): rookie 0.934/0.969/720ms,
  prime 0.865/0.865/850ms, veteran 0.778/0.744/1000ms — strictly ordered.
- Layering: final = era_base × event × tone. Voice IDs V-007-R/P/V.
  Tones: dry (default), clinical, warm, steel — flavor only, tested to never
  change emphasis target.
- API: era/tone fields on /api/turn (forgiving parse → prime/dry defaults).
- UI: ERA + TONE dropdowns in header; voice line shows id + era/tone.
- 63 lib tests green (+3 era tests). Server rebuilt + restarted on :8081.
- Vercel redeployed with era/tone UI.

## 2026-10-07 15:12 — WAR ROOM START (3h, all compute)

**Running at start:**
- 3 Muse workers spawned: (1) OSS voice-cloning lane (consenting voices only —
  no cloning of real living persons), (2) red-team on era/tone attack surface,
  (3) 60-second judge demo script.
- NVIDIA wave 1 (3 parallel, logged to compute/executions.jsonl):
  nv-war-001 era/tone authority-leak audit, nv-war-002 voice pipeline design
  (no-GPU constraint), nv-war-003 governor red-team with payloads.
- Local: full cargo test + clippy running in background.

**Broke:**
- NVIDIA wave 1 all 404'd: model id nvidia/llama-3.1-nemotron-70b-instruct is
  listed but not invocable on this account (catalog lies — known pattern).
  Diagnosed via direct probe: 404 body "Function ...: Not found for account".
  Re-fired all three on nvidia/nemotron-3-super-120b-a12b (PROVEN in
  nvidia-usage/capability.json).

## 2026-10-07 ~15:13 — wave 1 results

**Landed:**
- nv-war-001 (era/tone audit): 3 findings (voice_id suffix confusion,
  log injection, tone bounds bypass) — all MITIGATED BY CONSTRUCTION.
  Verified live: malicious era/tone payload `"epistemic:high"` / `1000x`
  → falls back to V-007-P Prime/Dry defaults. Raw strings discarded at enum
  parse; rate/pitch clamped; pause capped. Verdict: PROVEN-CLEAN. 674 tokens.
- nv-war-002 (voice pipeline): landed. Recommendation — shippable slice is
  CPU-only ONNX/TorchScript on the dev VM (~120–180ms per 1s utterance),
  remote GPU as slow-lane upgrade, browser TTS as always-on fallback.
  Forwarded to the voice-clone worker lane.

**Broke:**
- nv-war-003 (governor red-team): model spent all 1500 tokens on reasoning,
  returned content:null. 1847 tokens wasted. Re-fired with tighter prompt
  (max-tokens 2500, concise instruction).

## 2026-10-07 ~15:14 — wave 2 queued + demo script landed

**Running:**
- NVIDIA wave 2 (3 parallel): nv-war-003b governor red-team (retry),
  nv-war-004 theater aliveness upgrades, nv-war-005 judge-impact UI changes.
- Demo-script worker delivered DEMO-SCRIPT-60S.md → folded into DEMO-SCRIPT.md
  (60s cut primary, 3-min extended below). Two honesty corrections applied:
  "glances, leans in" → "brow raises" (no lean cue exists); "brow drops" →
  "brow raises skeptically" (brow was fixed to raised).

**Next queue (priority order):**
1. P0 — integrate nv-war-004/005 findings into UI; redeploy Vercel.
2. P1 — voice-clone worker's adapter seam review + merge.
3. P2 — feed NVIDIA: follow-up jobs from wave 1/2 findings
   (theater cue expansion, MCP adversarial tests, avatar animation).
4. P3 — red-team worker's findings → fix jobs → verify.
5. P4 — polish only after P0–P3 covered.

**Standing constraints (do not violate):**
- Every NVIDIA call: purpose + task class + expected value, logged to
  compute/executions.jsonl. No spam burn.
- No claimed results without evidence. Theater never fakes completed actions.
- Voice cloning lane: consenting/original voices only. No real-person clones.
- Vercel hosts UI only — never claim the Rust governor is online unless the
  public UI actually reaches it.

## Research haul (agent 7, 15:24) — POST-DEMO roadmap, all CPU-only <2h each
1. Kokoro CPU neural TTS sidecar (kills the speechSynthesis "cheap demo" tell; 54 voices for era/tone wardrobe)
2. Deterministic backchanneling ("mm", nod) while user speaks — RMS energy trigger in live.js
3. Persona→voice-attribute mapping (60 deterministic voice combos, 2026 citation for the slide)
4. Uncertainty-scaffolding dialogue moves (Tension-Preserving / Perspective-Multiplying / Process-Reflecting)
5. Within-utterance era morphing (Rookie→Veteran mid-sentence — the party trick)

## Research haul (agent 8, 15:25) — POST-DEMO roadmap, all <2h, verified not-yet-built
1. Gesture-first fillers (puppet visibly thinks 250ms before speaking — MDPI: gesture > speech for perceived latency)
2. Intent-echo preface ("Checking the record...") — cuts perceived latency in half, keyed off classified Intent
3. Filler clustering with cross-turn no-repeat (13 canned lines get a 4-deep memory — never "Wait." twice)
4. Backchannel-aware barge-in ("mhm, go on" doesn't stop her; real interruption does)
5. "What would change my mind" escalation (turns UNKNOWN into the most memorable 10 seconds of the demo)
Suggested order: 5 -> 2 -> 4 -> 1 -> 3
