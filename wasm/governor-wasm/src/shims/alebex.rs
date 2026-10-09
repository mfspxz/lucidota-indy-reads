//! Host-test shim for `crate::alebex` (wave-36).
//!
//! The #[path]-included parent source `src/governor.rs` references
//! `crate::alebex::LiveRegistry` under `#[cfg(not(target_arch = "wasm32"))]`.
//! That module lives in the parent binary crate, not in this wasm crate.
//! This shim exists ONLY for host `cargo test` (it is cfg'd out on wasm32,
//! so the .wasm build is untouched). It satisfies the type checker; the
//! live registry is never exercised by tests (AppState is never constructed
//! here).

/// Minimal stand-in: constructible, Send (Mutex-compatible).
#[derive(Debug, Default)]
pub struct LiveRegistry;

impl LiveRegistry {
    pub fn new() -> Self {
        LiveRegistry
    }
}
