//! `ie`: receipt organelle — hash-chained receipt construction primitives.
//!
//! Pure builders over the crate's receipt types: content hashing, id
//! generation, and the `ReceiptBuilder` fluent surface. The hash chain
//! itself is committed by the caller (prev_hash threading); this organelle
//! never invents chain links.

use super::{base_manifest, contract, get_str, OrganelleManifest};
use crate::receipts::{hash_str, new_id, ReceiptBuilder};
use organelle_host::{CallContext, HostRef, Organelle, OrganelleError};
use serde_json::Value;

pub struct IndyReceipts;

pub fn ie_manifest() -> OrganelleManifest {
    base_manifest(
        "indy-receipts",
        "ie",
        "Receipt construction: ids, content hashes, fluent receipt builder",
        vec![
            contract("new_id", "Generate a receipt/claim id",
                serde_json::json!({"type":"object","properties":{},"required":[]})),
            contract("hash_str", "sha256 hex of a string",
                serde_json::json!({"type":"object","required":["s"],"properties":{"s":{"type":"string"}}})),
            contract("build", "Build one receipt via the fluent builder",
                serde_json::json!({"type":"object","required":["request_id","session_id","actor","operation"],"properties":{"request_id":{"type":"string"},"session_id":{"type":"string"},"actor":{"type":"string"},"operation":{"type":"string"},"input":{"type":"string"},"decision":{"type":"string"},"epistemic":{"type":"string"},"model":{"type":"string"},"output":{"type":"string"},"success":{"type":"boolean"},"duration_ms":{"type":"integer"},"prev_hash":{"type":"string"}}})),
        ],
    )
}

impl Organelle for IndyReceipts {
    fn manifest(&self) -> OrganelleManifest {
        ie_manifest()
    }

    fn handle(
        &self,
        _ctx: &CallContext,
        _host: &dyn HostRef,
        op: &str,
        input: Value,
    ) -> Result<Value, OrganelleError> {
        match op {
            "new_id" => Ok(serde_json::json!({ "id": new_id() })),
            "hash_str" => Ok(serde_json::json!({ "sha256": hash_str(&get_str(&input, "s")?) })),
            "build" => {
                let mut b = ReceiptBuilder::new(
                    &get_str(&input, "request_id")?,
                    &get_str(&input, "session_id")?,
                    &get_str(&input, "actor")?,
                    &get_str(&input, "operation")?,
                );
                if let Some(s) = input.get("input").and_then(|v| v.as_str()) { b = b.input(s); }
                if let Some(s) = input.get("decision").and_then(|v| v.as_str()) { b = b.decision(s); }
                if let Some(s) = input.get("epistemic").and_then(|v| v.as_str()) { b = b.epistemic(s); }
                if let Some(s) = input.get("model").and_then(|v| v.as_str()) { b = b.model(s); }
                if let Some(s) = input.get("output").and_then(|v| v.as_str()) { b = b.output(s); }
                if let Some(s) = input.get("success").and_then(|v| v.as_bool()) { b = b.success(s); }
                if let Some(s) = input.get("duration_ms").and_then(|v| v.as_u64()) { b = b.duration_ms(s); }
                if let Some(s) = input.get("prev_hash").and_then(|v| v.as_str()) { b = b.prev_hash(s); }
                let receipt = b.build();
                Ok(serde_json::json!({ "receipt": serde_json::to_value(&receipt).unwrap() }))
            }
            _ => Err(OrganelleError::not_found(format!("unknown op '{op}'"))),
        }
    }
}
