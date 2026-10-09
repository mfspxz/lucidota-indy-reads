//! Tool schemas (schemars).
//!
//! The governor's tool-callable surface as JSON Schema, derived from Rust
//! types — the struct is the single source of truth, never a hand-written
//! schema file. This feeds future MCP tool definitions (see mcp_stdio).
//!
//! DRIFT NOTE: `ProposalTool` mirrors `governor::AiProposal`'s tool-facing
//! fields. `AiProposal` lives in the wasm-shared `governor.rs`, which this
//! module deliberately does not modify (the wasm crate would need the new
//! dep too). The `proposal_fields_match_governor` test fails the build if
//! the two drift apart.

use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};

/// The fields an LLM tool call may set on a proposal. Authority-bearing
/// fields (`authority`, `verified`, `execute`, `claim_status`) are
/// deliberately absent — the governor strips and records them; they are
/// not part of any tool contract.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProposalTool {
    pub intent: String,
    pub claim: Option<String>,
    pub hypothesis: Option<String>,
    pub requested_action: Option<String>,
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
    use crate::governor::AiProposal;
    use std::collections::BTreeSet;

    #[test]
    fn proposal_fields_match_governor() {
        // The schema's property set must equal AiProposal's serialized
        // field set minus the authority-bearing fields the governor strips.
        let schema = proposal_tool_schema();
        let props: BTreeSet<String> = schema["properties"]
            .as_object()
            .expect("schema has properties")
            .keys()
            .cloned()
            .collect();
        let proposal = AiProposal {
            intent: String::new(),
            claim: None,
            hypothesis: None,
            requested_action: None,
            response_style: None,
            authority: None,
            verified: None,
            execute: None,
            claim_status: None,
        };
        let all: BTreeSet<String> = serde_json::to_value(&proposal)
            .expect("serializes")
            .as_object()
            .expect("is object")
            .keys()
            .cloned()
            .collect();
        let stripped: BTreeSet<String> = ["authority", "verified", "execute", "claim_status"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let expected: BTreeSet<String> = all.difference(&stripped).cloned().collect();
        // ProposalTool adds `reply` (the spoken text) on top of AiProposal.
        let mut expected_with_reply = expected;
        expected_with_reply.insert("reply".to_string());
        assert_eq!(props, expected_with_reply);
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
