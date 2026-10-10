# Brutal-audit application — indy-reads (slice 2/8, 2026-10-10)

## 1. Scope + commit

Repo `~/workspace/indy-reads`, audit commit `b3c3b30`, files changed: `src/governor.rs`, `api/_auth.js`, `PROVENANCE.md`, `tests/governor_adversarial.rs`, `tests/equiv_turn_paths.rs`.

## 2. R-1

SPREE-REDTEAM-A2 claims (`NO AUTH BYPASS FOUND`, `No oracle. PROVEN`, `NOT practically exploitable. PROVEN by measurement`) are instance-only: 22 unauthenticated black-box attempts on 2026-10-09, no authorized-path testing by design, no dedicated re-attack wave since.

SPREE-SUMMARY-A1 fix claims (auth middleware, rt6 hermetic fixtures) are instance-only: verified by the spree's own tests, no re-attack.

WAR-ROOM-LOG `Red team 15/15 green` is instance-only for the 2026-10-07 battery.

## 3. R-2

HONEST BOUNDS doc comments added on `normalize_for_match` (finite map: invisible chars, full-width ASCII, 7 Cyrillic homoglyphs; misses Greek/Cherokee/Latin-extended homoglyphs, zalgo, mirrored text, paraphrase), `claims_authority` (exactly 5 fixed English phrasings; misses paraphrases, other languages, indirect grabs, multi-turn social engineering), `validate_reply` (fixed English word list; misses novel certainty phrasings, other languages, sarcasm, structural certainty).

Structural controls named: authority hardcoded false; authority-bearing fields stripped unconditionally in `AiProposal::sanitize`; canned substitution on mismatch.

## 4. R-3

`oob-heads/indy-reads.json` created. Measured 2026-10-10: `indy-reads.db` receipts table is a multi-instance mirror, not a single chain: 356 rows, 281 distinct `session_ids`, 265 linkage breaks when partitioned by `session_id` (each transport holds its own chain instance: native governor, per-McpState, per-isolate wasm). No single canonical head exists in this store.

Latest row anchor recorded: rowid 356, receipt `246c13fc-a744-4216-8703-b94f120dc86e`, prev_hash `a3186499bbc47dc269d80c5c0fbae4a8252f8474a988b963e16af6d0b9fe6b6a`, 2026-10-08T20:42:32Z.

Live per-instance heads are in-memory in the running Governor only. Same-VM limitation documented.

## 5. R-4

Backlog triage.

- MEDIUM "Bearer `<redacted>` routinely transmitted in URLs" → WONT-FIX: Zack's standing interim `?key=` pattern, documented tradeoff; complexity budget recorded: `?key=` reserved for `<audio>` GETs that cannot set headers; fetch()-to-Authorization-header migration quoted ~1 lane, revisit on redeploy.
- LOW "No rate limiting on auth endpoint" → WONT-FIX informational: Vercel platform abuse controls; 256-bit brute force infeasible.
- LOW "safeEqual length-check early return" → FIXED this pass: `safeEqual` now hashes both sides with sha256 before `crypto.timingSafeEqual`; node-verified (match true, mismatch false, different-length false, unicode true).

## 6. R-5

`claims_authority` phrase list and `validate_reply` word list declared BEST-EFFORT with FIXED complexity budgets, frozen; no more expansion waves; new evasion classes handled structurally.

## 7. R-6

Every detector has a named consumer + gate (verified by grep):

- `claims_authority` → `decide()` BLOCK + response_mode
- `validate_reply` → `api.rs:806` reply substitution
- `AiProposal::sanitize` → `rejected_fields` recorded in receipts
- decision-nonce registry → REJECT on replay/unknown

No detector-without-consumer found.

## 8. R-7

`tests/governor_adversarial.rs` labeled PROPERTY SUITE (security invariants in implementation-free terms; never rewrite expectations without adversarial sign-off); `tests/equiv_turn_paths.rs` labeled CHANGE-DETECTION SUITE (pins exact cross-transport behavior incl. verbatim receipt formulas; behavior changes need adversarial sign-off on the new behavior). Second sweep: `src/governor.rs` inline unit-test module labeled MIXED (flag-assertions are property; exact canned-substitution assertions are change-detection).

## 9. R-8

`PROVENANCE.md` now declares receipts UNSIGNED with verification power stated plainly: hash linkage only, never producer authentication; D1 mirror is a second copy, not an anchor.

Pre-existing honesty kept: Vercel demo build identity UNKNOWN (predates provenance chain), dirty builds not release-shipped by discipline not CI.

## 10. R-9

Schema frozen: `EpistemicStatus` enum (`Proven`/`Disproven`/`Unknown`), `ProvenanceSource` enum (`User`/`System`/`ExternalSource`/`GeneratedHypothesis`), `ClaimRevision` with fixed `USER`/`INVALIDATED` constants. No prose-status fields; no verdict-less entries in these types.

## 11. R-10

`WAR-ROOM-LOG.md` declares "No entry here without evidence" and is maintained per event (live record). `SPREE-SUMMARY-A1.md` and `SPREE-REDTEAM-A2.md` are post-hoc dated records (archival after their date). `DEV-PROCESS-PROOF-2026-10-07.md` is dated archival.

## 12. R-11

UNKNOWN list:

- (a) whether the Vercel demo currently serves the audited build (health carries no commit hash);
- (b) re-attack status of the auth gate since 2026-10-09;
- (c) whether any production LLM reply ever triggered `validate_reply` substitution (no production telemetry);
- (d) exploitability of Bearer-in-URL beyond the documented blast radius.

## 13. R-12

Grep for `probably`/`likely`/`seems` across repo docs found no unsourced footnotes.

## 14. Commits

`b3c3b30` (this pass). Tests measured 2026-10-10: `governor::` lib 32/32 green; `governor_adversarial` 10/10 green. `equiv_turn_paths` not run (spawns real binaries/servers; change was doc-only header label).

## 15. Wont-fix list

The two items from R-4 above.
