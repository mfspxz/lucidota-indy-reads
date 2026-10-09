# Receipt Chains — the two formulas (2026-10-08)

Indy_Reads has TWO hash-chained receipt formulas. This is deliberate and
pinned by `tests/equiv_turn_paths.rs` (C3 receipt-semantics equivalence) —
do not "unify" them without updating that contract.

## 1. Governor chain (canonical)

- **Where:** `src/governor.rs` `commit_receipt` — used by the native server,
  the wasm governor (edge `worker.js` turn/MCP paths), and the D1 mirror.
- **Genesis:** `last = "genesis"` (lowercase).
- **Link rule:** `receipt.prev_hash = last`, then
  `last = sha256(prev_hash + receipt_id + operation + decision)`.
- **Verify:** `src/ai.rs` `code_snippet` ("receipt" branch) emits a Python
  verifier implementing exactly this. The D1 `receipts` table stores
  `prev_hash` verbatim, so D1 rows verify with the same code.

## 2. MCP chain (variant)

- **Where:** `src/mcp.rs` `McpState::commit` — the stdio MCP server and the
  REST adapter (`POST /api/mcp/:tool`) when they run on `McpState`.
- **Genesis:** `prev = "GENESIS"` (uppercase), then `prev_hash = sha256(prev)`.
- **Link rule:** `receipt.prev_hash = sha256(previous receipt_id)`.
  No running chain hash is tracked — each link commits to the previous
  receipt's ID only.
- **Why it differs:** historical. The C3 test asserts both formulas side by
  side so external verifiers can audit either transport without trusting
  the server.

## Scope, not just formula

Each transport holds its OWN chain instance (native governor chain,
per-`McpState` chain, per-isolate wasm chain on the edge). The formulas
above are per-chain. Cross-isolate durability comes from the D1 mirror
(`cf-pages/worker.js` `mirrorReceiptsToD1`, `ctx.waitUntil`, best-effort):
every turn/MCP receipt the edge governor emits is INSERT OR IGNORE'd into
D1's `receipts` table with its native formula intact.

## Verifying

- Governor/wasm/edge/D1: genesis `"genesis"`, link as in §1.
- MCP (stdio or REST adapter): genesis `sha256("GENESIS")`, link as in §2.
- `tests/equiv_turn_paths.rs` lines ~415-446 assert both, verbatim.
