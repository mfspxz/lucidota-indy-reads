//! Host-test shim for `crate::voice` (wave-36).
//!
//! The #[path]-included parent source `src/governor.rs` references
//! `crate::voice::{clone_engine_from_env, CloneEngine}` under
//! `#[cfg(not(target_arch = "wasm32"))]`. That module lives in the parent
//! binary crate, not in this wasm crate. This shim exists ONLY for host
//! `cargo test` (cfg'd out on wasm32 — the .wasm build is untouched).
//!
//! The real trait has synthesize/check_consent machinery; the shim keeps
//! only what the type checker needs: the trait itself (object-safe, Send +
//! Sync for `Arc<dyn CloneEngine>`) and the constructor. No test constructs
//! or drives an engine here.

/// Minimal object-safe stand-in for the voice-clone engine trait.
pub trait CloneEngine: Send + Sync {}

/// Null engine: constructible, does nothing. Never exercised by tests.
pub struct NullCloneEngine;

impl CloneEngine for NullCloneEngine {}

pub fn clone_engine_from_env() -> std::sync::Arc<dyn CloneEngine> {
    std::sync::Arc::new(NullCloneEngine)
}
