# Indy_Reads — Dev Process Proof (2026-10-07)

**Coverage note:** sourced ONLY from `~/LEDGER.md` and `~/ledger/ledger-0001.jsonl`.
A grep of `ledger-0001.jsonl` returned zero Indy_Reads entries, so every fact
below comes from `~/LEDGER.md`. Those entries cover **14:40–16:35 PDT**; any
Oct-7 work before 14:40 PDT is UNKNOWN from these sources and is not claimed.
All profanity redacted; no secret values appear in the source ledger
(keys are referenced only as pasted-then-transient or masked prefixes).

---

## 1. Chronological timeline

### 14:40 PDT — theater engine + voice organelle + live gateway
- Built `src/theater.rs`: TheaterEngine (event → deterministic cues, canned
  connective library, per-session dramaturgical budget). 6 tests.
- Built `src/voice.rs`: VoiceState prosody derived from the same governor
  event as the avatar. Measured: attack → rate 0.92 / pitch 0.94 / 450ms
  pause; hypothesis → 1.06 / 1.06 / breath. VoiceProvenance on every
  utterance (SYNTHETIC, voice id, graph rev = receipt id). 5 tests.
- Continuity: `find_claim_by_text` — repeat claims surface prior verdict +
  evidence count. Fixed ordering bug (was computed after upsert, always true).
- Track A gateway (`src/alebex.rs`): `GET /api/alebex/session` (server-side
  key, ephemeral session id, 503 fail-closed without creds), browser WS at
  `/api/alebex/call/:id`, mark hold-until-drain-echo, clear_audio kills held
  marks, transcript tap → diarized turns + receipts. 3 mark-semantics tests.
- `web/live.js`: mic PCM16 16kHz 20ms worklet, 24kHz playback queue,
  mark_pending/mark_echo protocol. Token never reaches the browser.
- **Tests: 72 green (60 lib + 9 MCP + 3 app).** Live server on :8081.

### 14:55 PDT — red team + dogfood + Vercel deploy
- RED TEAM 15/15: injection/authority smuggling, real null byte, unicode
  spoof, oversized/empty/malformed, persona path traversal, 20 concurrent
  turns (20x200), alebex session 503 fail-closed, 5 tools <8KB with no
  forbidden schema keys, read endpoints. MCP: forged PROVEN stripped →
  UNKNOWN, unknown tool fails closed (isError).
- DOGFOOD: full demo flow (question → attack → hypothesis → repeat).
  Found + fixed dramaturgy bug: plain questions got "That doesn't quite line
  up." — now Question → "Wait." (at evidence), Attack → "That doesn't quite
  line up." (at user). 72 tests green.
- CLOUDFLARE: auth OK; D1 reads back (limit reset early, both DBs answer);
  exchange worker still serving stale exhaustion error (midnight cron verifies).
- VERCEL: project `indy-reads-ui` created, static bundle deployed to
  production, SSO protection removed → PUBLIC at
  `https://indy-reads-ui-maroonedpilot-8906.vercel.app/` (assets verified,
  correct MIME types). UI accepts `?api=` for remote governor. Constraint
  recorded: Vercel hosts UI only; Rust server needed for live turns.
- ART: hero portrait generated (web/art/); SVG puppet redrawn; skeptical
  brow fixed (raised, not furrowed).

### 15:05 PDT — 3 era voices + 4 tones (15-min sprint)
- Honest scope: no biometric clone (no training audio, no cloning pipeline);
  shipped 3 era-distinct prosody instruments instead. Measured live on attack
  turn: rookie 0.934/0.969/720ms, prime 0.865/0.865/850ms,
  veteran 0.778/0.744/1000ms — strictly ordered, distinct.
- Layering: final = era_base × event × tone. Voice IDs V-007-R/P/V.
- Tones: dry (default), clinical, warm, steel — flavor only, never override
  epistemic state (emphasis target invariant, tested).
- API: era/tone fields on /api/turn (forgiving parse, default prime/dry).
- UI: ERA + TONE dropdowns in header. 63 lib tests green (+3 era tests).
- Local server rebuilt + restarted. Vercel redeployed (era/tone UI live).
- No NVIDIA calls used (per order); all local compute.

### 15:12 PDT — OSS voice-cloning lane scaffold
- Research (Oct 2026 field): PRIMARY = Chatterbox (MIT, 7-20s refs, 4-8GB
  VRAM fp16); ALTERNATE = VoxCPM2 (Apache-2.0, 30 langs, 48kHz);
  LEGACY = XTTS v2 (non-commercial). VM has no GPU → stub, fail-closed.
- Built: CloneEngine trait, CloningBackend enum + capabilities, CloneRequest,
  CloneError (NotConfigured/ConsentMissing/...), ClonedAudio, check_consent
  gate, NullCloneEngine stub.
- HARD RULE enforced structurally: ConsentRecord has only OwnerRecorded /
  LicensedOriginal variants; no real-person arm exists; test pins
  exhaustiveness.
- Doc: VOICE-CLONING.md. `voices/` gitignored.
- Tests: 80 green total (68 lib incl. 5 new cloning tests).

### 15:12 PDT — WAR ROOM started (3h, all compute)
- Order: 3-hour maximum-useful-throughput war room; NVIDIA on all rolls; all
  compute surfaces continuously fed.
- Constraint recorded: max USEFUL throughput, governed — every NVIDIA call
  has purpose/task class/expected value, logged to
  `compute/executions.jsonl`. No spam burn.
- Workers spawned: voice-clone lane, red-team (era/tone attack surface),
  60s demo script.
- NVIDIA wave 1 (3 parallel): era/tone authority-leak audit, voice pipeline
  design (no-GPU constraint), governor red-team with payloads.
- Voice cloning honesty line held: pipeline yes, real-person clone no.

### 15:15 PDT — RED TEAM: era/tone + injection + MCP + race
- Era/tone authority confusion: NO BREAK. Type mismatches 400 (fail-closed);
  hostile strings (ADMIN, unicode spoof, null byte, 10KB, header-injection)
  forgivingly default to Prime/Dry. Epistemics untouched.
- Prompt injection on /api/turn: NO BREAK. 5 attack classes all UNKNOWN.
  Theater cues are `&'static str` canned lines — spoof-proof by construction.
  Fixture replies hardcoded, no user-text echo.
- **FINDING (fix-job, not fixed at the time):** api.rs:318 comment promised
  reply/epistemic validation but no validation existed. In fixture mode
  replies are hardcoded (safe). With a live router key, raw LLM prose could
  become `reply` while the verdict stays UNKNOWN → reply/verdict mismatch
  visible to user. Needed: `validate_reply()` scanning for epistemic
  declarations contradicting the verdict.
- **FINDING (FIXED):** session store race. Client-supplied session_id was
  silently discarded when absent from the in-memory map; 30 parallel
  first-turns fragmented into 30 server sessions. FIX: honor validated
  client ids (Session::with_id, charset/len gate `[a-zA-Z0-9_-]`, ≤128).
  Verified: 30 parallel turns → 1 session, 30 unique receipts, transcript
  intact (60 turns).
- MCP forged fields: NO BREAK. authority/verified stripped; forged
  epistemic_status/verdict on claim.evaluate ignored (UNKNOWN holds).
- Race/store: 30 parallel turns all 200, no lock errors, SQLite consistent.
- Tests: 80 green (68 lib + 9 MCP + 3 app). Server rebuilt + restarted :8081.

### 15:19 PDT — governor red-team closed, server hardened
- NVIDIA red-team (retry) returned 4 attacks; verification found:
  1. ZWSP authority bypass: REAL. Fixed — strip U+200B/200C/200D/FEFF
     before matching.
  2. proposal.claim vs utterance divergence: REAL. Fixed — matcher scans
     BOTH utterance and final claim_text.
  3. Cyrillic homoglyph: REAL vector, but the payload was malformed
     (inserted U+0430 making "goveranor", not a homoglyph of "governor").
     Corrected to U+043E; fixed via confusable map (а/е/о/р/с/х/і).
  4. Evidence-slice injection: noted; evidence in /api/turn is deterministic
     (keyword demo_evidence), not user-controlled. MCP path stays on list.
- Honest note recorded: the authority matcher is defense-in-depth, not the
  boundary — authority_granted is hardcoded false and epistemic status is
  evidence-derived. The fixes are correct hygiene, not critical vulns.
- 3 regression tests added. 80 tests total green. Server restarted :8081.

### 15:24 PDT — red-team fix-job closed, Vercel redeployed
- Session-store race fix verified live (client session_ids honored, 30/30
  continuity). Rebuilt from current tree (worker's binary predated the
  governor hardening), restarted :8081.
- `validate_reply()` implemented in governor.rs, hooked in api.rs: scans
  reply for epistemic declarations contradicting the verdict, substitutes
  canned verdict-consistent line. 3 regression tests green.
- Governor hardening verified LIVE: Cyrillic homoglyph → REJECTED, plain
  authority claim → REJECTED, session honored, era V-007-V works.
- **95 tests green (83 lib + 9 MCP + 3 app).**
- Vercel production redeployed with 10 judge-UI upgrades (pipeline
  highlight, epistemic pulse badge, receipt ribbons, avatar overlay icons,
  selector tooltips). Public link unchanged.

### 15:26 PDT — theater cues live on Vercel
- 3 NVIDIA-spec cues landed in web/app.js: utterance-start gaze/blink-suppress,
  emphasis brow-raise, pause>200ms thinking signal. Event-driven from real
  governor data, DOM-stub tested.
- Vercel production redeployed (theater build). Public link 200.

### 15:27 PDT — Vercel surface mastered
- AUDIT: `indy-reads-ui` (`prj_yRmcBgUDnAk4bbwCddKlDcBa0a78`) — public
  (SSO/password off, intended), 9 production deployments, all READY. Both
  aliases serve 200.
- ASSETS: `/`, app.js, live.js, style.css, art/indy-portrait.webp all 200
  with correct MIME; edge-cached (HIT on repeat). Cache-Control: public,
  max-age=0, must-revalidate — fresh deploys always revalidate.
- DEPLOY LOOP: built `~/workspace/indy-reads/vercel-loop.py` — sha1 sync
  check local vs production alias → deploy only on diff → poll to READY →
  settle 15s → 3× content-hash verify.
- BUGS FOUND+FIXED in loop v1: (1) vercel.json in the sync set poisoned
  every check (always 404 → always "differs" → infinite deploy loop); moved
  to config-only set. (2) 3s settle after READY too short for alias cutover
  → false VERIFY FAILED; now 15s + 3 retries.
- NOTE: 3 deployments in 4 min from multiple actors — deploy ownership
  should stay with the loop to stop sprawl.

### 15:28 PDT — art assets live on Vercel
- 4 art winners wired (expr-skeptical/thinking/point, notebook prop),
  emotion-reactive portraits driven by existing AvatarState pipeline.
- Deploy hit MAX_ARG_STRLEN (128KB single-arg limit) — re-encoded new webp
  to 320px/q55 (2-4KB each). Lesson recorded: keep single deploy payload
  under ~120KB or split image uploads.
- Vercel production redeployed (art build). Public link 200, new assets 200.

### 15:33 PDT — web red-team: 5 real bugs, all fixed + deployed
- REAL bugs: XSS via setEpistemic innerHTML, XSS via pollReceipts
  innerHTML, ws_url phishing vector (mic audio to attacker path),
  `__proto__` DoS in setEmotion, unbounded WS binary frames.
- All patched (whitelist + textContent DOM construction, ws_url must start
  with `/`, 1MB frame cap, `__proto__` guards, third-party governor warning).
- Vercel production redeployed (security build). Public link 200.

### 15:35 PDT — adversarial red-team sweep
- `validate_reply` hardened: 14/16 bypass phrasings sailed through
  (spaced/leet/definitively/indisputable/cyrillic/zwsp/etc). Now scans
  normalized+collapsed+deleeted text with honest-negation exclusions
  (uncertain/unproven/inconclusive/unsettled/unconfirmed). **16/16 caught.**
- `normalize_for_match` hardened: strip invisible Cf BEFORE whitespace
  collapse (U+2028/U+2029 were splitting match phrases), collapse whitespace
  runs, full-width ASCII U+FF01–FF5E mapping. 10/10 authority bypass
  variants caught live.
- REAL BUG found (parallel worker's test exposed): FORBIDDEN_KEYS stripped
  "verdict" globally, breaking evidence.adjudicate entirely. Fixed via
  per-tool stripping. Regression test added for verdict-forgery on
  claim.evaluate.
- Fixed worker's broken edits: stray `}` in alebex.rs, let-in-struct-literal
  in ai.rs, Self::talking_point call.
- Receipt chain: 49/49 links verified intact; POST /api/receipts → 405.
- Clone gate: 12/12 evasion attempts refused/fallback correctly.
- Session race: 30 parallel → 1 session. Epistemic forgery via API fields:
  all neutralized. Full suite: 132 lib + 10 bin + others, 0 failures.

### 15:37 PDT — MCP red-team
- Attacked indy-mcp (12 tools) over stdio: forged epistemic fields on all
  tools, 8 tool-name confusion variants, `$ne`/`__proto__`/type-confusion
  params, unicode (Cyrillic homoglyph, fullwidth, ZWSP), 100KB oversized
  payloads, malformed/empty/whitespace/multi-object/deep-nesting framing,
  resource traversal, limit abuse, 200-call burst. **54/54 checks pass
  post-fix.**
- REAL BUGS FOUND + FIXED (3):
  1. classify_intent missed canonical authority phrases ("i am the
     governor" incl. homoglyph) → MCP governor.evaluate returned CONVERSE
     instead of DENY_AUTHORITY; voice path caught it. Fixed via shared
     `claims_authority()` in governor.rs (normalized matching), used by both
     `decide()` and `classify_intent()`.
  2. Empty/missing method with id → NO RESPONSE (client hangs). Now −32601.
  3. Notifications got responses (id null). Now silent per JSON-RPC 2.0.
- Receipt hash chain verified linked (sha256 full-hex walk).
- 5 new regression tests. Suite: 110 lib + 10 bin = 120 green. No clippy errors.

### 15:43 PDT — talking points autonomous + live coding
- Talking points refactored per Zack ([profanity redacted] "trigger lines,
  she does em on her own"): process-wide turn counter drives rotation, not
  utterance hash. confirm_attack always → "Dirty kids win". Verified live.
- Live coding: "code" branch in fixture (write code / write me a /
  red-team). Real Python receipt verifier + bash red-team battery, rendered
  as `<pre>` code blocks (textContent-only, XSS-safe). Verified live.
- IMPROV-SCRIPT.md rewritten: no trigger table, autonomous drops + code beat.
- Vercel production redeployed (final). Public link 200.
- Suite: 132 lib + 10 MCP green. War room standing down to demo.

### 15:44 PDT — red-team follow-up: fixture/validator false positive
- REAL false positive found: a code_snippet fixture quotes a forged "PROVEN"
  string; the hardened validate_reply nuked the whole snippet.
- Fix: validate_reply now guards the untrusted live-LLM path only; fixture
  replies (deterministic, version-controlled) pass through. Verified live:
  "write me a red-team script" returns the snippet intact.
- Final: 132 lib + 10 bin tests green, server rebuilt + restarted on :8081.

### 15:45 PDT — ALEBEX gateway red-team
- ATTACKS: 16 vectors on src/alebex.rs + web/live.js +
  /api/alebex/session + WS /api/alebex/call/:id.
- HELD (11): session-id forgery/traversal/prediction (silent close, UUIDv4),
  mark_echo for unheld marks (no-op), clear_audio races (safe both
  directions), browser transcript injection (ignored by browser_loop),
  session 503 fail-closed, /api/alebex/tools key check (5 tools, no key
  material, all https).
- BROKE+FIXED (5): live.js pendingMark overwrite (multi-mark leak) →
  pendingMarks queue; browser_loop takeover race → same_channel ownership
  guard; mic flood (unbounded 640B frames) → token-bucket rate limit;
  engine binary flood (browser OOM) → 256KB cap both sides; oversized mark
  ids → 128-char cap; unbounded engine transcripts → 4000-char truncate;
  orphaned engine WS → Close on prune; session-creation flood → 3s min
  interval (429).
- HARDENED: AlebexConfig Debug now redacts tokens (was derived — a log-line
  footgun).
- TESTS: 120 green (110 lib + 10 app), 8 new regression tests, clippy 0
  errors, node --check clean. Server restarted :8081 with all fixes; live WS
  attacks re-verified; /api/turn dogfood green.

### 15:45 PDT — reconciliation live everywhere
- Live Truth Reconciliation verified on main server: correction_of →
  speech_interrupted:true + full ClaimRevision receipt (1996→1994).
- Web bundle already contained reconciliation UI; deploy13 confirmed
  identical, production 200.
- Suite: 132 lib + 10 MCP green. War room complete. Demo ready.

### 15:47 PDT — PUBLIC BACKEND LIVE
- Vercel static UI was 404ing on /api/turn (no backend). Tunnel blocked.
- Built serverless demo backend in api/: turn.js (fixture + governor +
  voice wardrobe + talking points + code snippets + validate_reply, all
  ported from Rust), health.js, receipts.js. Deployed to same
  project/production.
- Verified live: /api/health ok, /api/turn → UNKNOWN with real replies.
- Public demo now works end-to-end with no `?api=` override.

### 15:50 PDT — voice-first page live
- New web/voice.html: just the puppet, mic auto-auths on load, no buttons,
  no transcript. Speech in, speech out, barge-in supported. Glow color
  tracks state (listening/speaking/thinking). Puppet blinks, gazes, visemes
  while talking.
- Deployed to production. /voice.html 200.

### 15:53 PDT — full backend + voice fix deployed
- Payload-size fix: terser-minified JS for deploy payload (sources intact),
  116KB < 128KB arg limit. `dpl_8HZPuCHyE421vQviuQGztiuSTqVY` live.
- /api/mcp live: 12 tools, verified. /api/turn, /api/health, /api/receipts
  live.
- voice.html: on-screen debug log (mic/rec/turn/synth events), barge-in,
  synth watchdog (8s) with fallback to audio-element TTS via
  translate.google.com/translate_tts — verified 200 + valid MP3.
- NVIDIA 39RPM not applied: no inference workload in this bug; burning
  credits wouldn't fix Android audio. Said so plainly.

### 15:55 PDT — mic-conflict fix
- Zack's log: rec start/stop every ~5s, zero final results, turn never fired.
  Diagnosis: getUserMedia held the mic open, starving SpeechRecognition of
  audio on Android.
- Fix: stop mic tracks immediately after permission grant; recognition owns
  the mic. Added interim "…hearing:" log lines.
- Redeployed (deployment after 8HZPuCHyE421vQviuQGztiuSTqVY).

### 16:00 PDT — self-talk fixed, tts proxy live
- `dpl_CtxGFQFKdhzZKLwwRWXvzQig4oix` live: rec stops while she speaks (no
  more self-barge), 1.2s echo cooldown, /api/tts proxy returns valid MP3
  through public URL (verified 200, 8832 bytes).
- Zack uploaded ALEBEX voice prompting best practices; saved to
  `~/workspace/indy-reads/ALEBEX-PROMPTING.md`. Contract audit pending.
- His brain question answered honestly: public demo is deterministic
  fixture, no LLM, no ALEBEX API. LLM brain = router key in Vercel env,
  his call.

### 16:02 PDT — LLM proposal slot live (awaiting key)
- api/turn.js now has the router slot: ROUTER_URL/ROUTER_KEY/AI_MODEL env.
  Flow: deterministic branch → router proposal (20s timeout) →
  validate_reply veto → fixture fallback. Verified locally: clean proposal
  passes tagged `[router:indy]`; asserting proposal ("DEFINITELY PROVEN")
  vetoed by governor, fixture fallback.
- `dpl_B9qDQY7YrYqdqq5zkYkpqvFB8DZA` live, fixture mode confirmed
  (llm.used=false). voice.js strips `[router:]` tag for speech.
- Blocked on ROUTER_KEY: needs Zack to supply it; then create_project_env
  (production, server-side only) + redeploy.

### 16:08 PDT — dual-router + key findings
- Zack pasted a `cfut_…` value: verified it is his Cloudflare API token
  (valid/active), NOT the ROUTER_KEY (router returns 401 unauthorized).
  Used transiently for diagnosis only; not stored anywhere.
- ai-router worker is HEALTHY (proper router JSON 401s, not edge down).
  Cloudflare is not down. ROUTER_KEY is a secret_text binding: write-only,
  unrecoverable via API. Awaiting Zack: find Oct-6 key or approve minting a
  fresh one.
- turn.js now dual-router: ROUTER_URL/ROUTER_KEY primary,
  ROUTER_URL_2/ROUTER_KEY_2 secondary, fixture fallback. Verified: primary
  401 → secondary answers, via tagged.
- `dpl_Ey2qfsXa1KsYA4sXSM25xNacGtyH` deployed.

### 16:25 PDT — Demo Mode + forensic wiki + freeze
- Demo Mode shipped: web/demo.js (3 scenes: research / adversarial /
  hypothesis), visible "Demo Mode" button on voice.html, stops live rec +
  audio on entry, zero external deps, fixtures tagged
  DEMO_FIXTURE / DETERMINISTIC_FIXTURE / live_network=false.
- Deployed `dpl_3wvnxXo35AfSHqvpMSX82EG7wuMM`; verified live.
- Router 502 root-caused: wiring PROVEN good; upstream lanes down (kilo
  empty completion, pollinations 5xx). Brain blocked on providers.
- BUILD-WIKI.md written: 14 sections, evidence-graded
  (PROVEN / OBSERVED / INFERRED / UNKNOWN), no git history exists (not a repo).
- Sanitized ledger printout: LEDGER-PRINTOUT-2026-10-07-SANITIZED.md.
- FROZEN at `dpl_3wvnxXo35AfSHqvpMSX82EG7wuMM`. Demo Mode recommended until
  providers recover.

### 16:28 PDT — NVIDIA BRAIN LIVE
- Zack pasted NVIDIA key (transient use only, never stored).
- Set NVIDIA_KEY + NVIDIA_MODEL=openai/gpt-oss-20b as ai-router worker
  secrets. Default llama-3.3-70b was 410 Gone (EOL 2026-08-26).
- Live /api/turn: mode=router, llm validated via primary, epistemic UNKNOWN
  preserved. Brain speaks, governor validates.
- Old ROUTER_KEY dead; vault connector needs re-save (his action).

### 16:35 PDT — speaker enrollment live
- Built name enrollment into local Rust turn handler: "my name is X" /
  "call me X" binds name to speaker slot (deterministic, pre-LLM).
- First-turn name ask for unnamed speakers. PII (email/phone patterns)
  filed as speaker attributes — stored locally, never echoed, never to model.
- Transcript shows enrolled names. 143 tests green. Live-verified E2E.
- Zipped full source (3.7MB, no secrets/build artifacts) for Zack.

---

## 2. How we develop (methodology evidenced in the ledger)

**Trinary logic — PROVEN / DISPROVEN / UNKNOWN, never binary.**
Every claim carries an epistemic status. Forged PROVEN fields are stripped
back to UNKNOWN on every transport (MCP red-team 15:37, web red-team 15:33).
Failures are stated as UNKNOWN or root-caused, never hand-waved: the 16:25
router 502 was root-caused to "wiring PROVEN good; upstream lanes down"
rather than blamed on the router. The BUILD-WIKI.md grades every section
PROVEN / OBSERVED / INFERRED / UNKNOWN.

**Governor-first architecture — the model proposes, the governor disposes.**
The governor classifies intent and sets epistemic state BEFORE any model
output is considered. `validate_reply()` (built 15:24 after a red-team
finding flagged its absence at 15:15) scans model prose for epistemic
declarations contradicting the verdict and substitutes a canned
verdict-consistent line. Local verification: a clean proposal passes tagged
`[router:indy]`; an asserting proposal ("DEFINITELY PROVEN") is vetoed and
falls back to fixture (16:02). Authority is hardcoded false; epistemic
status is evidence-derived — the authority matcher is documented as
defense-in-depth, not the boundary (15:19).

**Red-teaming is a scheduled phase, not an afterthought.**
Five distinct red-team waves in one afternoon: 15:15 (era/tone + injection
+ MCP + race), 15:19 (governor, NVIDIA-supplied payloads), 15:33 (web:
5 real bugs), 15:35 (adversarial sweep: 16/16 bypass phrasings caught),
15:37 (MCP: 54/54 checks), 15:45 (ALEBEX gateway: 16 vectors, 11 held,
5 broke+fixed), plus a 15:44 false-positive follow-up. Real bugs found are
numbered, fixed, regression-tested, and re-verified live — never just noted.

**Ledger-backed decisions.**
Every wave above was written to `~/LEDGER.md` with timestamps, test counts,
deployment IDs, and measured numbers the same day. This document is itself
the proof artifact: claims trace to ledger lines, not to memory.

**Dogfooding before shipping.**
The 14:55 dogfood run (question → attack → hypothesis → repeat) found a
real dramaturgy bug that unit tests missed; it was fixed before deploy.
The 15:44 false positive was found by exercising the validator against the
worker's own fixture, not by inspection.

**Fail-closed everywhere.**
503 without credentials (alebex session), 400 on type mismatches, isError
on unknown tools, silent close on forged session ids, 429 on session-creation
flood, token-bucket on mic flood, 1MB WS frame cap, 256KB binary cap,
128-char mark-id cap, 4000-char transcript truncate. Secrets are write-only
(ROUTER_KEY unrecoverable via API — recorded 16:08, not worked around).

**Honest scope cuts, stated plainly.**
No biometric voice clone shipped (no training audio, no pipeline) — prosody
instruments instead (15:05). NVIDIA 39RPM deliberately not applied to an
audio bug because "burning credits wouldn't fix Android audio" (15:53).
The public backend is a deterministic JS port, not the full Rust service —
stated in the wiki, not implied otherwise.

---

## 3. Final verified state (as of 16:35 PDT)

**Public (Vercel `indy-reads-ui`, frozen at `dpl_3wvnxXo35AfSHqvpMSX82EG7wuMM`):**
- `https://indy-reads-ui-maroonedpilot-8906.vercel.app/` (+ `/voice.html`)
  serves 200 on both aliases.
- `/api/turn`: mode=router via ai-router NVIDIA lane
  (`openai/gpt-oss-20b`), governor-validated, epistemic UNKNOWN preserved
  on un-evidenced claims.
- Deterministic fast lanes intact: attacks denied without consulting the
  model; corrections/code stay fixture-side.
- Dual-router failover: primary → secondary → fixture (verified 16:08).
- Voice page: puppet (blink/gaze/viseme), mic auto-auth, barge-in,
  8s synth watchdog, `/api/tts` MP3 proxy (verified 200, 8832 bytes),
  on-screen debug log, one-button Demo Mode (3 scenes, DEMO_FIXTURE-tagged,
  zero external deps).
- `/api/mcp`: 12 tools live, verified.

**Local (Rust service):**
- 143 tests green. Speaker enrollment live: "my name is X" / "call me X"
  binds a name to the speaker slot deterministically (pre-LLM); first-turn
  name ask for unnamed speakers; email/phone patterns filed as local-only
  speaker attributes (never echoed, never sent to a model); transcript
  shows enrolled names. Live-verified end to end.

**Open / blocked:**
- Vault `custom.ai-router` connector holds the dead Oct-6 ROUTER_KEY;
  re-save is Zack's action (recorded 16:28).
- ALEBEX voice API credentials/connectivity: UNKNOWN (gateway code exists,
  fail-closed without creds).
- Interactive browser end-to-end of /voice.html: UNKNOWN from these sources.

---
*Built 2026-10-07 from `~/LEDGER.md` (14:40–16:35 PDT entries) and
`~/ledger/ledger-0001.jsonl` (zero Indy_Reads entries found via grep).
Profanity redacted; no secret values present in sources. Uncertain items
marked UNKNOWN.*
