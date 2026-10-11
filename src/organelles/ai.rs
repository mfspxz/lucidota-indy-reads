//! `ia`: AI router organelle — the governed 5% generative layer.
//!
//! Wraps `AiClient::from_env()` (key-gated provider lanes; deterministic
//! fixture mode when no keys) and `Governor::new()`. `interpret` runs the
//! REAL control path (`interpret_governed`) on a dedicated tokio runtime —
//! the governor selects the provider, attempts are classified and receipted,
//! failover re-enters the governor. The AI never decides authority/truth/
//! permission; its output is a proposal.

use super::{base_manifest, contract, get_str, OrganelleManifest};
use crate::ai::AiClient;
use crate::governor::Governor;
use organelle_host::{CallContext, HostRef, Organelle, OrganelleError};
use serde_json::Value;

pub struct IndyAi {
    client: AiClient,
    governor: Governor,
    rt: tokio::runtime::Runtime,
}

impl IndyAi {
    pub fn new() -> Self {
        Self {
            client: AiClient::from_env(),
            governor: Governor::new(),
            rt: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime for ia organelle"),
        }
    }
}

pub fn ia_manifest() -> OrganelleManifest {
    base_manifest(
        "indy-ai",
        "ia",
        "Governed AI interpretation: provider selection, classified failover, fixture mode",
        vec![
            contract("load_persona", "Load a persona system prompt (falls back to embedded Indy)",
                serde_json::json!({"type":"object","required":["name"],"properties":{"name":{"type":"string"}}})),
            contract("interpret", "One governed turn: governor-selected provider, trail recorded",
                serde_json::json!({"type":"object","required":["utterance","epistemic_hint","persona","request_id","session_id"],"properties":{"utterance":{"type":"string"},"epistemic_hint":{"type":"string"},"persona":{"type":"string"},"request_id":{"type":"string"},"session_id":{"type":"string"},"requested":{"type":"string"}}})),
        ],
    )
}

impl Organelle for IndyAi {
    fn manifest(&self) -> OrganelleManifest {
        ia_manifest()
    }

    fn handle(
        &self,
        _ctx: &CallContext,
        _host: &dyn HostRef,
        op: &str,
        input: Value,
    ) -> Result<Value, OrganelleError> {
        match op {
            "load_persona" => {
                Ok(serde_json::json!({ "persona": crate::ai::load_persona(&get_str(&input, "name")?) }))
            }
            "interpret" => {
                let requested = input.get("requested").and_then(|v| v.as_str());
                let reply = self.rt.block_on(self.client.interpret_governed(
                    &get_str(&input, "utterance")?,
                    &get_str(&input, "epistemic_hint")?,
                    &get_str(&input, "persona")?,
                    &get_str(&input, "request_id")?,
                    &get_str(&input, "session_id")?,
                    &self.governor,
                    requested,
                ));
                Ok(serde_json::json!({
                    "provider": reply.provider,
                    "refused": reply.refused,
                    "overridden": reply.overridden,
                    "from_fixture": reply.from_fixture,
                    "trail": reply.trail,
                    "reply": serde_json::to_value(&reply.reply).unwrap(),
                }))
            }
            _ => Err(OrganelleError::not_found(format!("unknown op '{op}'"))),
        }
    }
}
