//! indy-mcp: live MCP server over the Indy_Reads deterministic core.
//!
//! Transport: newline-delimited JSON-RPC 2.0 on stdio.
//!   $ cargo run --bin indy-mcp
//!
//! Thin transport wrapper: all tool logic lives in indy_reads::mcp,
//! shared verbatim with the REST adapter (POST /api/mcp/:tool).
//! MCP clients are CALLERS, never governors.

use indy_reads::mcp::{handle, McpState};
use std::io::{self, BufRead, Write};

fn main() {
    let mut state = McpState::new();
    let stdin = io::stdin();
    let mut out = io::stdout();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(resp) = handle(&mut state, &line) {
            if let Ok(s) = serde_json::to_string(&resp) {
                let _ = writeln!(out, "{}", s);
                let _ = out.flush();
            }
        }
    }
}
