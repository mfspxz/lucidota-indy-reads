# DIVERGENCES.md — NIH-FIND-5 JS governor-port equivalence

Harness: `tests/js_port_equivalence/` — 30 scripted turns × 5 surfaces
(Rust `evaluate_utterance` / `api/turn.js` / `cf-pages/worker.js` turn handler /
`api/mcp.js` / `cf-pages/worker.js` mcp handler) + 13 adversarial reply veto
cases + pure-function differentials (`claims_authority`, `validate_reply`,
`normalize`) + external receipt-chain recomputation + failover/no-network
classification. **738/738 checks green, 0 failed** (post-fix).

Every divergence below was MEASURED against the Rust oracle
(`tests/js_port_equivalence/rust-ref`, which calls the real
`indy_reads::governor` functions — never a reimplementation), fixed in the
port (ports follow the Rust governor, never the reverse), and re-verified.
Falsification entries: `FIND-5-DV-{1..7}` in `~/workspace/compute/falsification.jsonl`.

Severity scale: HIGH = an attacker could evade a governor guard on the JS
surface while the Rust surface blocks it. MEDIUM = verdict/contract mismatch
without a direct guard bypass.

## DV-1 — claims_authority phrase set (HIGH, FIXED)

- Rust (`src/governor.rs:181`): `i am the governor` | `i'm the administrator` |
  `ignore your governor` | `you are my puppet` | `override the governor`.
- Old JS: `i am the governor` | `i am indy` | `you are my puppet` |
  `override the governor` (space-padded substring).
- False negatives: `ignore your governor`, `i'm the administrator` passed the
  JS ports unchallenged (Rust: DENY_AUTHORITY). False positive: `i am indy`
  was REJECTED by JS (Rust: not an authority claim).
- Fix: exact Rust phrase set, bare `contains` on normalized text, in
  `api/turn.js`, `cf-pages/functions/api/turn.js` (+ regenerated worker.js).

## DV-2 — normalize_for_match fidelity (HIGH, FIXED)

Old JS `normalize` vs Rust `normalize_for_match`:
- Missing strip chars: U+2060 (word joiner), U+2028/U+2029 — match-evaders.
- Missing full-width ASCII map (U+FF01–FF5E): `Ｉ ＡＭ ＴＨＥ ＧＯＶＥＲＮＯＲ`
  sailed through JS, caught by Rust.
- Extra map: Cyrillic у→y (Rust does not map it).
- Fix: faithful port (`normalizeForMatch`) in both turn files (+ worker.js via
  assembler). Note the strip-then-collapse order is load-bearing: stripping
  U+2060 merges words (`i\u2060am` → `iam`), exactly as Rust does.

## DV-3 — validate_reply veto weakness (HIGH, FIXED)

- Old JS had no `deleet`: `pr0ven`, `d3f1nitely`, `c0nfirmed` evaded the JS
  guard; Rust catches them.
- Old JS word list missed `caseisclosed`, `mysteryissolved`, `indisputably`,
  `irrefutably`, `undeniably`, `conclusive`.
- Old JS `neg` list cancelled the whole check if ANY negated word appeared
  anywhere, so `debunked`/`disproven` (which Rust VETOES under UNKNOWN via
  `claims_disproven`) passed JS.
- Old substitution used `--`; Rust uses `—` (U+2014).
- Fix: faithful port of `validate_reply` incl. `deleet`, full word lists,
  per-word negation scoping, and the `claimsDisproven` arm.

## DV-4 — input contract: silent truncation (MEDIUM, FIXED)

- Old JS: `String(body.utterance || '').slice(0, 2000)` — over-long input
  silently truncated. Rust `evaluate_utterance`: `Err("utterance required,
  max 2000 chars")`.
- Fix: trim, then 400 + the exact Rust error on empty or >2000 chars, in both
  turn handlers and both `governor.evaluate` implementations.

## DV-5 — mcp governor.evaluate reimplemented the classifier (HIGH, FIXED)

`api/mcp.js` and `cf-pages/functions/api/mcp.js` (and hence the worker.js mcp
section) did NOT port `evaluate_utterance` — they hand-rolled a different
decision table:
- Authority regex missed `i'm the administrator`, `permission`/`admin`/
  `pretend` signals; over-triggered on `i am indy`.
- Question arm was start-anchored and missed Rust's `contains('?')`,
  `tell me`, `is the`, `are there`.
- Hypothesis arm was start-anchored (`maybe|perhaps|...`); Rust matches
  `could be`/`what if`/`maybe`/`hypothes` anywhere — `what if X?` classified
  Question by the mcp port, HypothesisAssertion by Rust.
- No ClaimAssertion gate: EVERYTHING else became a claim (junk claim
  persistence); Rust only persists on exists/is real/i saw/confirmed/definitely.
- No trim; whitespace-only input accepted (Rust: Err).
- Fix: `classifyIntentGov` (faithful port of `classify_intent`) +
  `evaluate_utterance` decision table, with local `normalizeForMatch` /
  `claimsAuthority` in both mcp files.

## DV-6 — worker.js is generated; direct edits are ephemeral (PROCESS, FIXED)

`cf-pages/worker.js` is assembled from `cf-pages/functions/api/*.js` by
`cf-pages/assemble-worker.py` (verified: HEAD assembly is byte-identical).
The initial fix pass edited `worker.js` directly — correct content, wrong
layer. Fix: all changes moved to `cf-pages/functions/api/turn.js` and
`cf-pages/functions/api/mcp.js`; `worker.js` regenerated via the assembler
and re-verified section-identical.

## DV-7 — twin drift between api/ and cf-pages/ sources (MEDIUM, FIXED)

`api/turn.js` (Vercel CJS) and `cf-pages/functions/api/turn.js` (ESM) had
identical governor functions at HEAD by luck, not by construction; same for
the mcp pair. Any future one-sided edit re-opens the drift. The harness now
pins them: every check runs against BOTH twins, so drift fails the suite.
A shared-source refactor (single governor-primtives module per platform) is
possible but would change the deploy assembly; left as a follow-up.

## Designed boundaries (NOT divergences)

- **Intent taxonomies differ by layer**: the turn handlers' `classifyIntent`
  (attack/question/hypothesis/chatter) is a UI latency preface; the Rust
  `classify_intent` (Question/HypothesisAssertion/ClaimAssertion/
  AuthorityRequest/Converse) drives verdicts. Twin parity asserted; mapping
  to Rust intentionally not forced.
- **Branch/persona/voice/theater/scaffold layers** are demo-backend features
  with no Rust counterpart — twin parity only.
- **Genesis label**: JS chains start `GENESIS`, Rust `genesis` — cosmetic;
  chain SHAPE (`sha256(prev‖id‖op‖decision)`) is identical and externally
  re-verified per port.
- **Failover**: Rust has provider failover (`select_provider`); the JS ports'
  analog is the router-key chain → fixture. With no key configured both ports
  classify `fixture` with zero network attempts (asserted, fetch monkeypatched
  to throw). Live provider failover is untestable at $0/no-network — honest gap.
- **`api/mcp.js` claim.create etc.** were already ports of `src/mcp.rs` tool
  logic; only `governor.evaluate` diverged and it is now fixed.
