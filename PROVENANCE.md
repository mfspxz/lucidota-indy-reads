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
| source | this repo, commit `df523f91f2a0de92312775a3e64aeba18259aed2` (filled at init) | PROVEN after git init |
| build | `cargo build --release` on the VM | PROVEN (binary carries hash) |
| deploy (demo) | https://indy-reads-ui-maroonedpilot-8906.vercel.app/voice.html | PROVEN (live, hash-verified Oct 7) |
| runtime | `GET /health` → compare `commit` | verify per deploy |

## Verifying a deployment

```bash
# 1. source identity
git -C ~/workspace/indy-reads rev-parse HEAD
# 2. runtime identity (replace host)
curl -s http://localhost:8080/health | python3 -c "import json,sys; print(json.load(sys.stdin)['commit'])"
# 3. they must match; if not, the runtime is a different artifact
```

## Known gaps (honest)

- The Vercel demo deployment's build provenance predates this chain; its
  `/api/health` does not yet serve the commit hash. Until redeployed from
  a provenance-carrying build, treat its build identity as UNKNOWN.
- `dirty_worktree=true` builds are reproducible in content but not in
  identity — commit before release builds.
