//! Host-test shim for `crate::store` (wave-36).
//!
//! The #[path]-included parent source `src/governor.rs` references
//! `crate::store::Store` under `#[cfg(not(target_arch = "wasm32"))]` and
//! calls `Store::open(path)?` inside `AppState::open`, which returns
//! `rusqlite::Result<Self>`. That module lives in the parent binary crate,
//! not in this wasm crate. This shim exists ONLY for host `cargo test`
//! (cfg'd out on wasm32 — the .wasm build is untouched). It satisfies the
//! type checker with the real `rusqlite` crate (target-gated dep); the chat
//! store is never opened by tests (AppState is never constructed here).

/// Minimal stand-in: openable, Send (Mutex-compatible).
#[derive(Debug, Default)]
pub struct Store;

impl Store {
    pub fn open(_path: &str) -> rusqlite::Result<Store> {
        Ok(Store)
    }
}
