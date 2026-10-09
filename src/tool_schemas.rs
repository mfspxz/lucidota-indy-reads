//! Tool schemas (schemars).
//!
//! The governor's tool-callable surface as JSON Schema, derived from Rust
//! types — the struct is the single source of truth, never a hand-written
//! schema file. This feeds future MCP tool definitions (see mcp_stdio).
//!
//! DRIFT NOTE (depenaudit K-1, 2026-10-09): `ProposalTool` is DELIBERATELY
//! narrower than `governor::AiProposal`. The schema is the language-only
//! contract the model sees ({reply, response_style}); `AiProposal` keeps its
//! classification fields for internal construction (fixtures, refusal path),
//! defaulted on deserialize so a model that omits them parses cleanly.
//! The `proposal_schema_is_language_only` test fails the build if the two
//! drift out of this contract.

use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};

/// The fields an LLM tool call may set on a proposal.
///
/// Language-only contract (depenaudit K-1, 2026-10-09): the model produces
/// prose, never classifications. `intent`, `claim`, `hypothesis`, and
/// `requested_action` were deleted — production code never read them; the
/// deterministic governor owns classification (`classify_intent`,
/// `evaluate_utterance` in governor.rs). Authority-bearing fields
/// (`authority`, `verified`, `execute`, `claim_status`) remain deliberately
/// absent — the governor strips and records them; they are not part of any
/// tool contract.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProposalTool {
    pub response_style: Option<String>,
    pub reply: String,
}

/// JSON Schema for the proposal tool parameters (draft 2020-12, what LLM
/// providers expect).
pub fn proposal_tool_schema() -> serde_json::Value {
    serde_json::to_value(schema_for!(ProposalTool)).expect("schema serialization cannot fail")
}

/// Full tool definition object: name + description + parameters.
pub fn proposal_tool_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "submit_proposal",
        "description": "Submit a turn proposal to the Indy_Reads governor. The governor validates, sanitizes authority-bearing fields, and decides. Output is a proposal, never an action.",
        "parameters": proposal_tool_schema(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn proposal_schema_is_language_only() {
        // Depenaudit K-1: the tool schema is the language-only contract.
        // Classification fields (intent/claim/hypothesis/requested_action)
        // must never reappear here — the governor owns classification.
        let schema = proposal_tool_schema();
        let props: BTreeSet<String> = schema["properties"]
            .as_object()
            .expect("schema has properties")
            .keys()
            .cloned()
            .collect();
        let expected: BTreeSet<String> = ["reply", "response_style"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(props, expected);
    }

    #[test]
    fn tool_definition_snapshot() {
        let def = proposal_tool_definition();
        assert_eq!(def["name"], "submit_proposal");
        insta::assert_json_snapshot!(def);
    }

    #[test]
    fn schema_round_trips_through_tempfile() {
        // tempfile proves the dev-dep links; the schema must survive a
        // write/read round-trip byte-identical (deterministic output).
        let def = proposal_tool_definition();
        let text = serde_json::to_string_pretty(&def).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tool.json");
        std::fs::write(&path, &text).unwrap();
        let back = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text, back);
    }
}
