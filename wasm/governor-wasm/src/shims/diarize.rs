//! Host-test shim for `crate::diarize` (wave-36).
//!
//! The #[path]-included parent source `src/governor.rs` references
//! `crate::diarize::SingleMicDiarizer` under
//! `#[cfg(not(target_arch = "wasm32"))]`. That module lives in the parent
//! binary crate, not in this wasm crate. This shim exists ONLY for host
//! `cargo test` (cfg'd out on wasm32 — the .wasm build is untouched).

/// Minimal stand-in: unit struct, Send (Arc-compatible).
#[derive(Debug, Default)]
pub struct SingleMicDiarizer;
