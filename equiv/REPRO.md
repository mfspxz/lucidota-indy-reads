# INDY-REPRO — binary reproducibility measurement (2026-10-08)

Pinned commit: `15dc1d3` ("fix: release script verify echo")
Toolchain: `rustc 1.99.0 (b940084d7 2026-09-28)` — the ONLY toolchain installed
(`rustup toolchain list` → `stable-x86_64-unknown-linux-gnu` only).
Profile: `cargo build --release`. Worktree path identical across builds
(same absolute path — path-based nondeterminism excluded by construction).

## Measured results

| binary      | build 1 (clean, 16m45s) | build 3 (rebuild) | identical? |
|-------------|-------------------------|-------------------|------------|
| indy-reads  | `abc3e510…f4e2db7b8`    | `953266b9…903e7cab29` | NO |
| indy-mcp    | `d1eb930a…47006ddc0ce89` | `c2087909…8f3cf6404b3c525b` | NO |
| indy-verify | `dc25c8e5…6ff3ff06fd`    | `dc25c8e5…6ff3ff06fd` | **YES** |

Full hashes in `equiv/repro-builds/b3/CHECKSUMS.b3` (build-3 binaries preserved
there; build-1 hashes recorded from the build log — the build-1 copies were
lost when the scratch tmpfs filled, see environmental notes).

## Cause of the difference (measured)

`build.rs` bakes `INDY_BUILD_TS` (UTC timestamp from `date -u`) into the binary
via `cargo:rustc-env`, consumed by `src/lib.rs:31` in the `/health` provenance
payload (`built_at`). `strings` on the build-3 binary finds the literal
`2026-10-08T21:38:37Z`. This is INTENTIONAL (C1 source/build identity —
runtime truth vs source truth on `/health`), not an accident.

`indy-verify` does not reference `INDY_BUILD_TS` and is **bit-identical across
two builds ~40 minutes apart** — the compiler/linker pipeline is deterministic
for timestamp-free code on this toolchain.

## Verdict: DETERMINED-BY-DESIGN (DERIVED, strong evidence)

Binaries are reproducible **modulo the intentional UTC build timestamp**.
`indy-reads` and `indy-mcp` differ between builds only because the timestamp
differs (same commit, branch, dirty=false, rustc, absolute path). The
byte-level diff confining the difference to the timestamp region was NOT
completed (blocked — see below), so "timestamp is the SOLE difference" is
DERIVED from: (a) build.rs design + measured timestamp string in the binary,
(b) indy-verify bit-identical across builds, (c) all other baked fields
(commit/branch/rustc/path) identical. Residual unknown: an additional hidden
nondeterminism source alongside the timestamp cannot be fully excluded
without the byte-diff.

To get bit-for-bit reproducibility: make `build.rs` honor `SOURCE_DATE_EPOCH`
(or drop the timestamp from the binary and serve build time from the receipt
chain instead). That is a design decision for Zack — the timestamp exists
deliberately for `/health` provenance.

Cross-toolchain comparison: IMPOSSIBLE on this VM (single toolchain installed).

## Environmental notes (real findings)

1. `/tmp` is a 512 MB tmpfs. A full release `target/` (~450 MB) fills it; the
   failures present as confusing downstream errors (rustc "failed to parse
   process output" exit 101, `cc` failing on sqlite3.c). **Build worktrees
   belong on persistent disk, never /tmp.**
2. Under VM load, rustc 1.99.0 shows intermittent IPC failures and the LLVM
   linker (`lld`) crashed outright once (stack dump, "submit a bug report to
   llvm-project"). Retries usually succeed. Reproducibility measurements on
   this VM should budget for toolchain flakiness under contention.
3. Intermediate rebuild attempts (build 2) failed for the above environmental
   reasons, not source reasons — the source tree built cleanly three times.
