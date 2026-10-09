//! Indy_Reads deterministic core.
//!
//! These organelles are the reusable substrate. The voice UI, the HTTP
//! server, and the MCP server are all thin transports over this core.
//! No transport owns truth; the governor does.

pub mod alebex;
pub mod avatar;
pub mod diarize;
pub mod domain;
pub mod embeddings;
pub mod gmail;
pub mod governor;
pub mod mcp;
pub mod mcp_stdio;
pub mod prompts;
pub mod receipts;
pub mod store;
pub mod theater;
pub mod time_util;
pub mod tokens;
pub mod tool_schemas;
pub mod voice;

/// Build-time provenance (C1). Baked in by build.rs: the binary carries its
/// own source identity, so /health can report it and a reviewer can compare
/// runtime truth against source truth.
pub mod provenance {
    pub fn report() -> serde_json::Value {
        serde_json::json!({
            "service": "indy-reads",
            "version": env!("CARGO_PKG_VERSION"),
            "commit": env!("INDY_BUILD_COMMIT"),
            "branch": env!("INDY_BUILD_BRANCH"),
            "dirty_worktree": env!("INDY_BUILD_DIRTY") == "true",
            "rustc": env!("INDY_BUILD_RUSTC"),
            "built_at": env!("INDY_BUILD_TS"),
        })
    }

    /// True when the binary was built from a real VCS commit with a clean tree.
    pub fn vcs_clean() -> bool {
        env!("INDY_BUILD_COMMIT") != "no-vcs" && env!("INDY_BUILD_DIRTY") != "true"
    }
}
