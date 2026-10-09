//! Indy_Reads receipts: every consequential operation produces one.
//!
//! A receipt is a deterministic, hash-chained record of what happened.
//! No fake receipts. No fake success.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(not(target_arch = "wasm32"))]
use uuid::Uuid;

/// Instance-unique ID. Native: real UUID v4. wasm32-unknown-unknown: the
/// getrandom js backend needs wasm-bindgen's full externref object protocol,
/// which the hand-rolled loader does not implement — so on wasm the ID is
/// `w-<host_ms>-<counter>` (host clock + instance-atomic counter). IDs need
/// UNIQUENESS, not unpredictability: replay prevention rests on the
/// consumed-once nonce registry and chain integrity on hashes, never on ID
/// entropy. Documented, not hidden.
#[cfg(not(target_arch = "wasm32"))]
pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(target_arch = "wasm32")]
pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CTR: AtomicU64 = AtomicU64::new(1);
    format!("w-{}-{}", now_ms(), CTR.fetch_add(1, Ordering::Relaxed))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub receipt_id: String,
    pub request_id: String,
    pub session_id: String,
    pub timestamp_ms: u64,
    pub actor: String,
    pub operation: String,
    pub input_hash: String,
    pub decision: String,
    pub epistemic_status: Option<String>,
    pub evidence_ids: Vec<String>,
    pub model: Option<String>,
    pub output_hash: String,
    pub duration_ms: u64,
    pub success: bool,
    pub prev_hash: String,
    /// Structured claim-revision detail. Present only on operation
    /// "claim.revise": a user correction that invalidated pending output.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub revision: Option<ClaimRevision>,
}

/// A live truth-reconciliation record: the user corrected something mid-
/// flight, the pending conversational state was mutated, and any in-progress
/// speech was invalidated. Deterministic — no model inference involved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimRevision {
    pub claim: String,
    pub old_value: String,
    pub new_value: String,
    /// Always "USER": only the user can revise a claim mid-conversation.
    pub source: String,
    /// Always "INVALIDATED": the previous output is retracted, not edited.
    pub previous_output: String,
    pub speech_interrupted: bool,
}

impl ClaimRevision {
    pub fn new(claim: &str, old_value: &str, new_value: &str, speech_interrupted: bool) -> Self {
        Self {
            claim: claim.to_string(),
            old_value: old_value.to_string(),
            new_value: new_value.to_string(),
            source: "USER".to_string(),
            previous_output: "INVALIDATED".to_string(),
            speech_interrupted,
        }
    }
}

pub fn hash_str(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// wasm32-unknown-unknown has no OS clock (SystemTime::now panics). The
/// host supplies time: the JS loader MUST provide the `indy_host_now_ms`
/// import (Date.now()). Same signature, same units — the clock is an input,
/// not a secret.
#[cfg(target_arch = "wasm32")]
pub fn now_ms() -> u64 {
    unsafe { indy_host_now_ms() as u64 }
}

/// Raw host import: the JS loader provides `env.indy_host_now_ms`
/// (Date.now()). Declared with an explicit wasm import module so the
/// linker emits an import instead of an undefined-symbol error.
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn indy_host_now_ms() -> f64;
}

pub struct ReceiptBuilder {
    receipt: Receipt,
}

impl ReceiptBuilder {
    pub fn new(request_id: &str, session_id: &str, actor: &str, operation: &str) -> Self {
        Self {
            receipt: Receipt {
                receipt_id: new_id(),
                request_id: request_id.to_string(),
                session_id: session_id.to_string(),
                timestamp_ms: now_ms(),
                actor: actor.to_string(),
                operation: operation.to_string(),
                input_hash: String::new(),
                decision: String::new(),
                epistemic_status: None,
                evidence_ids: Vec::new(),
                model: None,
                output_hash: String::new(),
                duration_ms: 0,
                success: false,
                prev_hash: String::new(),
                revision: None,
            },
        }
    }

    pub fn input(mut self, s: &str) -> Self {
        self.receipt.input_hash = hash_str(s);
        self
    }

    pub fn decision(mut self, d: impl Into<String>) -> Self {
        self.receipt.decision = d.into();
        self
    }

    pub fn epistemic(mut self, e: impl Into<String>) -> Self {
        self.receipt.epistemic_status = Some(e.into());
        self
    }

    pub fn model(mut self, m: impl Into<String>) -> Self {
        self.receipt.model = Some(m.into());
        self
    }

    pub fn output(mut self, s: &str) -> Self {
        self.receipt.output_hash = hash_str(s);
        self
    }

    pub fn duration_ms(mut self, ms: u64) -> Self {
        self.receipt.duration_ms = ms;
        self
    }

    pub fn success(mut self, ok: bool) -> Self {
        self.receipt.success = ok;
        self
    }

    pub fn prev_hash(mut self, h: &str) -> Self {
        self.receipt.prev_hash = h.to_string();
        self
    }

    /// Attach a claim-revision record. Only meaningful with
    /// operation "claim.revise".
    pub fn revision(mut self, r: ClaimRevision) -> Self {
        self.receipt.revision = Some(r);
        self
    }

    pub fn build(self) -> Receipt {
        self.receipt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_receipt_created_for_decision() {
        let r = ReceiptBuilder::new("req-1", "sess-1", "governor", "claim.assess")
            .input("lake monster")
            .decision("QUALIFIED")
            .success(true)
            .build();
        assert_eq!(r.operation, "claim.assess");
        assert!(!r.receipt_id.is_empty());
        assert!(!r.input_hash.is_empty());
    }

    #[test]
    fn test_receipt_contains_request_id() {
        let r = ReceiptBuilder::new("req-abc", "sess-1", "governor", "test")
            .decision("ALLOW")
            .success(true)
            .build();
        assert_eq!(r.request_id, "req-abc");
    }

    #[test]
    fn test_input_hash_is_deterministic() {
        assert_eq!(hash_str("hello"), hash_str("hello"));
        assert_ne!(hash_str("hello"), hash_str("world"));
    }
}

#[cfg(test)]
mod corner_tests {
    use super::*;

    #[test]
    fn test_claim_revision_pins_structural_constants() {
        // source and previous_output are hardcoded by construction:
        // only the user revises, and revision always invalidates.
        let r = ClaimRevision::new("expedition_year", "1996", "1994", true);
        assert_eq!(r.source, "USER");
        assert_eq!(r.previous_output, "INVALIDATED");
        assert_eq!(r.claim, "expedition_year");
        assert_eq!(r.old_value, "1996");
        assert_eq!(r.new_value, "1994");
        assert!(r.speech_interrupted);
    }

    #[test]
    fn test_revision_defaults_to_none() {
        // A receipt without an explicit revision carries none — the
        // "claim.revise" operation is the only one that should attach it.
        let r = ReceiptBuilder::new("req-1", "sess-1", "governor", "claim.assess")
            .decision("QUALIFIED")
            .success(true)
            .build();
        assert!(r.revision.is_none());
    }

    #[test]
    fn test_revision_serialization_round_trip() {
        let r = ReceiptBuilder::new("req-9", "sess-9", "governor", "claim.revise")
            .decision("REVISED")
            .success(true)
            .revision(ClaimRevision::new("c", "old", "new", false))
            .build();
        let json = serde_json::to_string(&r).unwrap();
        let back: Receipt = serde_json::from_str(&json).unwrap();
        let rev = back.revision.expect("revision must survive the round trip");
        assert_eq!(rev.source, "USER");
        assert_eq!(rev.previous_output, "INVALIDATED");
        assert_eq!(rev.new_value, "new");
        assert!(!rev.speech_interrupted);
    }

    #[test]
    fn test_revision_absent_from_json_when_none() {
        // skip_serializing_if keeps non-revision receipts compact.
        let r = ReceiptBuilder::new("req-1", "sess-1", "governor", "claim.assess")
            .decision("QUALIFIED")
            .success(true)
            .build();
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("revision"), "revision key must be absent");
    }
}
