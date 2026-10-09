//! MCP server stub (rmcp, stdio transport).
//!
//! STATUS: stub only. The `rmcp` dependency (official Rust MCP SDK) is
//! added and the stdio transport handle is constructed here to prove the
//! wiring, but no tools are registered and no serve loop runs yet.
//! Full MCP server wiring — exposing the governor's tools
//! (blast-radius query, WHY-chain, receipt writer) over MCP — is a
//! separate task. See INTEGRATION-NOTES.md.
//!
//! TRANSPORT POSTURE (from the crate atlas): stdio transport only. The
//! networked MCP transports (streamable HTTP, OAuth) carry the advisory
//! history; they stay off until a use case forces them.

/// Construct the stdio transport and stop. Returns an error explaining
/// that the serve loop is not yet wired — this is a deliberate stub, not
/// a silent no-op.
pub async fn serve_stdio() -> anyhow::Result<()> {
    let (_stdin, _stdout) = rmcp::transport::io::stdio();
    anyhow::bail!(
        "mcp-stdio: transport constructed but no service is registered; \
         full MCP server wiring is a separate task (see INTEGRATION-NOTES.md)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_is_explicit_not_silent() {
        let err = serve_stdio().await.expect_err("stub must not succeed silently");
        assert!(err.to_string().contains("separate task"));
    }
}
