//! `im`: MCP tool-surface organelle — descriptors, forbidden-arg stripping, dispatch.
//!
//! Wraps the crate's MCP surface (`tool_descriptors`, `strip_forbidden`,
//! `call_tool`) over a Mutex-owned `McpState`. `call_tool` mutates claim /
//! evidence / receipt state: Write under `im.write`.

use super::{base_manifest, contract, get_str, Contract, MutationClass, OrganelleManifest};
use crate::mcp::{call_tool, strip_forbidden, tool_descriptors, McpState};
use organelle_host::{CallContext, HostRef, Organelle, OrganelleError};
use serde_json::Value;

pub struct IndyMcp {
    state: std::sync::Mutex<McpState>,
}

impl IndyMcp {
    pub fn new() -> Self {
        Self { state: std::sync::Mutex::new(McpState::new()) }
    }
}

pub fn im_manifest() -> OrganelleManifest {
    base_manifest(
        "indy-mcp",
        "im",
        "MCP tool surface: descriptors, forbidden-arg stripping, governed dispatch",
        vec![
            contract("tool_descriptors", "The MCP tool descriptor set",
                serde_json::json!({"type":"object","properties":{},"required":[]})),
            contract("strip_forbidden", "Strip forbidden args from a tool call; returns stripped names",
                serde_json::json!({"type":"object","required":["args"],"properties":{"args":{"type":"object"}}})),
            Contract {
                op: "call_tool".to_string(),
                description: "Dispatch one MCP tool call (mutates claim/evidence/receipt state)".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["name","args"],"properties":{"name":{"type":"string"},"args":{"type":"object"}}}),
                mutation_class: Some(MutationClass::Write),
                required_capability: Some("im.write".to_string()),
            },
        ],
    )
}

impl Organelle for IndyMcp {
    fn manifest(&self) -> OrganelleManifest {
        im_manifest()
    }

    fn handle(
        &self,
        _ctx: &CallContext,
        _host: &dyn HostRef,
        op: &str,
        input: Value,
    ) -> Result<Value, OrganelleError> {
        match op {
            "tool_descriptors" => Ok(serde_json::json!({ "descriptors": tool_descriptors() })),
            "strip_forbidden" => {
                let mut args = input.get("args").cloned().unwrap_or(Value::Null);
                let stripped = strip_forbidden(&mut args);
                Ok(serde_json::json!({ "stripped": stripped, "args": args }))
            }
            "call_tool" => {
                let mut state = self.state.lock().map_err(|_| OrganelleError::failed("mcp state lock poisoned"))?;
                let name = get_str(&input, "name")?;
                let args = input.get("args").cloned().unwrap_or(Value::Null);
                Ok(serde_json::json!({ "result": call_tool(&mut state, &name, args) }))
            }
            _ => Err(OrganelleError::not_found(format!("unknown op '{op}'"))),
        }
    }
}
