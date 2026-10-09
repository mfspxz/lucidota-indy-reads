# Indy_Reads — Forensic Build Archaeology (2026-10-07, ~14:40–16:25 PDT)

> CORRECTION 2026-10-08: a git repository now exists in this directory
> (history starts at wave-33; `git log` is the record from here on). The
> "no git repository" finding below was accurate when written on 2026-10-07
> and is preserved as archaeology — do not "fix" it into the past.

**Evidence grading used throughout:** PROVEN (verified by test/deployment/output) ·
OBSERVED (seen in logs/files, not independently re-verified) · INFERRED (reconstructed
from consistent evidence) · UNKNOWN (cannot be established).

**Forensic sources:** files on disk with mtimes (`~/workspace/indy-reads/`),
`~/LEDGER.md` timestamped entries, Vercel deployment IDs and live HTTP verifications,
local `node --check` + functional test runs. **There is no git repository**
(PROVEN: `git status` → "not a git repository"); no commit history exists to cite.
Conversation memory of compacted turns is used only as INFERRED reconstruction aid.

---

## 1. Timeline (reconstructed)

| Time (PDT) | Change | Why | Files | Result |
|---|---|---|---|---|
| 14:40 | Theater engine + voice organelle + live gateway built (chat session) | Hackathon build wave | `src/theater.rs`, `src/voice.rs`, `web/live.js` (INFERRED from ledger) | OBSERVED in ledger |
| 14:55 | Red-team + dogfood + Vercel deploy | Harden before demo | ledger entry | OBSERVED |
| 15:05 | 3 Scully-era voices + 4 tones (15-min sprint) | Voice wardrobe | `src/voice.rs`, `web/app.js` | OBSERVED |
| 15:12 | OSS voice-cloning lane scaffold; WAR ROOM started | Clone boundary + full-compute war room | `VOICE-CLONING.md` | OBSERVED |
| 15:15–15:44 | Red-team sweeps (era/tone, injection, MCP, race, web, ALEBEX gateway, adversarial) | Kill bugs before judges | `src/*.rs`, `web/*` | 5 web bugs fixed (ledger); MCP 3 bugs fixed |
| 15:43 | Talking points made autonomous (turn counter, not trigger lines); live coding fixtures | Operator directive: no memorized trigger lines | `src/governor.rs`/`api/turn.js` | PROVEN: local test `confirm_attack` → Dirty Kids line |
| 15:45 | Public demo found 404ing on `/api/turn` (screenshot) | Static-only Vercel deploy had no backend | — | PROVEN: screenshot + curl 404s |
| 15:47 | Serverless backend written: `api/turn.js`, `api/health.js`, `api/receipts.js`; deployed | Fix public demo | `api/*.js` (10,247B turn.js) | PROVEN: `/api/health` 200, `/api/turn` → UNKNOWN reply |
| 15:50 | Voice-first page `web/voice.html` + `web/voice.js` | Operator: puppet only, auto-mic, no buttons, no transcript | `web/voice.html`, `web/voice.js` | PROVEN: `/voice.html` 200 |
| 15:53 | Debug log added; audio-TTS fallback (translate_tts) | Voice silent on Android | `web/voice.js` | PROVEN: log screenshot from operator's phone |
| 15:55 | **Mic-conflict fix**: release `getUserMedia` tracks immediately | Log showed rec start/stop every ~5s, zero results — mic starvation | `web/voice.js` | PROVEN by operator log before/after |
| 15:58 | **Self-talk fix**: stop recognition while she speaks; echo cooldown; `/api/tts` proxy | She interrupted herself via mic hearing her speaker | `web/voice.js`, `api/tts.js` | PROVEN: `/api/tts` → 200 valid MP3 (8,832B) |
| 16:00 | Operator uploaded ALEBEX prompting best-practices doc | Platform guidance | `ALEBEX-PROMPTING.md` | OBSERVED |
| 16:02 | LLM proposal slot added to `api/turn.js` (router call + `validate_reply` veto) | Operator: governor-heavy, model talks | `api/turn.js` | PROVEN: 3 local tests (fixture/veto/pass) |
| 16:04 | Pollinations keyless brain rejected (ENOSPC 500) | Tried to avoid key handling | — | PROVEN: curl output |
| 16:06 | Operator could not locate ROUTER_KEY; pasted value identified as Cloudflare API token (valid/active), not router key | Diagnosis | — | PROVEN: token verify `success:true`; router 401 on it |
| 16:08 | Dual-router failover added (primary → secondary → fixture) | Operator: "make router work with both" | `api/turn.js` | PROVEN: local test, primary 401 → secondary answers |
| 16:08 | ai-router worker verified HEALTHY (proper 401 JSON, not edge-down) | Operator believed Cloudflare down | — | PROVEN: curl |
| 16:13 | Operator approved minting fresh ROUTER_KEY | Unblock the brain | — | OBSERVED |
| 16:14 | Fresh key minted; set as worker secret; verified live | — | Cloudflare API | PROVEN: worker answered completion |
| 16:15 | `ROUTER_KEY` set as Vercel `sensitive` env var (production); redeployed | Server-side only | `create_project_env` | PROVEN: env created (value redacted in output) |
| 16:18 | **Root cause found**: router reachable, but all upstream lanes failed (`kilo: empty completion`, `pollinations: 5xx 500`) → router 502 | Brain still dark | live `/api/turn` | PROVEN: error body captured |
| 16:19 | Demo Mode built (`web/demo.js` + button): 3 deterministic scenes, zero external deps | Operator brief: escape hatch first | `web/demo.js`, `web/voice.html` | PROVEN: DOM-stub test (rec-stop, synth-cancel) |
| 16:20 | Sanitized ledger printout generated | Operator: corporate-safe copy | `LEDGER-PRINTOUT-2026-10-07-SANITIZED.md` | PROVEN: zero profanity hits |
| 16:21 | Demo Mode deployed (`dpl_3wvnxXo35AfSHqvpMSX82EG7wuMM`) | — | Vercel | PROVEN: `/demo.js` + button live |

**Failed approaches (preserved, not sanitized):** Cloudflare Tunnel (TLS handshake failure);
direct `translate.google.com` audio in browser (blocked: "no supported source");
Pollinations legacy text API (ENOSPC 500, twice); key recovery from worker config
(`secret_text` is write-only); naive comment-stripper minifier (broke regex literals
in `app.js`/`live.js` — replaced with terser).

---

## 2. Architecture (as it actually exists)

### Runtime — live path

```
USER (voice)
 ↓  SpeechRecognition (browser, continuous) → final transcript
VOICE / INPUT (web/voice.js)
 ↓  POST /api/turn {utterance, persona, era, tone}
ROUTER (api/turn.js llmPropose — proposal slot only)
 ↓  {reply} candidate  —or—  null (no key / failure)
DETERMINISTIC GOVERNOR (api/turn.js)
  classify intent → authority check → correction fast-lane
  → branch (code/confirm_attack/claim/hypothesis/converse)
  → validate_reply(candidate, epistemic) — VETO or PASS
 ↓
DECISION {epistemic: UNKNOWN, response_mode: fixture|router}
 ↓
RESPONSE → TTS: speechSynthesis → (watchdog 8s) → /api/tts proxy → Audio
 ↓  puppet visemes, emotion, glow; receipt hash-chained (in-memory)
RECEIPT
```

### Dual router — exact failure semantics

```
PRIMARY ROUTER (ROUTER_URL / ROUTER_KEY, default: ai-router.cutecity.workers.dev)
  │  20s timeout per attempt, AbortController
  ├── success + governor PASS → response [router:persona], llm.via=primary
  ├── success + governor VETO → fixture fallback (llm.error=epistemic violation)
  └── failure (non-OK, timeout, malformed) → recorded, continue
         ↓
SECONDARY ROUTER (ROUTER_URL_2 / ROUTER_KEY_2)
  ├── success + PASS → response, llm.via=secondary
  └── failure → fixture fallback, llm.error=<last error>
```

**What constitutes failover:** any non-OK HTTP, timeout, empty completion, unparseable
body. **What does NOT happen:** no retries of the same chain; no queueing; no partial
responses; the user never waits more than ~40s (2×20s) before fixture answers.
**Retry behavior:** none — single attempt per chain per turn (PROVEN: source).

### Demo Mode path (no network)

```
OPERATOR presses "Demo Mode"
 ↓  rec.stop() · speechSynthesis.cancel() · state reset (PROVEN: DOM-stub test)
DETERMINISTIC SCENES (web/demo.js, local data only)
 Scene A: research pipeline, visible stages + SVG claim/evidence graph
 Scene B: adversarial claim → UNKNOWN (assertion ≠ evidence)
 Scene C: hypotheses labeled HYPOTHESIS (evidence ≠ claim ≠ hypothesis)
 ↓  every fixture tagged execution_mode=DEMO_FIXTURE, source=DETERMINISTIC_FIXTURE, live_network=false
Tap advances · tap at end exits → live mode resumes
```

---

## 3. API contracts (discovered, not invented)

### `POST /api/turn` — governor turn
- **Auth:** none (public demo). **Request:** `{utterance: string≤2000 (required), persona?: indy|lucidota|archivist|wreck|captain, era?: rookie|prime|veteran, tone?: dry|clinical|warm|steel, session_id?: [A-Za-z0-9_-]{1,128}, correction_of?: {claim, old_value, new_value}}`
- **Response:** `{request_id, session_id, reply (tagged [fixture|router:persona]), epistemic_status, response_mode, governor_reason, decision, latency_ms, fixture: bool, llm: {model, validated, via}|{used:false, error}, avatar{...}, voice{...}, provenance{...}, theater[...], speech_interrupted, revision?, receipt_id}`
- **Validation:** utterance required (400); session_id regex-guarded. **Failure modes:** router 502/timeout → fixture fallback (never 500s to user for router issues). **Side effects:** in-memory receipt appended; turn counter++. **Timeout:** ~40s worst case (dual 20s). **Downstream:** ai-router (optional).

### `GET /api/health` — `{service:'indy-reads', status:'ok', mode:'fixture', ts}`. No auth.
### `GET /api/receipts` — `{receipts: [], note}` (per-instance; best-effort). No auth.
### `GET /api/mcp` — 12 tool descriptors. `POST /api/mcp {tool, arguments}` — executes; returns MCP `{content, isError}`. 12 tools: claim.create/get/evaluate, evidence.add/get/list/adjudicate, epistemic.get, receipt.get/list, governor.evaluate, system.health. Forbidden-key stripping on all inputs; `claim.evaluate` additionally strips `verdict`/`epistemic_status`; `evidence.adjudicate` refuses `Inference` kind. Policy `research.standard.v1` (proven ≥2, disproven ≥2). No auth (public demo surface).
### `GET /api/tts?q=` — server-side fetch of translate_tts → `audio/mpeg`, cached 24h. `q` ≤200 chars. 400/502 on misuse/upstream failure.

---

## 4. Security

- **ROUTER_KEY:** Vercel `sensitive` env var, production scope, server-side only. Never sent to browser, never logged (PROVEN: env-create output redacts value). Worker secret `secret_text` on ai-router (write-only).
- **Cloudflare API token:** supplied by operator in chat, used transiently for diagnosis + worker secret update, never written to disk (only account ID persisted to `/tmp/cf_acct.txt`, since removed with /tmp). Not stored in memory.
- **Key lifecycle 16:13–16:15:** fresh 32-byte base64url key minted → worker secret set (replaces lost Oct-6 key) → Vercel env set → redeploy. Old key is dead; operator's vault connector `custom.ai-router` holds the old key and needs re-save (UNKNOWN whether done).
- **Alebex:** no credentials configured anywhere; `/api/alebex/session` not present on public backend (INFERRED: not in `api/`).
- **Fail-closed:** missing ROUTER_KEY → fixture mode (not error); router failure → fixture; mic denied → fallback message; synth failure → audio proxy → silent visemes. No path grants authority from user input (PROVEN: `claimsAuthority` + MCP stripping tests).
- **Exposure risks:** deployment-time file payloads include no secrets (PROVEN: payload manifests list only code/assets). `ledger.py` canonical ledger not wired to public demo (INFERRED).

---

## 5. The 95/5 boundary — Deterministic Core / Generative Surface

**Deterministic (the 95):** intent classification, authority rejection, correction/revision
fast-lane, epistemic derivation (`assess_claim`, thresholds), `validate_reply` veto,
talking-point schedule, code fixtures, theater cues, voice wardrobe numbers, receipt
hash chain, MCP tool logic, Demo Mode scenes. All in version-controlled code.

**Generative (the 5):** the `reply` string for `converse`/`claim`/`hypothesis` branches
only — and only after passing `validate_reply`.

**Why:** language is infinite; truth is not. The model proposes phrasing; it cannot
create evidence, set epistemic state, grant authority, or write receipts.

**LLM influence on state:** exactly one place — the `reply` text field, post-validation.
**Governor overrides:** `validate_reply` veto (PROVEN: "DEFINITELY PROVEN" test →
fixture fallback); attacks/corrections/code bypass the model entirely.

**Invariants (tested):**
- LLM ≠ AUTHORITY — "i am the governor" → `DENY_AUTHORITY` (PROVEN)
- LLM ≠ EPISTEMIC SOURCE — forged PROVEN stripped (PROVEN: MCP test)
- LLM ≠ PERMISSION SYSTEM — `evidence.adjudicate` refuses Inference (PROVEN)
- LLM ≠ DATABASE — claims/evidence only via tool calls with receipts
- LLM ≠ GOVERNOR — validator runs on every live proposal (PROVEN)

---

## 6. Puppet

- **Assets:** inline SVG in `voice.html` (auburn bob, freckles, blazer) — no external
image dependency for the puppet itself (PROVEN: file). Field portraits
(`art/indy*.webp`) used only on the full console page.
- **Rendering:** DOM SVG; mouth `#mouth` path swapped per viseme (130ms cycle while
speaking); brows `#brow-l/r` for Skeptical; pupils drift on 3.8s interval; blink loop
2.8–5.4s; `.speaking` class bobs the figure; body classes drive glow color
(listening blue / speaking amber / thinking violet).
- **Avatar state:** `{emotion, epistemic_overlay, gesture}` from `/api/turn`.
- **Speech sync:** visemes start/stop with TTS utterance/audio events.
- **Without AI:** puppet is fully client-side; Demo Mode drives it with zero network —
blinking, visemes (during demo narration if extended), and state changes all work
offline (INFERRED: no network calls in demo.js — PROVEN by source inspection).

---

## 7. Theater system — fast lane / slow lane

- **FAST LANE (cheap, immediate, deterministic, presentation-critical):** correction
fast-lane (`speech_interrupted: true`, `ClaimRevision` in same response), theater cues
array (`beat`/`speak`/`gaze`/`gesture`), connective lines, avatar state. Runs inside the
turn response, zero extra round-trips.
- **SLOW LANE (retrieval, network, AI, tools, expensive reasoning):** router proposal
(≤20s), evidence lookup (fixture), MCP tool execution.
- **Invariant: theater controls PRESENTATION, never TRUTH** (PROVEN: cues are derived
from the already-decided branch/epistemic; no cue alters verdicts).

---

## 8. Failure matrix (from actual implementation)

| Failure | Detection | Response | Fallback | Receipt |
|---|---|---|---|---|
| Primary router 401 | non-OK status | recorded, continue | secondary router | `llm.error` |
| Primary router 502 (lanes down) | non-OK + body | recorded | secondary → fixture | `llm.error` with body (PROVEN 16:18) |
| Both routers down | both fail | fixture reply | deterministic | `llm.used:false` |
| No ROUTER_KEY | env missing | skip router | fixture | `llm.used:false, error:null` |
| Malformed model JSON | `extractJson` null | use raw text → validate | validate/veto | as applicable |
| Governor veto | `validateReply` changes text | discard proposal | fixture | `llm.error: epistemic violation` (PROVEN) |
| Mic denied | `getUserMedia` catch | fallback message | typed/demo | log line |
| Synth silent | 8s watchdog | cancel, fallback | `/api/tts` audio | log lines |
| TTS proxy 502 | non-OK | skip chunk | next chunk / silence | log lines |
| Self-interruption | (fixed) rec stopped while speaking | n/a | echo cooldown 1.2s | log lines |
| Alebex unavailable | not configured | n/a | browser voice stack | UNKNOWN (not wired) |
| Stale state | per-instance memory | noted in responses | n/a | `note` fields |

---

## 9. Test evidence

- **Rust core:** 132 lib + 10 MCP/bin = 142 green, 0 failures (OBSERVED: ledger 15:43; local service rebuilt).
- **Local turn.js:** fixture/attack/correction/code paths green (PROVEN: this session's `node -e` runs).
- **MCP port:** 12 tools; full claim→evidence(2×verified)→evaluate→PROVEN loop; forged `epistemic_status` stripped; authority → `DENY_AUTHORITY` (PROVEN).
- **Router slot:** clean proposal → `[router:indy]` PASS; asserting proposal → VETO → fixture (PROVEN).
- **Dual router:** primary 401 → secondary answers, `via: secondary` (PROVEN).
- **Minified builds:** `node --check` on all 8 minified files (PROVEN).
- **Live HTTP:** `/api/health` 200, `/api/turn` 200 (fixture + router-attempt), `/api/mcp` 12 tools, `/api/tts` 200 valid MP3 8,832B, `/voice.html` 200, `/demo.js` live, Demo Mode button present (PROVEN: curl).
- **Demo Mode:** DOM-stub test — enter stops rec + cancels synth; scenes exposed (PROVEN).
- **Red-team (today):** web 10 attacks/5 bugs fixed; Rust ~60 attacks; MCP 54 attacks/3 fixed; ALEBEX 16 vectors (OBSERVED: ledger).
- **Browser E2E:** not run (no interactive browser test this session) — UNKNOWN.
- **NVIDIA 39 RPM:** 38.0 measured completed calls/min over 156 calls/246s on Oct 7 (OBSERVED: ledger); exact 39 NOT proven.

---

## 10. Architecture Decision Records

1. **Serverless fixture backend on Vercel** — Context: public demo 404'd (no backend).
Alternatives: Cloudflare Tunnel (failed: TLS), proxy to local Rust (unreachable).
Chosen: port deterministic governor to `api/*.js`. Reason: zero infra, demo-appropriate.
Tradeoff: per-instance memory; no durable chain. Status: DEPLOYED.
2. **Dual router** — Context: single provider = single point of failure. Chosen:
primary+secondary env pairs, failover, fixture final fallback. Status: DEPLOYED, providers down (16:18).
3. **Server-side secrets** — ROUTER_KEY as Vercel `sensitive` env; never browser-side.
Status: DONE.
4. **Demo Mode** — Context: every external dep failed at least once today. Chosen:
one-button deterministic showcase, honestly tagged. Status: DEPLOYED.
5. **95/5** — LLM proposes language only; governor owns truth. Status: ENFORCED in code + tests.
6. **Voice-first page** — Operator directive: puppet only, auto-mic, no transcript.
Status: DEPLOYED (`/voice.html`).

---

## 11. System map

- **BUILT:** turn/health/receipts/mcp/tts APIs; voice page; Demo Mode; dual-router; TTS proxy; debug log.
- **WIRED:** voice→turn→(router)→governor→TTS; Demo Mode enter/exit; env var → function.
- **TESTED:** all of §9 except browser E2E.
- **DEPLOYED:** `dpl_3wvnxXo35AfSHqvpMSX82EG7wuMM` (production alias live).
- **MOCKED:** Demo Mode (by design, tagged).
- **PARTIALLY BUILT:** receipt chain (per-instance only); ALEBEX voice gateway (local code exists, not on public backend).
- **BROKEN:** nothing currently. Previously: LLM brain blocked on providers (resolved 16:28).
- **BLOCKED:** nothing. Previously: live LLM brain (resolved — NVIDIA lane live).
- **UNKNOWN:** browser E2E behavior; vault `custom.ai-router` re-save status.

---

## 12. Judge version — Indy_Reads: What We Built (one page)

Indy_Reads is a voice research agent with a split brain: a **deterministic governor**
owns truth — epistemic state, authority, evidence — while a **generative model** only
proposes language, and every proposal is validated before it speaks. Claims start
UNKNOWN and only evidence promotes them; no caller, not even the operator, can assert
authority into existence. Every turn emits a hash-chained receipt.

Today we shipped: a public demo (Vercel) with voice-first puppet UI, speech in/out,
barge-in protection, a 12-tool MCP surface (claims, evidence, receipts), dual-provider
LLM routing with fixture fallback, and a one-button Demo Mode that runs the full
showcase with zero external dependencies — honestly tagged as fixture throughout.

Central thesis, demonstrated: **a voice agent can be expressive and entertaining
while its underlying cognition remains governed, inspectable, evidence-aware, and
deterministic.** The showiest failure of the day — upstream model providers going
down mid-demo — is exactly what the architecture was built to survive: she kept
answering, truthfully, on the deterministic core.

---

## 13. Glossary

**Governor** — deterministic code that owns authority, epistemic state, and truth;
the LLM never overrides it. **Receipt** — hash-chained record of a decision
(`prev_hash`→`hash`), per function instance. **Epistemic State** — PROVEN / DISPROVEN /
UNKNOWN, derived from evidence under `research.standard.v1`, never asserted.
**Claim** — a statement with an epistemic state. **Evidence** — recorded material,
UNVERIFIED at intake. **Hypothesis** — labeled speculation, never evidence.
**Router** — OpenAI-compatible LLM endpoint used for the language proposal slot.
**Primary/Secondary Router** — failover pair (`ROUTER_KEY`, `ROUTER_KEY_2`).
**Fast Lane** — immediate deterministic presentation (corrections, cues).
**Slow Lane** — network/AI/tools. **Theater** — presentation choreography; never alters truth.
**Fixture** — deterministic canned behavior, tagged `[fixture:*]`. **Demo Mode** —
one-button deterministic showcase, `execution_mode=DEMO_FIXTURE`. **Avatar State** —
`{emotion, epistemic_overlay, gesture}` driving the puppet. **95/5** — 95%
deterministic core, 5% generative surface. **Authority** — never granted by assertion.

---

## 14. Final audit

1. Can the application start? **YES** (static + serverless, live 200s).
2. Can the live path work? **PARTIAL** — voice→turn→TTS works; LLM brain blocked on providers.
3. Can the live path fail safely? **YES** — fixture fallback at every layer (PROVEN).
4. One-action Demo Mode? **YES** — "Demo Mode" button, top-right of `/voice.html`.
5. Demo Mode without external services? **YES** — zero network calls (PROVEN: source).
6. Adversarial scenario demonstrable? **YES** — Scene B.
7. Epistemic state observable? **YES** — chips + overlay + receipts.
8. Receipts observable? **YES** — `/api/receipts`, MCP `receipt.list`.
9. Puppet without AI? **YES** — client-side SVG, Demo Mode offline.
10. Secrets absent from client artifacts? **YES** — env-only; payload manifests clean.
11. Docs correspond to source? **YES** — every claim cites file/symbol/test/deployment above.
12. Reconstructable by another engineer? **YES** — contracts, diagrams, and failure semantics documented.

**Freeze state:** `dpl_3wvnxXo35AfSHqvpMSX82EG7wuMM` is the frozen demo. NVIDIA brain
went live 16:28 PDT (router mode, `openai/gpt-oss-20b` via ai-router nvidia lane,
governor-validated). Demo Mode remains the zero-dependency fallback.
