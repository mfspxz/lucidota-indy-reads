# Indy_Reads provenance chain (C1)

Every identity a hostile reviewer needs, with no tribal knowledge.

## The chain

```
SOURCE        ~/workspace/indy-reads  (git repo; this directory)
  → COMMIT    git rev-parse HEAD  (immutable source identity)
  → BUILD     cargo build --release  (rustc version + timestamp baked in)
  → DEPLOY    artifact copied to its runtime host
  → RUNTIME   /health reports commit/branch/dirty/version — compare it
```

Source truth != runtime truth. The `/health` endpoint carries the build's
own commit hash so you never have to trust prose: if the running commit
doesn't match the source commit you audited, you are not looking at the
artifact you think you are.

## Build identity (baked in by build.rs)

- `version` — crate version from Cargo.toml
- `commit` — `git rev-parse HEAD` at build time, or `no-vcs`
- `branch` — branch at build time
- `dirty_worktree` — true if the tree had uncommitted changes at build
- `rustc` — compiler version
- `built_at` — UTC timestamp

`provenance::vcs_clean()` is true only for a real commit with a clean tree.
A `no-vcs` or dirty build is UNKNOWN provenance, never "proven".

## Current deployment identities

| layer | identity | status |
|---|---|---|
| source | this repo, tag `provenance-baseline` → commit `d71a5b7944e4ae27db320f6fbae29328a096a20f` | PROVEN (immutable tag) |
| build | `cargo build --release` on the VM | PROVEN (binary carries hash) |
| deploy (demo) | https://indy-reads-ui-maroonedpilot-8906.vercel.app/voice.html | LIVE (responding 2026-10-08 ~13:05 PDT: page title "Indy — talk to her"; `/api/health` → `{"service":"indy-reads","status":"ok","mode":"fixture","router_key":true}`) — build identity UNKNOWN (health carries no commit hash; predates provenance chain) |
| runtime | `GET /health` → compare `commit` | verify per deploy |

## Test evidence (measured, 2026-10-08)

- Baseline `cargo test` 12:56 PDT: **149 green, 0 failed** (139 lib + 10 bin).
  The work-order's "143 tests passing" is DENIED as a count — measured 149.
  (The 143 figure was asserted, never measured; it is superseded.)
- After C2/C4/C5 wiring (governor-controlled provider selection, classified
  failover, receipt-chain reconstruction): **166 green, 0 failed**
  (144 lib + 22 bin), commit `<HEAD>`.
- Live smoke (local binary, fixture mode): `/health` commit ==
  `git rev-parse HEAD`, dirty=false; one turn → `provider: fixture`,
  `provider_trail: ["fixture:ok"]`; `/api/receipts` chain
  `governor.select_provider → provider.call → governor.decide → turn.state`
  hash-verified VALID (independent sha256 recomputation).

## Verifying a deployment

```bash
# 1. source identity
git -C ~/workspace/indy-reads rev-parse HEAD
# 2. runtime identity (replace host)
curl -s http://localhost:8080/health | python3 -c "import json,sys; print(json.load(sys.stdin)['commit'])"
# 3. they must match; if not, the runtime is a different artifact
```

## Known gaps (honest)

- **Receipts are UNSIGNED (R-8, 2026-10-10).** The governor and MCP chains
  bind records by hash linkage only; no key, no signature, no cosign/Rekor.
  Anyone with the bytes can mint a chain that verifies. Verification power =
  tamper-evidence against accidental corruption for a verifier holding an
  independent anchor — never authentication of the producer. The D1 mirror
  is a second copy, not an anchor (same operator controls both).
- The Vercel demo deployment's build provenance predates this chain; its
  `/api/health` does not yet serve the commit hash. Until redeployed from
  a provenance-carrying build, treat its build identity as UNKNOWN.
  (Re-probed 2026-10-08: live and responding, but still no commit hash —
  deployment = DEPLOYED/responding, source identity = UNKNOWN.)
- `dirty_worktree=true` builds are reproducible in content but not in
  identity — commit before release builds.
- Release discipline is not CI-enforced (no gate prevents shipping a
  no-vcs or dirty build). The provenance exists; the enforcement doesn't.
- CLOSED 2026-10-08: `select_provider` is now wired into the live turn path
  (`AiClient::interpret_governed`, called from the turn handler). Provider
  selection, failover, and refusal are governor decisions with receipts,
  not app convention. The old `interpret()` (try-router-then-fixture) is
  deleted.
