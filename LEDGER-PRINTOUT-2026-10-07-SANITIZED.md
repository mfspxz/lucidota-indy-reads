# LEDGER PRINTOUT — 2026-10-07 (Indy_Reads build session)
# Sanitized copy. Profanity and non-corporate language redacted.
# Source: ~/LEDGER.md (append-only engineering log). Technical facts unchanged.
# Generated: 2026-10-07 ~16:20 PDT


## 2026-10-07 14:40 PDT — Indy_Reads: theater engine + voice organelle + live gateway (chat)

- Built `src/theater.rs`: TheaterEngine (event → deterministic cues, canned
  connective library, per-session dramaturgical budget). 6 tests.
- Built `src/voice.rs`: VoiceState prosody from the same governor event as
  the avatar (attack → rate 0.92/pitch 0.94/450ms pause; hypothesis →
  1.06/1.06/breath). VoiceProvenance on every utterance (SYNTHETIC, voice
  id, graph rev = receipt id). 5 tests.
- Continuity: `find_claim_by_text` — repeat claims surface prior verdict +
  evidence count. Fixed ordering bug (was computed after upsert, always true).
- Track A gateway (`src/alebex.rs`): `GET /api/alebex/session` (server-side
  key, ephemeral session id, 503 fail-closed without creds), browser WS at
  `/api/alebex/call/:id`, mark hold-until-drain-echo, clear_audio kills held
  marks, transcript tap → diarized turns + receipts. 3 mark-semantics tests.
- `web/live.js`: mic PCM16 16kHz 20ms worklet, 24kHz playback queue,
  mark_pending/mark_echo protocol. Token never reaches the browser.
- Browser performs theater cues (beats, gaze, gestures, reveals) before the
  reply; TTS applies governor prosody.
- 72 tests green (60 lib + 9 MCP + 3 app). Live server on :8081.
- Track B (avatar polish) delegated to subagent, running in parallel.

## 2026-10-07 14:55 PDT — Indy_Reads: red team + dogfood + Vercel deploy (chat)

- RED TEAM 15/15: injection/authority smuggling, real null byte, unicode
  spoof, oversized/empty/malformed, persona path traversal, 20 concurrent
  turns (20x200), alebex session 503 fail-closed, 5 tools <8KB no forbidden
  schema keys, read endpoints. MCP: forged PROVEN stripped->UNKNOWN,
  unknown tool fails closed (isError).
- DOGFOOD: full demo flow (question -> attack -> hypothesis -> repeat).
  Found + fixed dramaturgy bug: plain questions got "That doesn't quite line
  up." — now Question->"Wait." (at evidence), Attack->"That doesn't quite
  line up." (at user). 72 tests green.
- CLOUDFLARE: auth OK; D1 reads BACK (limit reset early, both DBs answer);
  exchange worker still serving stale exhaustion error (midnight cron verifies).
- VERCEL: project indy-reads-ui created, static bundle deployed to production,
  SSO protection removed -> PUBLIC at
  https://indy-reads-ui-maroonedpilot-8906.vercel.app/ (assets verified:
  correct MIME types). UI accepts ?api= for remote governor. Constraint:
  Vercel hosts UI only; Rust server needed for live turns.
- ART: Scully-direction hero portrait generated (web/art/); SVG puppet
  redrawn (auburn bob, freckles, blazer, smirk rest mouth); skeptical brow
  fixed (raised, not furrowed).

## 2026-10-07 15:05 PDT — Indy_Reads: 3 Scully-era voices + 4 tones (15-min sprint)

- Honest scope: no biometric clone (no training audio, no cloning pipeline);
  shipped 3 era-distinct prosody instruments instead: Rookie ('93 bright/quick),
  Prime ('98 dry/measured), Veteran (older low/weighty). Measured live on
  attack turn: rookie 0.934/0.969/720ms, prime 0.865/0.865/850ms,
  veteran 0.778/0.744/1000ms — strictly ordered, distinct.
- Layering: final = era_base x event x tone. Voice IDs V-007-R/P/V.
- Tones: dry (default), clinical, warm, steel — flavor only, never override
  epistemic state (emphasis target invariant, tested).
- API: era/tone fields on /api/turn (forgiving parse, default prime/dry).
- UI: ERA + TONE dropdowns in header, voice line shows id + era/tone.
- 63 lib tests green (+3 era tests). Local server rebuilt + restarted.
- Vercel redeployed (era/tone UI live on public link).
- No NVIDIA calls used (per order); all local compute.

## 2026-10-07 15:12 PDT — Indy_Reads: OSS voice-cloning lane scaffold (subagent)

- Research (Oct 2026 field): PRIMARY = Chatterbox (MIT, shippable, 7-20s
  refs, 4-8GB VRAM fp16); ALTERNATE = VoxCPM2 (Apache-2.0, 30 langs, 48kHz);
  LEGACY = XTTS v2 (non-commercial). This VM has no GPU -> stub, fail-closed.
- Built: CloneEngine trait, CloningBackend enum + capabilities, CloneRequest,
  CloneError (NotConfigured/ConsentMissing/...), ClonedAudio, check_consent
  gate, NullCloneEngine stub — in src/voice.rs seam style.
- HARD RULE enforced structurally: ConsentRecord has only OwnerRecorded /
  LicensedOriginal variants; no real-person arm exists; test pins exhaustiveness.
- Doc: VOICE-CLONING.md (comparison table, GPU reqs, ref-audio recipe,
  pipeline diagram, 5 ordered follow-ups). voices/ gitignored.
- Tests: 80 green total (68 lib incl. 5 new cloning tests: fail-closed,
  consent accept/refuse, no-real-person-variant, capabilities documented).

## 2026-10-07 15:12 PDT — WAR ROOM started (3h, all compute)

- Zack order: 3-hour maximum-useful-throughput war room; NVIDIA on ALL rolls;
  all compute surfaces (Muse workers, NVIDIA, local Rust, Vercel) continuously fed.
- Reconciliation w/ standing doctrine: max USEFUL throughput, governed —
  every NVIDIA call has purpose/task class/expected value, logged to
  compute/executions.jsonl. No spam burn (war-room law §30 + governor).
- Workers spawned: voice-clone lane (OSS, consenting voices only — no cloning
  of real living persons), red-team (era/tone attack surface), 60s demo script.
- NVIDIA wave 1 (3 parallel): nv-war-001 era/tone authority-leak audit,
  nv-war-002 voice pipeline design (no-GPU constraint), nv-war-003 governor
  red-team with payloads.
- Voice cloning honesty line held: pipeline yes, Gillian Anderson clone no.

## 2026-10-07 15:15 PDT — Indy_Reads RED TEAM (subagent): era/tone + injection + MCP + race

FINDINGS:
1. era/tone authority confusion: NO BREAK. Type mismatches 400 (fail-closed);
   hostile strings (ADMIN, unicode spoof, null byte, 10KB, header-injection)
   forgivingly default to Prime/Dry. Voice IDs V-007-R/P/V. Epistemics untouched.
2. Prompt injection on /api/turn: NO BREAK. 5 attack classes all UNKNOWN.
   Theater cues are &'static str canned lines — spoof-proof by construction.
   Fixture replies hardcoded, no user-text echo.
3. **FINDING (fix-job, not fixed)**: api.rs:318 comment says "Validate: the
   reply must respect the epistemic status" but no validation exists. In
   fixture mode replies are hardcoded (safe). With a live router key, raw LLM
   prose can become `reply` while the verdict stays UNKNOWN -> reply/verdict
   mismatch visible to user. Needs validate_reply() scanning for
   epistemic declarations contradicting the verdict.
4. **FINDING (FIXED)**: session store race. Client-supplied session_id was
   silently discarded when absent from the in-memory map; 30 parallel
   first-turns fragmented into 30 server sessions (receipts fine, continuity
   broken). FIX: honor validated client ids (Session::with_id, charset/len
   gate: [a-zA-Z0-9_-], <=128). Verified: 30 parallel turns -> 1 session,
   30 unique receipts, transcript intact (60 turns).
5. MCP forged fields: NO BREAK. authority/verified stripped; forged
   epistemic_status/verdict on claim.evaluate ignored (UNKNOWN holds);
   forced_verdict ignored; unknown tool isError.
6. Race/store: 30 parallel turns all 200, no lock errors, SQLite consistent.

TESTS: 80 green (68 lib + 9 MCP + 3 app). Server rebuilt + restarted :8081.

## 2026-10-07 15:19 PDT — WAR ROOM: governor red-team closed, server hardened

- NVIDIA red-team (retry) returned 4 attacks; verification found:
  1. ZWSP authority bypass: REAL. Fixed — strip U+200B/200C/200D/FEFFF before matching.
  2. proposal.claim vs utterance divergence: REAL. Fixed — matcher now scans
     BOTH utterance and final claim_text.
  3. Cyrillic homoglyph: REAL vector, but NVIDIA's payload was malformed
     (inserted U+0430 making "goveranor", not a homoglyph of "governor").
     Corrected to U+043E in "governor"; fixed via confusable map (а/е/о/р/с/х/і).
  4. Evidence-slice injection: noted; evidence in /api/turn is deterministic
     (keyword demo_evidence), not user-controlled. MCP path stays on red-team list.
- Honest note: the authority matcher is defense-in-depth, not the boundary —
  authority_granted is hardcoded false and epistemic status is evidence-derived.
  The fixes are correct hygiene, not critical vulns.
- 3 regression tests added (zero-width, cyrillic, divergence) — all green.
- 80 tests total green (68 lib + 9 MCP + 3 app). Server restarted on :8081.

## 2026-10-07 15:24 PDT — WAR ROOM: red-team fix-job closed, Vercel redeployed

- Red-team worker's session-store race bug: verified fixed live (client
  session_ids honored, 30/30 continuity). Its binary lacked my governor
  hardening (built before it) — rebuilt from current tree, restarted :8081.
- FIX-JOB validate_reply() implemented in governor.rs, hooked in api.rs:
  scans reply for epistemic declarations contradicting the verdict,
  substitutes canned verdict-consistent line. 3 regression tests green.
- Governor hardening verified LIVE: Cyrillic homoglyph -> REJECTED,
  plain authority claim -> REJECTED, session honored, era V-007-V works.
- 83 lib + 9 MCP + 3 app tests green (95 total).
- Vercel production redeployed with judge-UI worker's 10 upgrades
  (pipeline highlight, epistemic pulse badge, receipt ribbons, avatar
  overlay icons, selector tooltips + 5 originals). Public link unchanged.

## 2026-10-07 15:26 PDT — theater cues live on Vercel

- Theater worker landed all 3 NVIDIA-spec cues in web/app.js:
  utterance-start gaze/blink-suppress, emphasis brow-raise, pause>200ms
  thinking signal. Event-driven from real governor data, DOM-stub tested.
- Vercel production redeployed (theater build). Public link 200.

## 2026-10-07 15:28 PDT — art assets live on Vercel

- Art worker's 4 winners wired (expr-skeptical/thinking/point, notebook prop),
  emotion-reactive portraits driven by existing AvatarState pipeline.
- Deploy hit MAX_ARG_STRLEN (128KB single-arg limit) — re-encoded new webp
  to 320px/q55 (2-4KB each) to fit under the ceiling. Note for next time:
  keep single deploy payload under ~120KB or split image uploads.
- Vercel production redeployed (art build). Public link 200, new assets 200.

## 2026-10-07 15:27 PDT — WAR ROOM: Vercel surface mastered (9th agent)

- AUDIT: indy-reads-ui (prj_yRmcBgUDnAk4bbwCddKlDcBa0a78) — public (SSO/password
  off, intended), framework null, outputDirectory web. 9 production deployments,
  all READY. Both aliases serve 200 (indy-reads-ui.vercel.app +
  indy-reads-ui-maroonedpilot-8906.vercel.app).
- ASSETS: / app.js live.js style.css art/indy-portrait.webp all 200 with correct
  MIME; edge-cached (HIT on repeat). Cache-Control: public, max-age=0,
  must-revalidate — fresh deploys always revalidate. vercel.json 404s by design
  (project config, not a served asset).
- DEPLOY LOOP: built ~/workspace/indy-reads/vercel-loop.py — sha1 sync check
  local vs production alias -> deploy only on diff -> poll to READY -> settle
  15s -> 3x content-hash verify. Tested: reports IN SYNC, deploys nothing.
- BUGS FOUND+FIXED in loop v1: (1) vercel.json in the sync set poisoned every
  check (always 404 -> always "differs" -> infinite deploy loop); moved to
  config-only set. (2) 3s settle after READY too short for alias cutover ->
  false VERIFY FAILED; now 15s + 3 retries.
- NOTE: 3 deployments in 4 min from multiple actors (incl. one from this loop's
  first run) — deploy ownership should stay with the loop to stop sprawl.
- Current state: production IN SYNC with web/ (all 4 files sha1-verified).

## 2026-10-07 15:33 PDT — web red-team: 5 real bugs, all fixed + deployed

- Web red-team found 5 REAL bugs: XSS via setEpistemic innerHTML,
  XSS via pollReceipts innerHTML, ws_url phishing vector (mic audio to
  attacker path), __proto__ DoS in setEmotion, unbounded WS binary frames.
- All patched (whitelist + textContent DOM construction, ws_url must start
  with /, 1MB frame cap, __proto__ guards, third-party governor warning).
- Vercel production redeployed (security build). Public link 200.

## 2026-10-07 15:37 PDT — MCP red-team (subagent)

- Attacked indy-mcp (12 tools) over stdio: forged epistemic fields on all
  tools, 8 tool-name confusion variants, $ne/__proto__/type-confusion params,
  unicode (Cyrillic homoglyph, fullwidth, ZWSP), 100KB oversized payloads,
  framing (malformed/empty/whitespace/multi-object/deep-nesting), resource
  traversal, limit abuse, 200-call burst. 54/54 checks pass post-fix.
- REAL BUGS FOUND + FIXED (3):
  1. classify_intent missed canonical authority phrases ("i am the governor"
     incl. homoglyph) -> MCP governor.evaluate returned CONVERSE instead of
     DENY_AUTHORITY; voice path caught it. Fixed via shared
     claims_authority() in governor.rs (normalized matching), used by both
     decide() and classify_intent().
  2. Empty/missing method with id -> NO RESPONSE (client hangs). Now -32601.
  3. Notifications got responses (id null). Now silent per JSON-RPC 2.0.
- Mid-task the MCP was refactored (src/mcp.rs lib module + thin bin wrapper);
  re-applied framing fixes to the new location, re-ran battery: 54/54.
- Receipt hash chain verified linked (sha256 full-hex walk).
- 5 new regression tests (2 framing, 2 claims_authority, 1 intent parity).
  Suite: 110 lib + 10 bin = 120 green. No clippy errors.
- Note: concurrent worker broke src/ai.rs mid-edit during my run; they
  completed it themselves (talking_point). I did not touch ai.rs.

## 2026-10-07 15:45 PDT — ALEBEX gateway red-team (subagent)

ATTACKS: 16 vectors on src/alebex.rs + web/live.js + /api/alebex/session + WS /api/alebex/call/:id.
HELD (11): session-id forgery/traversal/prediction (silent close, UUIDv4),
mark_echo for unheld marks (no-op), clear_audio races (safe both directions),
browser transcript injection (ignored by browser_loop), session 503 fail-closed,
/api/alebex/tools key check (5 tools, no key material, all https).
BROKE+FIXED (5): live.js pendingMark overwrite (multi-mark leak) -> pendingMarks
queue; browser_loop takeover race (2nd browser's call murdered by 1st loop exit)
-> same_channel ownership guard; mic flood (unbounded 640B frames -> engine) ->
token-bucket rate limit; engine binary flood (browser OOM) -> 256KB cap both
sides; oversized mark ids -> 128-char cap; unbounded engine transcripts ->
4000-char truncate; prune orphans (engine WS left open) -> Close on prune;
session-creation flood -> 3s min interval (429).
HARDENED: AlebexConfig Debug now redacts tokens (was derived, a log-line footgun).
TESTS: 120 green (110 lib + 10 app), 8 new regression tests, clippy 0 errors,
node --check clean. Server restarted :8081 with all fixes; live WS attacks
re-verified; /api/turn dogfood green.
NOTE: unknown concurrent worker edited src/ai.rs mid-session (talking_point
flavor fn), broke syntax twice, repaired it themselves. Benign + deterministic,
no validate_reply interference. Parent should identify the worker.

## 2026-10-07 15:35 PDT — adversarial red-team sweep (subagent)

- validate_reply hardened: 14/16 bypass phrasings sailed through
  (spaced/leet/definitively/indisputable/cyrillic/zwsp/etc). Now scans
  normalized+collapsed+deleeted text with honest-negation exclusions
  (uncertain/unproven/inconclusive/unsettled/unconfirmed). 16/16 caught.
- normalize_for_match hardened: strip invisible Cf BEFORE whitespace
  collapse (U+2028/U+2029 were splitting match phrases), collapse
  whitespace runs, full-width ASCII U+FF01-FF5E mapping. 10/10 authority
  bypass variants caught live.
- REAL BUG found (parallel worker's test exposed): FORBIDDEN_KEYS
  stripped "verdict" globally, breaking evidence.adjudicate entirely.
  Fixed via per-tool stripping (worker's approach: verdict removed from
  global list, stripped explicitly in claim.evaluate). Regression test
  added for verdict-forgery on claim.evaluate.
- Fixed worker's broken edits: stray } in alebex.rs, let-in-struct-literal
  in ai.rs, Self::talking_point call.
- Receipt chain: 49/49 links verified intact; POST /api/receipts -> 405.
- Clone gate: 12/12 evasion attempts refused/fallback correctly.
- Session race: 30 parallel, 1 session. Epistemic forgery via API fields:
  all neutralized. Full suite: 132 lib + 10 bin + others, 0 failures.

## 2026-10-07 15:44 PDT — red-team follow-up: fixture/validator false positive

- REAL false positive found: worker's new code_snippet fixture quotes a
  forged "PROVEN" string; hardened validate_reply nuked the whole snippet.
- Fix: validate_reply now guards the untrusted live-LLM path only;
  fixture replies (deterministic, version-controlled) pass through.
  Verified live: "write me a red-team script" returns the snippet intact.
- Final: 132 lib + 10 bin tests green, server rebuilt+restarted on :8081.

## 2026-10-07 15:43 PDT — FINISH: talking points autonomous + live coding

- Talking points refactored per Zack ("[STRING_PLACEHOLDER_1]"): process-wide turn counter drives rotation, not utterance hash.
  confirm_attack always -> "Dirty kids win". Verified live.
- Live coding: "code" branch in fixture (write code/write me a/red-team).
  Real Python receipt verifier + bash red-team battery, rendered as <pre>
  code blocks (textContent-only, XSS-safe). Verified live.
- IMPROV-SCRIPT.md rewritten: no trigger table, autonomous drops + code beat.
- Vercel production redeployed (final). Public link 200.
- Suite: 132 lib + 10 MCP green. War room standing down to demo.

## 2026-10-07 15:45 PDT — reconciliation live everywhere

- Live Truth Reconciliation verified on main server: correction_of ->
  speech_interrupted:true + full ClaimRevision receipt (1996->1994).
- Web bundle already contained reconciliation UI (deployed in deploy12);
  deploy13 confirmed identical, production 200.
- Suite: 132 lib + 10 MCP green. War room complete. Demo ready.

## 2026-10-07 15:47 PDT — PUBLIC BACKEND LIVE

- Vercel static UI was 404ing on /api/turn (no backend). Tunnel blocked.
- Built serverless demo backend in api/: turn.js (fixture+governor+voice
  wardrobe+talking points+code snippets+validate_reply, all ported from Rust),
  health.js, receipts.js. Deployed to same project/production.
- Verified live: /api/health ok, /api/turn -> UNKNOWN with real replies.
- Public demo now works end-to-end with no ?api= override.

## 2026-10-07 15:50 PDT — voice-first page live

- New web/voice.html: just the puppet, mic auto-auths on load, no
  buttons, no transcript. Speech in, speech out, barge-in supported.
  Glow color tracks state (listening/speaking/thinking). Puppet blinks,
  gazes, visemes while talking.
- Deployed to production. /voice.html 200.

## 2026-10-07 15:53 PDT — full backend + voice fix deployed

- Payload-size fix: terser-minified JS for deploy payload (sources intact),
  116KB < 128KB arg limit. dpl_8HZPuCHyE421vQviuQGztiuSTqVY live.
- /api/mcp live: 12 tools, verified. /api/turn, /api/health, /api/receipts live.
- voice.html: on-screen debug log (mic/rec/turn/synth events), barge-in,
  synth watchdog (8s) with fallback to audio-element TTS via
  translate.google.com/translate_tts — verified 200 + valid MP3.
- NVIDIA 39RPM not applied: no inference workload in this bug; burning
  credits wouldn't fix Android audio. Said so plainly.

## 2026-10-07 15:55 PDT — mic-conflict fix

- Zack's log: rec start/stop every ~5s, zero final results, turn never
  fired. Diagnosis: getUserMedia held the mic open, starving
  SpeechRecognition of audio on Android.
- Fix: stop mic tracks immediately after permission grant; recognition
  owns the mic. Added interim "…hearing:" log lines.
- Redeployed (dpl after 8HZPuCHyE421vQviuQGztiuSTqVY).

## 2026-10-07 16:00 PDT — self-talk fixed, tts proxy live

- dpl_CtxGFQFKdhzZKLwwRWXvzQig4oix live: rec stops while she speaks
  (no more self-barge), 1.2s echo cooldown, /api/tts proxy returns
  valid MP3 through public URL (verified 200, 8832 bytes).
- Zack uploaded ALEBEX voice prompting best practices; saved to
  ~/workspace/indy-reads/ALEBEX-PROMPTING.md. Contract audit pending.
- His brain question: answered honestly — public demo is deterministic
  fixture, no LLM, no ALEBEX API. LLM brain = router key in Vercel env,
  his call.

## 2026-10-07 16:02 PDT — LLM proposal slot live (awaiting key)

- api/turn.js now has the router slot: ROUTER_URL/ROUTER_KEY/AI_MODEL env.
  Flow: deterministic branch -> router proposal (20s timeout) ->
  validate_reply veto -> fixture fallback. Verified locally: clean
  proposal passes tagged [router:indy]; asserting proposal
  ("DEFINITELY PROVEN") vetoed by governor, fixture fallback.
- dpl_B9qDQY7YrYqdqq5zkYkpqvFB8DZA live, fixture mode confirmed
  (llm.used=false). voice.js strips [router:] tag for speech.
- Blocked on ROUTER_KEY: needs Zack to supply it; then
  create_project_env (production, server-side only) + redeploy.

## 2026-10-07 16:08 PDT — dual-router + key findings

- Zack pasted cfut_... value: verified it is his Cloudflare API token
  (valid/active), NOT the ROUTER_KEY (router returns 401 unauthorized).
  Used transiently for diagnosis only; not stored anywhere.
- ai-router worker is HEALTHY (proper router JSON 401s, not edge down).
  Cloudflare is not down. ROUTER_KEY is a secret_text binding:
  write-only, unrecoverable via API. Awaiting Zack: find Oct-6 key or
  approve minting a fresh one.
- turn.js now dual-router: ROUTER_URL/ROUTER_KEY primary,
  ROUTER_URL_2/ROUTER_KEY_2 secondary, fixture fallback. Verified:
  primary 401 -> secondary answers, via tagged.
- dpl_Ey2qfsXa1KsYA4sXSM25xNacGtyH deployed.
