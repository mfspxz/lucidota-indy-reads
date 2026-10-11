//! `ig`: Gmail connector organelle — fail-closed mail search/read.
//!
//! Wraps `GmailConnector`. When the gmail CLI / account is not connected,
//! every op fails closed with the disabled reason (never a silent empty).
//! Network egress (gmail CLI subprocess) is declared in execution.json.

use super::{base_manifest, contract, get_str, OrganelleManifest};
use crate::gmail::GmailConnector;
use organelle_host::{CallContext, HostRef, Organelle, OrganelleError};
use serde_json::Value;

pub struct IndyGmail {
    connector: GmailConnector,
}

impl IndyGmail {
    pub fn new() -> Self {
        Self { connector: GmailConnector::new() }
    }
}

pub fn ig_manifest() -> OrganelleManifest {
    base_manifest(
        "indy-gmail",
        "ig",
        "Fail-closed Gmail search and read via the gmail CLI",
        vec![
            contract("status", "Connector status: enabled or the disabled reason",
                serde_json::json!({"type":"object","properties":{},"required":[]})),
            contract("search", "Search mail with Gmail search syntax (fail-closed when disabled)",
                serde_json::json!({"type":"object","required":["query"],"properties":{"query":{"type":"string"},"max":{"type":"integer"}}})),
            contract("read", "Read one message by id (fail-closed when disabled)",
                serde_json::json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"}}})),
        ],
    )
}

impl Organelle for IndyGmail {
    fn manifest(&self) -> OrganelleManifest {
        ig_manifest()
    }

    fn handle(
        &self,
        _ctx: &CallContext,
        _host: &dyn HostRef,
        op: &str,
        input: Value,
    ) -> Result<Value, OrganelleError> {
        match op {
            "status" => Ok(serde_json::json!({
                "enabled": self.connector.is_enabled(),
                "disabled_reason": self.connector.disabled_reason(),
            })),
            "search" => {
                let hits = self.connector
                    .search(&get_str(&input, "query")?, input.get("max").and_then(|v| v.as_u64()).map(|m| m as u32))
                    .map_err(OrganelleError::failed)?;
                Ok(serde_json::json!({ "hits": serde_json::to_value(&hits).unwrap() }))
            }
            "read" => {
                let msg = self.connector
                    .read(&get_str(&input, "id")?)
                    .map_err(OrganelleError::failed)?;
                Ok(serde_json::json!({ "message": serde_json::to_value(&msg).unwrap() }))
            }
            _ => Err(OrganelleError::not_found(format!("unknown op '{op}'"))),
        }
    }
}
