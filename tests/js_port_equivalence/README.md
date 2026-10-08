# js_port_equivalence — NIH-FIND-5 harness

Behavioral equivalence for the hand-written JS governor ports
(`api/turn.js`, `cf-pages/functions/api/turn.js` → assembled
`cf-pages/worker.js`, `api/mcp.js`, `cf-pages/functions/api/mcp.js`)
against the Rust governor. Closes the NIH-FIND-5 gap: these ports were not
covered by the 169/169 Rust equivalence harness.

## Layout

- `intents.json` — 30 scripted turns + 13 adversarial reply-veto cases.
  No hardcoded expectations: the Rust reference is the oracle.
- `rust-ref/` — standalone Cargo crate (path dependency on `indy-reads`,
  own `[workspace]` so it never joins the main build). Calls the REAL
  `indy_reads::governor::{claims_authority, evaluate_utterance, validate_reply}`.
- `run.mjs` — the runner. Loads both turn ports fresh (CJS require-cache
  bust + ESM query-bust), mocks req/res and Request/Response, monkeypatches
  `fetch` to throw (any network attempt fails the run), drives every intent
  through all 5 surfaces, recomputes each port's receipt hash chain
  externally, and prints a per-intent verdict table. Exit 0 = all pass.
- `DIVERGENCES.md` — every divergence found, severity, fix, and designed
  boundaries.

## Run

```sh
cd ~/workspace/indy-reads/tests/js_port_equivalence
node run.mjs
```

Requires: `node` ≥ 18, `cargo` (defaults to `~/.cargo/bin/cargo`).
`rust-ref` builds once (~6 min first time; cached after).

## What is asserted per turn

- error-class vs Rust (empty / whitespace-only / >2000 chars → reject)
- authority REJECT vs Rust `claims_authority`
- epistemic UNKNOWN on success
- twin payload identity (`api/turn.js` ≡ worker.js turn handler, ids scrubbed)
- receipt-chain linkage recomputed externally
  (`sha256(prev‖id‖op‖decision)` — same shape as Rust `commit_receipt`)
- failover classification = fixture, `llm.used === false`, zero fetches
- mcp `governor.evaluate`: intent / decision / epistemic / claim-persisted /
  reply-text vs Rust `evaluate_utterance`, both mcp ports

## What is asserted per reply case

- `validateReply` veto trigger vs Rust `validate_reply(Unknown)` on both ports
- byte-identical veto substitution between ports

## Non-vacuity

The pre-fix port code fails 7 of the differential probes against the Rust
oracle (measured from `git HEAD` sources); the fixed code passes 738/738.

## Regenerating worker.js

`cf-pages/worker.js` is GENERATED. Never edit it directly:

```sh
python3 ~/workspace/indy-reads/cf-pages/assemble-worker.py
```

The harness imports the assembled file, so always reassemble after touching
`cf-pages/functions/api/*.js`.
