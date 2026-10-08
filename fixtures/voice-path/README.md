# INDY-VOICE — voice path behavioral verification (wave-4)

Lane: indy-voice. Date: 2026-10-08. Method: behavioral, not asserted.
Server: local `./target/debug/indy-reads` (commit 15dc1d3, clean worktree),
fixture mode (no provider keys), port 18091. Client guards: the REAL
`web/voice.js` source loaded in Node with stubbed browser APIs, real
`Date.now()` and real timers.

## Files

- `turns.json` — 10 scripted turns: HTTP status, latency, reply, epistemic,
  provider per turn.
- `transcript.json` — captured `/api/transcript/:session` (20 rows: 10 user
  turns + 10 INDY replies). Schema per row: speaker / text / started_at_ms /
  ended_at_ms / intent. Timestamps monotonic. Intent classified per turn
  (NameEnrollment, Respond, Question, Converse).
- `session.json`, `receipts-sample.json`, `parallel.json` — session state,
  5-way parallel turn outcomes, receipt sample.
- `guard-measurements.json` — 11/11 client-guard checks with measured timings.

## Verified

1. **Scripted fixtures through the governor (fixture mode):**
   - `My name is Zack` → enrollment reply "Noted — you're Zack in my book
     now." (703ms). Speaker slot bound BEFORE the AI sees anything.
   - `call me Ponyboy` → rename confirmed (328ms). Transcript displays the
     CURRENT enrolled name on all rows (historical rows show "Ponyboy",
     not "Zack" — display semantic, documented not hidden).
   - Ordinary turns → deterministic `[fixture:indy]` replies via the
     governor's provider path (provider=fixture, epistemic=UNKNOWN).
   - Turn response carries deterministic prosody (`voice`: rate 0.9312,
     pitch 0.9506, pause_ms 550, voice_id V-007-P, emotion Skeptical —
     derived from the governor verdict, same event as the avatar) and
     `clone_status: off` (no GPU on this host — honest fallback, not a
     silent capability claim).
2. **Self-talk spam controls (measured, real clock):**
   - One turn in flight: 5 results in 14ms → exactly 1 `/api/turn` fetch;
     the rest dropped (`turn already in flight` / `brain thinking`).
   - Echo fingerprint: her own reply fed back verbatim → dropped
     (`heard herself`); distinct text → accepted. Case/punctuation-insensitive
     variant also dropped.
   - 2500ms post-speech cooldown: dropped at 2400ms, accepted at 2600ms;
     bisection measured the boundary at **~2507ms** (range [2500,2513]) —
     the design value confirmed behaviorally.
   - Fingerprint armed before audio: echo fired between `speak()` and
     audio-start → dropped (lastSaid is set synchronously in speak()).
   - Empty / whitespace / single-char / interim results → zero turns.
3. **Diarized transcript schema:** every row populates speaker, text,
   started_at_ms, ended_at_ms, intent. No invented speakers (single-mic
   honest attribution: SPEAKER_00 / INDY). Speaker labels are metadata —
   the governor never reads them as authority (pinned by
   `test_speaker_label_is_not_authority`).
4. **Receipt chain:** 11 turns → 44 receipts
   (`governor.select_provider` → `provider.call` → `governor.decide` →
   `turn.state`), prev_hash linkage verified intact.

## Adversarial (deterministic documented error states)

| input | observed | verdict |
|---|---|---|
| empty utterance | HTTP 400 "utterance required, max 2000 chars" (16ms) | deterministic reject, no transcript row |
| whitespace-only | HTTP 400 (7ms) | deterministic reject |
| 2001 chars | HTTP 400 (2ms) | deterministic reject |
| garbled "M M M M M" | HTTP 200, deterministic `[fixture:indy]` reply, recorded as a real turn with receipt | NOT silent success (full governor path + receipt), NOT a phantom (transcript row exists); fixture fallback is the documented deterministic behavior for unintelligible input |
| 5 parallel overlapping turns | 5× HTTP 200, 10 transcript rows, chain intact, no 500s | deterministic; server serializes via session lock, no corruption |

## Not claimed

- No real microphone/audio was exercised (no browser in this environment);
  the client-guard harness drives the real `voice.js` result path with
  stubbed I/O — the guard LOGIC is verified, the browser plumbing is not.
- Vercel demo untouched (stays empty per standing order). No deploys.
