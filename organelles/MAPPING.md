# indy-reads — organelle mapping decisions

12 GB voice research agent (Alebex hackathon build: voice UI + HTTP + MCP
transports over a deterministic core, governor-owned truth). Migration
reviewed 2026-10-10/11 under the corrected doctrine (decompose ACTUAL
capabilities; map-or-design per unit, justified). **Capability units only —
no data directories touched** (kokoro-lane/, wasm/, tests/, fixtures/,
personas/ all excluded; `load_persona` reads persona files by name at call
time but ships no data).

## Capability units found (from src/ module layout + actual code)

| Unit | Source | Codon | Ops | Decision |
|---|---|---|---|---|
| Honest diarization + prosody | `diarize.rs` (`SingleMicDiarizer`: `diarize_turn`, `diarize_pcm` — one speaker, never invents changes) + `voice.rs` (`derive_voice`: pure epistemic→prosody arithmetic; `voice_id_for_persona`) | `iv` | `diarize_turn`, `diarize_pcm`, `derive_voice`, `voice_id` | **Bespoke** (`iv`). Justification: the honesty constraint (no invented speaker changes without a model) and the epistemic prosody derivation are domain logic. No ref organelle diarizes or derives voice prosody. |
| Gmail connector | `gmail.rs` (`GmailConnector::search`/`read`, fail-closed `is_enabled`; runner-injected) | `ig` | `status`, `search`, `read` | **Bespoke** (`ig`). Justification: Gmail-API-specific connector with fail-closed disabled semantics over a CLI runner. `ref-http-fetch` is generic HTTP; this is account-bound mail search/read with a connected/disabled state machine. |
| MCP tool surface | `mcp.rs` (`tool_descriptors`, `strip_forbidden`, `call_tool` over `McpState`: claims/evidence/receipts) | `im` | `tool_descriptors`, `strip_forbidden` (ReadOnly), `call_tool` (Write, `im.write`) | **Bespoke** (`im`). Justification: Indy's governed tool-dispatch surface (claim.create/get, evidence attach, receipted). Not a generic MCP adapter — it's the crate's own tool namespace with forbidden-arg stripping. |
| AI router (governed 5%) | `ai.rs` (`AiClient::from_env` — key-gated lanes, `interpret_governed`: governor-selected provider, classified failover, deterministic fixture mode) | `ia` | `load_persona`, `interpret` | **Bespoke** (`ia`). Justification: NOT `ref-model-call` (a stub with one `chat` op). This is the governor-controlled generation path: provider selection, per-attempt classification, receipted failover, fixture determinism. The governor path is the capability. `interpret` runs on a dedicated tokio runtime via block_on. |
| Receipt construction | `receipts.rs` (`ReceiptBuilder` fluent surface, `hash_str`, `new_id`; hash-chained via caller-threaded `prev_hash`) | `ie` | `new_id`, `hash_str`, `build` | **Bespoke** (`ie`). Justification: NOT `ref-hash-seal` (seals one blob). This builds *chained* claim receipts with epistemic status, model, revision — the receipt-chain domain object. The chain is threaded by the caller; the organelle never invents links. |

## Why five organelles, not one

The seams are the crate's own module boundaries, and they have different
trust shapes: `iv`/`ie` are pure and Proven; `ig` is fail-closed over an
external CLI; `im` owns mutable claim/evidence state; `ia` is the only
async/network generative surface. Collapsing them would merge pure functions
with stateful dispatch and provider calls — different mutation classes,
different failure modes, different network declarations.

## Structural change made

`src/ai.rs` was binary-only (`mod ai;` in `main.rs`). Moved to the lib
(`pub mod ai;` in `lib.rs`; `indy_reads::` paths → `crate::`; `api.rs`
now uses `indy_reads::ai::AiClient`). Additive, behavior-preserving —
the binary's control path is unchanged.

## Deliberate exclusions

- **Transports** (`api.rs` HTTP, `mcp_stdio.rs`, voice UI, `theater.rs`):
  thin transports over the core, not capabilities. Excluded.
- **`store.rs`** (SQLite): the MCP state's persistence detail; the
  capability surface is `im`, not the DB.
- **`governor.rs` standalone**: the governor's authority decisions are
  exercised *through* `ia interpret` (the real control path) and `im`
  dispatch. Exposing raw adjudication would bypass the invocation
  surfaces the binary actually uses.
- **`embeddings.rs`, `prompts.rs`, `time_util.rs`, `tokens.rs`**:
  supporting libraries, not independently callable surfaces.
- **Data dirs** (`kokoro-lane/`, `wasm/`, `tests/`, `fixtures/`,
  `personas/`): not touched, per task constraint.
- **`alebex.rs`**: hackathon-specific shim, not a capability unit.

## Reference-organelle check (all 12 considered)

- `ref-model-call`: stub with one `chat` op; `ia` is the governed
  provider-selection + classified-failover + fixture path. Genuinely
  different → bespoke.
- `ref-hash-seal`: seals a blob; `ie` builds chained claim receipts.
  Genuinely different → bespoke.
- `ref-http-fetch`: generic fetch; `ig` is a fail-closed Gmail connector
  with account state. Different → bespoke.
- Others (`ref-kv-store`, `ref-json-validate`, `ref-text-transform`,
  `ref-time-oracle`, `ref-geo-point`, `ref-ledger-append`, `ref-notify`,
  `ref-scrape-extract`, `ref-file-cas`): no overlap.
- Verdict: five bespoke organelles. Nothing reinvented that already existed.

## Codon collision check

New: `iv`, `ig`, `im`, `ia`, `ie`. Checked against the reserved set
(refs: fc,gp,hs,hf,jv,kv,la,mc,nt,se,tt,to; migrated: ad,at,ay,bc,bd,be,
bg,bi,bk,bm,br,bs,bv,ce,cm,co,dg,ex,fd,fs,ip,ir,lc,ld,lg,lt,lx,or,pa,pc,
pj,pv,rt,sl,sm,sy,tj,tm,up,va,vf,vm,vp,yp,no,ca,wg,wp,ne,na,ai,lv,lq,me,
af,ge,gj,gi,ks,kd,ke,ak,as,mx,kx,kn,fp,fx,fv,fw). No collisions.

## Execution facts

- `iv`, `ie`: ReadOnly, Proven (pure). No fs, no network, no secrets.
- `ig`: ReadOnly; network egress to the gmail CLI subprocess; fail-closed
  when unconnected. No secrets in manifests (account name only).
- `im`: `tool_descriptors`/`strip_forbidden` ReadOnly; `call_tool` Write
  (`im.write`), owns `Mutex<McpState>`.
- `ia`: ReadOnly; network egress to provider lanes when keys are set;
  deterministic fixture mode otherwise. Owns a tokio runtime.
- Receipts: content-addressed sha256, chained; `verify_chain` green in tests.
- Note: organelle-host P0/P1 hardening (2026-10-10 23:39 / 2026-10-11 00:14
  UTC) changed `host.call` → `CallOk { output, receipt_id }` and added
  `caller_trace` to `HostRef::call_organelle`; tests use `.output`.

## Tests

- `organelles::tests`: 8 tests green (register→call→verify_chain across
  all five, gmail status-shape honesty, fixture interpret, receipt build,
  manifest validation, static-JSON match).
- Full lib suite: run at commit time; result recorded below.
