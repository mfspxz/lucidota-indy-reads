# Indy_Reads — Crate Atlas Integration Notes

Date: 2026-10-08. Source: `~/workspace/crate-scout/00-CRATE-ATLAS-100.md`
(5 scout lanes × 20 crates, adversarially vetted).

This documents every dependency added, every code change, and why.
Nothing here touches the hackathon submission boundary (this is the
Indy_Reads main-project repo, not the Context Forge submission).

## Dependencies added

| Crate | Ver | Why | Where it lands |
|---|---|---|---|
| thiserror | 2 | Typed errors for lib/binary code; replaces hand-rolled `Display`+`Error` impls | `src/ai.rs` (`GenError`) |
| anyhow | 1.0.104 | Context-chained errors at fallible boundaries | `src/mcp_stdio.rs` stub; future binary edges |
| tracing | 0.1 | Already a dep; now actually used | `governed_turn` span in `src/ai.rs` |
| tracing-subscriber | 0.3 | Already a dep; unchanged | `main.rs` init (pre-existing) |
| clap | 4 (derive) | CLI arg parsing for binaries | `src/bin/indy-verify.rs` |
| async-openai | 0.42.1 | The default LLM client; base-URL override covers the $0 fleet | new `OpenAiCompatBackend` in `src/ai.rs` |
| schemars | 1 | JSON Schema derived from Rust types | new `src/tool_schemas.rs` |
| tiktoken-rs | 0.12.1 | Exact cl100k token budgeting for the 5% LLM slot | new `src/tokens.rs` |
| minijinja | 3 | Prompt template rendering | new `src/prompts.rs` |
| jiter | 0.17 | Typed, tolerant JSON parsing of LLM output | `OpenAiCompatBackend` envelope parse |
| jiff | 0.2 | Timestamps (chrono's successor) | new `src/time_util.rs` |
| rmcp | 3.5.1 | Official Rust MCP SDK | `src/mcp_stdio.rs` **stub only** |
| tempfile | dev | Secure temp dirs in tests | `tool_schemas` round-trip test |
| rstest | dev | Parametrized test cases | `tokens` budget tests |
| insta | dev | Snapshot tests with review workflow | `tokens` count snapshot, `tool_schemas` JSON snapshot |

### Deliberate deviation from the atlas

The atlas says reqwest must use the `rustls-tls` feature, never
default/native TLS. Here `async-openai` is wired with
`default-features = false, features = ["chat-completion", "native-tls"]`
because:

1. This repo already standardized on native-tls
   (`tokio-tungstenite` has used it since before this change) —
   consistency within one binary beats a global rule.
2. The `rustls` feature pulls `aws-lc-sys`, a ~500kLOC C compile that
   **fails on this VM** (`/tmp` is a 512 MB tmpfs; the C build ran it
   dry mid-compile). Native TLS keeps the build green here.
3. The threat model that motivates rustls (supply-chain C code) is
   unchanged, but a build that cannot compile is worse than a
   consistent native-tls posture. Revisit when the build VM has disk.

## Code changes

### 1. `src/ai.rs` — thiserror, tracing, new provider lane

**thiserror for `GenError`.** The hand-rolled `Display` + `Error` impls
(15 lines) became a two-line derive:
```rust
#[derive(Debug, Clone, thiserror::Error)]
#[error("{kind:?}: {msg}")]
pub struct GenError { pub kind: GenErrorKind, pub msg: String }
```
Same struct shape, same `GenError::new` constructor, same rendered
message — every existing call site compiles unchanged, including the
failover tests that match on `GenErrorKind`.

**Tracing for the governor loop.** `interpret_governed` now opens one
`info_span!("governed_turn", request_id, session_id, persona)` per turn,
so the provider trail (`trail: Vec<(String, String)>`) is observable in
structured logs alongside the hash-chained receipts. The existing
`tracing::warn!` on provider failure was already there; the span gives
it a parent.

**`OpenAiCompatBackend` (new, alongside — not instead of — `RouterBackend`).**
Implements the existing `Backend` trait (`name`/`probe`/`generate`), so
the governor's failover loop treats it as one more candidate with zero
control-path changes. How it works:

- Built by `async_openai::Client::with_config(OpenAIConfig::new()
  .with_api_base(base).with_api_key(key))` — the base URL override is
  the whole point: any OpenAI-compatible endpoint (Groq, DeepSeek,
  Ollama, vLLM, local gateway) becomes a lane.
- Enabled only when `OPENAI_BASE_URL` is set (`from_env() -> Option`).
  Key from `OPENAI_API_KEY` (may be empty for keyless local endpoints),
  lane name from `OPENAI_LANE_NAME` (default `"openai-compat"`), model
  from `OPENAI_MODEL` (default `"gpt-4o-mini"`).
- Registered in `AiClient::from_env()` **after** the three
  `RouterBackend` lanes, key-gated the same way: no
  `OPENAI_BASE_URL`, no lane. The ai-router wiring is untouched.
- Per turn: system prompt truncated to 3000 cl100k tokens
  (`tokens::truncate_to_budget`), user message rendered from the
  in-binary minijinja template (`prompts::render_user_turn`), request
  built with `CreateChatCompletionRequestArgs` (400 max tokens, temp
  0.7 — same contract as `RouterBackend`), 30s `tokio::time::timeout`.
- Response: `extract_json` finds the envelope, `jiter::from_str::<AiReply>`
  does the typed parse. Timeout → `GenErrorKind::Timeout`, transport
  failure → `Unavailable`, bad envelope → `Malformed` — the same
  classified kinds the governor's failover already understands.

### 2. `src/tokens.rs` (new lib module)

`tiktoken_rs::cl100k_base()` (embedded BPE ranks, `OnceLock` singleton)
with three functions: `count_tokens`, `fits_budget`,
`truncate_to_budget` (encode → truncate ranks → decode; tail trim, so
callers keep the contract section and trim context). Tests: rstest
parametrized count/budget cases, truncation invariants, and an insta
snapshot pinning one count so a tiktoken upgrade that changes counts is
a deliberate review.

### 3. `src/prompts.rs` (new lib module)

minijinja `Environment` built once from two `const` template strings:
`user_turn` (hint + utterance) and `system_wrapper` (persona + the JSON
envelope contract). **Security invariant, enforced by API shape:**
there is no function here that accepts a template string — templates
are consts, LLM output and user input are only ever *data* passed to
`render`. The test suite asserts `{{ 7*7 }}` in user input is NOT
evaluated (no SSTI by construction).

### 4. `src/tool_schemas.rs` (new lib module)

schemars-derived JSON Schema (draft 2020-12) for the tool-callable
proposal surface. `ProposalTool` mirrors `governor::AiProposal` minus
the four authority-bearing fields (`authority`, `verified`, `execute`,
`claim_status`) — which are deliberately absent from any tool contract.
`proposal_tool_definition()` emits the full
`{name, description, parameters}` tool object for future MCP wiring.
**Anti-drift:** `proposal_fields_match_governor` fails the build if the
schema's property set drifts from `AiProposal`'s serialized fields.
(`AiProposal` itself lives in wasm-shared `governor.rs` and was not
touched — the wasm crate would need the new dep too.)

### 5. `src/mcp_stdio.rs` (new lib module) — STUB

`rmcp::transport::io::stdio()` is constructed to prove the dependency
links, then the function bails with an explicit error: **full MCP
server wiring is a separate task.** No tools registered, no serve loop.
Transport posture per the atlas: stdio only — rmcp's networked
transports (streamable HTTP, OAuth) carry the advisory history and stay
off until a use case forces them. The test asserts the stub fails
loudly, never silently.

### 6. `src/time_util.rs` (new lib module)

`now_rfc3339()` via jiff. Additive only — the receipt hash chain keeps
using `receipts::now_ms()`; chain format, hashing, and verification are
untouched. Used in `main.rs`'s startup log line.

### 7. `src/bin/indy-verify.rs` — clap

Hand-rolled `std::env::args` parsing replaced with
`#[derive(clap::Parser)]`: one optional positional `input` (file, or
stdin when omitted). Behavior identical — same exit codes (0 verified,
1 failed, 2 unreadable input), same JSON verdict output.

### 8. `src/main.rs` — startup log

One line: `tracing::info!("indy-reads starting at {}", now_rfc3339())`
after subscriber init.

## What was NOT done (and why)

- **Full MCP server wiring** — stub only, per the task. The existing
  hand-rolled `src/mcp.rs` stdio JSON-RPC server is untouched; rmcp
  wiring comes later.
- **`wasm/governor-wasm`** — untouched. It `#[path]`-includes only
  `governor.rs`/`domain.rs`/`receipts.rs`; none were modified, so the
  wasm build is unaffected. (New lib modules don't enter its build.)
- **No `RouterBackend` changes** — the ai-router lane is the live demo
  path; the async-openai lane is additive and inert without
  `OPENAI_BASE_URL`.
- **No receipt/chain format changes** — jiff is display-only.

## Build lessons (for the next integration wave)

1. **`tracing::info_span!(...).entered()` is `!Send`.** Holding the guard
   across `.await` makes the future `!Send`, which broke axum's `Handler`
   bound on `/api/turn` (E0277). Fix: `#[tracing::instrument]` on
   `interpret_governed`, which is Send-safe by design.
2. **`rmcp` needs `features = ["transport-io"]`** — the stdio transport
   (`rmcp::transport::io::stdio`) is feature-gated, not in defaults.
3. **`jiter` needs `features = ["serde"]`** for `jiter::serde::from_str`
   (typed parse). Without it, only `JsonValue` is available.
4. **`insta` needs `features = ["json"]`** for `assert_json_snapshot!`.
5. **async-openai types live under `types::chat::`**, not `types::`
   directly (`CreateChatCompletionRequestArgs`,
   `ChatCompletionRequestMessage`, …).
6. **`/tmp` is a 512 MB tmpfs.** The default `rustls` feature of
   async-openai pulls `aws-lc-sys` (C compile) which ran it dry
   mid-build — one more reason for the native-tls deviation above.
   (`/tmp/jack`, leftover LiteBox/dfirswarm clones, was removed to
   free 215 MB.)

## Verification

- `cargo check --all-targets`: green.
- `cargo test`: **203 passed, 0 failed** (2026-10-08) — 168 lib (incl. gmail
  connector, tokens, prompts, tool_schemas, mcp_stdio, time_util tests),
  23 + 12 integration (equiv_turn_paths, governor_adversarial), bins.
- Boot smoke: server starts, `/health` returns `ok`, startup log shows
  the jiff RFC 3339 timestamp. `indy-verify --help` shows clap output.
- Demo behavior: unchanged when `OPENAI_BASE_URL` is unset (the new
  lane doesn't register; `probe()` never fires).

## Env vars added

| Var | Default | Effect |
|---|---|---|
| `OPENAI_BASE_URL` | unset → lane off | Enables the async-openai lane |
| `OPENAI_API_KEY` | `""` | Bearer key (empty OK for keyless endpoints) |
| `OPENAI_MODEL` | `gpt-4o-mini` | Model string sent in the request |
| `OPENAI_LANE_NAME` | `openai-compat` | Lane name in governor failover order |
