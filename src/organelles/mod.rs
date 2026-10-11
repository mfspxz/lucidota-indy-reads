//! Indy_Reads capability organelles (wave-4a migration).
//!
//! Five organelles over the voice research agent's real capability units.
//! Capability units only — no data directories are touched.

pub mod ai;
pub mod gmail;
pub mod mcp;
pub mod receipts;
pub mod voice;

pub use ai::{ia_manifest, IndyAi};
pub use gmail::{ig_manifest, IndyGmail};
pub use mcp::{im_manifest, IndyMcp};
pub use receipts::{ie_manifest, IndyReceipts};
pub use voice::{iv_manifest, IndyVoice};

use organelle_manifest::{
    Authority, Contract, MutationClass, OrganelleManifest, ReceiptPolicy, Transport, Valence,
    MANIFEST_SCHEMA_VERSION,
};
use serde_json::Value;
use std::sync::Arc;

pub(crate) fn base_manifest(
    name: &str,
    codon: &str,
    description: &str,
    contracts: Vec<Contract>,
) -> OrganelleManifest {
    OrganelleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        name: name.to_string(),
        codon: codon.to_string(),
        version: "0.1.0".to_string(),
        description: description.to_string(),
        contracts,
        authority: Authority {
            mutation_class: MutationClass::ReadOnly,
            declared_valence: Valence::Candidate,
            required_capability: None,
            granted_capabilities: Vec::new(),
        },
        transports: vec![Transport::Native],
        receipt_policy: ReceiptPolicy {
            content_addressed: true,
            algorithm: "sha256".to_string(),
            chain: true,
        },
    }
}

pub(crate) fn contract(op: &str, description: &str, schema: Value) -> Contract {
    Contract {
        op: op.to_string(),
        description: description.to_string(),
        input_schema: schema,
        mutation_class: None,
        required_capability: None,
    }
}

/// Register all five organelles on a host. Returns the codons registered.
pub fn register_all(host: &Arc<organelle_host::Host>) -> Result<Vec<String>, String> {
    for (org, codon) in [
        (Arc::new(IndyVoice) as Arc<dyn organelle_host::Organelle>, "iv"),
        (Arc::new(IndyGmail::new()) as Arc<dyn organelle_host::Organelle>, "ig"),
        (Arc::new(IndyMcp::new()) as Arc<dyn organelle_host::Organelle>, "im"),
        (Arc::new(IndyAi::new()) as Arc<dyn organelle_host::Organelle>, "ia"),
        (Arc::new(IndyReceipts) as Arc<dyn organelle_host::Organelle>, "ie"),
    ] {
        host.register(org)
            .map_err(|e| format!("{codon} registration failed: {e:?}"))?;
    }
    Ok(vec![
        "iv".to_string(),
        "ig".to_string(),
        "im".to_string(),
        "ia".to_string(),
        "ie".to_string(),
    ])
}

fn get_str(input: &Value, key: &str) -> Result<String, organelle_host::OrganelleError> {
    input
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| organelle_host::OrganelleError::failed(format!("missing '{key}'")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use organelle_host::{CallContext, Host};

    fn ctx() -> CallContext {
        CallContext::new("test", vec![])
    }

    fn wctx() -> CallContext {
        CallContext::new("test", vec!["im.write".to_string()])
    }

    #[test]
    fn registers_all_five_on_real_host() {
        let host = Host::new();
        let codons = register_all(&host).expect("register");
        assert_eq!(codons, vec!["iv", "ig", "im", "ia", "ie"]);
        for c in ["iv", "ig", "im", "ia", "ie"] {
            assert!(host.list_manifests().iter().any(|m| m.codon == c), "codon {c}");
        }
    }

    #[test]
    fn voice_roundtrip_through_host() {
        let host = Host::new();
        register_all(&host).expect("register");
        let seg = host.call(&ctx(), "iv", "diarize_turn",
            serde_json::json!({"speaker_id":"u1","speaker_label":"Zack","text":"hello","started_at_ms":0,"ended_at_ms":1000}))
            .expect("diarize_turn");
        assert_eq!(seg.output["segment"]["speaker_label"], "Zack");
        let v = host.call(&ctx(), "iv", "derive_voice",
            serde_json::json!({"epistemic":"unknown","intent":"question","emotion":"curious","persona":"indy"}))
            .expect("derive_voice");
        assert!(v.output["voice"]["rate"].as_f64().unwrap() > 0.0);
        assert!(host.verify_chain().is_ok());
    }

    #[test]
    fn gmail_status_shape_through_host() {
        let host = Host::new();
        register_all(&host).expect("register");
        let st = host.call(&ctx(), "ig", "status", serde_json::json!({})).expect("status");
        // Honest either way: enabled XOR disabled_reason, never both/neither.
        let enabled = st.output["enabled"].as_bool().unwrap();
        let reason = st.output["disabled_reason"].as_str();
        assert_eq!(enabled, reason.is_none(), "enabled={enabled} reason={reason:?}");
        if !enabled {
            // Fail-closed: ops error, never silent empty.
            let err = host.call(&ctx(), "ig", "search", serde_json::json!({"query":"x"})).unwrap_err();
            assert!(format!("{err:?}").contains("disabled"));
        }
        assert!(host.verify_chain().is_ok());
    }

    #[test]
    fn mcp_dispatch_through_host() {
        let host = Host::new();
        register_all(&host).expect("register");
        let d = host.call(&ctx(), "im", "tool_descriptors", serde_json::json!({})).expect("descriptors");
        assert!(d.output["descriptors"].is_array() || d.output["descriptors"].is_object());
        let r = host.call(&wctx(), "im", "call_tool",
            serde_json::json!({"name":"claim.create","args":{"text":"the moon is cheese"}}))
            .expect("call_tool");
        assert!(r.output["result"]["claim_id"].is_string() || r.output["result"]["result"].is_object() || r.output["result"].is_object());
        assert!(host.verify_chain().is_ok());
    }

    #[test]
    fn ai_fixture_interpret_through_host() {
        let host = Host::new();
        register_all(&host).expect("register");
        let p = host.call(&ctx(), "ia", "load_persona", serde_json::json!({"name":"nonexistent"}))
            .expect("load_persona");
        assert!(p.output["persona"].as_str().unwrap().len() > 0);
        // No provider keys in this env: deterministic fixture path.
        let r = host.call(&ctx(), "ia", "interpret",
            serde_json::json!({"utterance":"hello","epistemic_hint":"unknown","persona":"indy","request_id":"r1","session_id":"s1"}))
            .expect("interpret");
        assert_eq!(r.output["from_fixture"], true);
    }

    #[test]
    fn receipts_build_through_host() {
        let host = Host::new();
        register_all(&host).expect("register");
        let h = host.call(&ctx(), "ie", "hash_str", serde_json::json!({"s":"abc"})).expect("hash");
        assert_eq!(h.output["sha256"], "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let r = host.call(&ctx(), "ie", "build",
            serde_json::json!({"request_id":"r","session_id":"s","actor":"t","operation":"op","decision":"ALLOW","success":true}))
            .expect("build");
        assert_eq!(r.output["receipt"]["decision"], "ALLOW");
        assert_eq!(r.output["receipt"]["success"], true);
        assert!(host.verify_chain().is_ok());
    }

    #[test]
    fn all_manifests_validate() {
        for m in [iv_manifest(), ig_manifest(), im_manifest(), ia_manifest(), ie_manifest()] {
            let v = serde_json::to_value(&m).unwrap();
            organelle_manifest::validate_manifest_json(&v).expect("manifest validates");
        }
    }

    #[test]
    fn rust_manifest_matches_static_json() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("organelles");
        for (codon, m) in [
            ("iv", iv_manifest()),
            ("ig", ig_manifest()),
            ("im", im_manifest()),
            ("ia", ia_manifest()),
            ("ie", ie_manifest()),
        ] {
            let path = dir.join(format!("indy-reads-{codon}.manifest.json"));
            let raw = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("missing static manifest {}: {e}", path.display()));
            let static_v: serde_json::Value = serde_json::from_str(&raw).expect("static JSON parses");
            assert_eq!(serde_json::to_value(&m).unwrap(), static_v, "static manifest {codon} matches");
        }
    }
}
