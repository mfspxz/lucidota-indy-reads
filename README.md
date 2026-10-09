# Indy_Reads

**Voice research agent where the AI generates hypotheses and language, but a
deterministic Rust governor controls authority, evidence, epistemic state, and
every consequential action — with a live receipt trail proving what happened.**

Thesis: **AI ≠ authority.** The LLM is a 5% proposal engine. The 95%
deterministic core decides what the system is entitled to claim.

## Run it

```bash
# Voice UI + HTTP API (typed fallback + browser mic/TTS)
PORT=8081 cargo run --bin indy-reads
# -> http://localhost:8081

# Live MCP server (JSON-RPC 2.0 over stdio, newline-delimited)
cargo run --bin indy-mcp
```

With `ROUTER_KEY` set (ai-router, OpenAI-compatible), the AI slot uses a live
model. Without it, a deterministic fixture fills the slot — the governor
behaves identically either way. Swap implementation, never architecture.

## Architecture: organelles, three transports

```
src/lib.rs  ->  indy_reads crate (the reusable substrate)
├── domain     claim / evidence / epistemic state / evidence POLICY
├── governor   authority, intent, sanitization, decisions (LLM = untrusted input)
└── receipts   hash-chained audit trail (genesis-anchored)

transports (thin, own no truth)
├── src/main.rs + api.rs + web/   voice UI + HTTP + WebSocket
└── src/bin/indy-mcp.rs           MCP server: 12 tools, 2 resource families
```

Voice, HTTP, and MCP all call the **same** `classify_intent`,
`assess_claim`, `EvidencePolicy`, and `ReceiptBuilder`. Different transports
never produce different truth.

### MCP tools

`claim.create/get/evaluate` · `evidence.add/get/list/adjudicate` ·
`epistemic.get` · `receipt.get/list` · `governor.evaluate` · `system.health`

Caller-asserted `epistemic_state`, `authority`, `verified`, `execute`,
`claim_status`, `actor`, `role`, `permission` are **stripped from every tool
call and reported**. There is no tool that sets claim state —
`claim.evaluate` *derives* it from evidence under the policy.

## Epistemic semantics (the law)

- State belongs to a **claim**, never a conversation/model/source.
- `PROVEN` = sufficient verified evidence under `research.standard.v1`
  (threshold 2, configurable — not the programmer's hidden opinion).
- `DISPROVEN` = sufficient contradictory evidence. Absence of evidence is
  **never** disproof → `UNKNOWN`.
- `UNKNOWN` is a valid deterministic result, not a failure.
- User assertions are statements, not evidence. Verified source ≠ proven claim.
- Hypotheses stay `HYPOTHESIS` until evaluated. No `probably_proven` escape hatch.
- LLM-proposed state is untrusted input; the governor re-derives and receipts
  the transition (`previous -> new`, evidence IDs, policy ID).

## Tests

```bash
cargo test   # 203 tests: 168 lib + 35 integration (equiv_turn_paths, governor_adversarial, bins) — all green as of 2026-10-08
```

Adversarial coverage: authority impersonation, forged epistemic state via
voice/HTTP/MCP, fake verification, inference-laundering, malformed/oversized/
unicode input, unknown tools, concurrent load. Bypasses found: **0**.

## CI (remote readiness)

`.github/workflows/ci.yml` mirrors the proven local gate, in order:
`cargo fmt --check` → `cargo clippy --all-targets -D warnings` →
`cargo test --all` → secret-scrub (vendored at `tools/secret-scrub.py`,
changed-files semantics, fail-closed) → `./verify` behavioral smoke
(localhost fixture mode, no keys, no network). The workflow promises nothing
the local gate cannot deliver — every step was executed on the dev VM with
the same pass/fail outcome as `./test` / `./verify`.

**Parked activation:** the workflow is ready but not active. The single
activation step is pushing this repo to GitHub, which is blocked until
Zackary's Google passkey lands (GitHub OAuth via Google is parked; `gh` is
not logged in). Nothing else is needed — no secrets, no custom runners, no
external services. Push the repo and CI runs on the first commit.

## Embeddings

`embeddings` organelle: `EmbeddingProvider` trait (embed/batch/model_id/state).
`MockEmbeddingProvider` (deterministic, labeled FIXTURE) for offline work;
`TernlightProvider` (contract-complete, reports UNAVAILABLE until the WASM is
vendored — never silently substitutes). Verified live 2026-10-07:
@ternlight/base, 384-dim L2-normalized, related 0.82 / unrelated 0.03.
Law: similarity -> candidate edge -> verification -> graph. Never similarity -> PROVEN.

## Cloud

`cloudflare/`: wrangler.toml (D1 + Vectorize + R2 bindings) and d1-schema.sql
(mirrors local SQLite + graph tables). D1 provisioned 2026-10-08 (was:
"unprovisioned / no Indy_Reads D1 exists yet" as of 2026-10-07). Wiring of
worker.js/deploy.py to D1: in progress, see rebuild repair log 2026-10-08.
Cloudflare first, Vercel second (UI/emergency demo only).

## Machine notes (2026-10-07)

Dev VM: 2 CPU, 7.7 GB RAM, **no GPU** — the 39/min NVIDIA rule is moot here;
the deterministic governor is the demo and needs no accelerator.

## Chat persistence + diarization

- SQLite (`indy-reads.db`, `INDY_DB` to override): `speakers`, `sessions`,
  `turns`, `claims`, `evidence`, `receipts`. Every speaker turn is
  timestamped (ms) and attributed. `GET /api/transcript/:session_id` returns
  the diarized transcript.
- `diarize` organelle: `Diarizer` trait + deterministic `SingleMicDiarizer`.
  Labels are transcript metadata, never authority — the governor never reads
  a speaker label as a credential (covered by test).
- Personas: `personas/` — indy, lucidota, archivist, wreck, captain. Each file
  carries its own `## System prompt`, loaded at runtime (`PERSONAS_DIR`,
  `persona` field on `/api/turn`). Costumes change; the governor doesn't.
  Fixture mode speaks in each persona's voice with identical epistemic content.
