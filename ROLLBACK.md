# Rollback procedure — Indy_Reads (E5)

Local target only. No remote deploys are authorized from this repo
(Vercel redeploy is publish-gated: Zackary's explicit go-ahead required).

## Layout (`deploy-local/`)

```
deploy-local/
  releases/<name>/        immutable release bundles (from ./release)
  current -> releases/<name>   symlink = what is live
  manifest                name of the release `current` should point at
```

A release is deployed by copying a `./release` bundle under `releases/`,
verifying `CHECKSUMS.sha256` on ingest, then flipping the `current`
symlink. The previous bundle is never deleted by a deploy — that is what
makes rollback possible.

## The loop

1. **deploy** — copy bundle, verify checksums, flip `current`, restart.
2. **detect** — `GET /health` must return 200 within the startup window.
   Any failure here is a failed deploy, not a slow one: investigate, don't retry blindly.
3. **identify** — compare `/health` `commit` + `contract_version` against
   `releases/<name>/BUILD.json` and the `manifest`. Three outcomes:
   - matches → this IS the artifact you think it is; the failure is behavioral, not identity.
   - mismatch → wrong artifact deployed; treat as identity incident.
   - unreachable → failed/partial deploy (binary missing, port conflict, crash on start).
4. **rollback** — point `current` back at the previous `releases/<name>`,
   restore the manifest, restart.
5. **verify** — `/health` 200 AND `commit` equals the pre-deploy commit AND
   a live `/api/turn` completes AND the receipt chain verifies via
   `indy-verify`. Rollback is not done until all four hold.

## Drill

```bash
./release                    # build a bundle (runs ./verify first)
./rollback-test release/indy-reads-<version>-<sha>
```

The drill deploys the good release, simulates a partial deploy (bundle
with the binary missing — the classic failed-deploy shape), and runs the
full detect → identify → rollback → verify loop. It exits non-zero if any
stage fails. Last run: see ledger.

## Rules

- Never delete the currently-live release bundle before its replacement
  passes verify.
- Keep at least one known-good release in `releases/` at all times.
- A rollback that skips the verify step is not a rollback, it's a hope.
- If identity (`commit`) ever mismatches after rollback, stop: the box is
  in an unknown state and must be re-imaged from a verified bundle.
