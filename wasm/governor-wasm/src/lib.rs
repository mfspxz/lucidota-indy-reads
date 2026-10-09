//! governor-wasm: the Indy_Reads governor as a WebAssembly module.
//!
//! ANTI-DRIFT DESIGN (wave-24, Zack: "why is it JavaScript... why not rust"):
//! this crate contains ZERO governor logic of its own. It includes the real
//! sources via #[path]:
//!
//!   - ../../src/governor.rs  — the Governor decision core (decide, vetoes,
//!     provider selection/failover, consumed-once nonces, transition auth)
//!   - ../../src/domain.rs    — claims, evidence, epistemics
//!   - ../../src/receipts.rs  — hash-chained receipts
//!
//! A change to the governor changes this module on the next build, by
//! construction. If a future edit adds a wasm-unsafe dependency to those
//! files, THIS build fails — that is the intended fail-closed drift signal,
//! not a bug.
//!
//! The wasm32-unsafe parts of the sources are cfg-gated there:
//!   - AppState + store/voice/diarize/alebex imports: not(target_arch="wasm32")
//!   - now_ms(): on wasm32 the host supplies the clock via the
//!     `indy_host_now_ms` import (the JS loader provides Date.now()).
//!   - uuid v4 randomness: getrandom's `js` feature (host crypto).
//!
//! Boundary convention (same as the proven spike loader):
//!   - `#[wasm_bindgen]` exports; strings cross as JSON (&str in, String out).
//!   - Every fallible call returns JSON: the value on success, or
//!     {"error": "..."} on bad input. A Rust panic becomes a JS throw —
//!     the shell must drop and recreate the governor on any throw
//!     (a poisoned mutex never recovers).
//!   - The JS shells (api/turn.js, cf-pages/worker.js, web/voice.html) do
//!     request parsing, WASM invocation, and response shaping ONLY.
//!     No normalize, no claimsAuthority, no classifyIntent, no validateReply,
//!     no receipt chain, no provider loop in JS. Ever again.

#[path = "../../../src/domain.rs"]
mod domain;
#[path = "../../../src/receipts.rs"]
mod receipts;
#[path = "../../../src/governor.rs"]
mod governor;
#[path = "../../../src/mcp.rs"]
mod mcp;

use governor::{
    classify_intent as rs_classify_intent, claims_authority as rs_claims_authority,
    normalize_for_match, validate_reply as rs_validate_reply, AiProposal, Governor,
    ProviderDecision, ProviderOutcome, ProviderPolicy,
};
use receipts::{hash_str, Receipt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Input DTOs (JSON shapes the shells send)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct DecideInput {
    request_id: String,
    session_id: String,
    utterance: String,
    proposal: AiProposal,
    #[serde(default)]
    evidence: Vec<domain::Evidence>,
}

#[derive(Deserialize)]
struct ReviseClaimInput {
    request_id: String,
    session_id: String,
    claim: String,
    old_value: String,
    new_value: String,
    speech_interrupted: bool,
}

#[derive(Deserialize)]
struct SelectProviderInput {
    request_id: String,
    session_id: String,
    requested: Option<String>,
    policy: ProviderPolicy,
    /// Pre-probed provider health, gathered by the shell (transport job):
    /// name -> "ok" | anything-else-means-err. The governor's probe closure
    /// reads this map; the selection loop, nonce minting, override/refusal
    /// semantics, and max_probes bound stay in Rust.
    #[serde(default)]
    probes: HashMap<String, String>,
}

#[derive(Deserialize)]
struct RecordProviderResultInput {
    request_id: String,
    session_id: String,
    provider: String,
    success: bool,
    kind: String,
    duration_ms: u64,
}

#[derive(Deserialize)]
struct RecordStateTransitionInput {
    request_id: String,
    session_id: String,
    transition: String,
    detail: String,
}

#[derive(Deserialize)]
struct ValidateReplyInput {
    reply: String,
    status: String,
}

#[derive(Deserialize)]
struct StrInput {
    utterance: String,
}

// ---------------------------------------------------------------------------
// Output helpers
// ---------------------------------------------------------------------------

fn ok_json<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|e| format!(r#"{{"error":"serialize: {e}"}}"#))
}

fn err_json(msg: impl std::fmt::Display) -> String {
    serde_json::to_string(&serde_json::json!({ "error": msg.to_string() }))
        .unwrap_or_else(|_| r#"{"error":"serialize failed"}"#.to_string())
}

fn parse_input<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| format!("bad input: {e}"))
}

// ---------------------------------------------------------------------------
// Stateful governor handle
// ---------------------------------------------------------------------------

/// The real Governor, living in wasm linear memory. The shell creates one
/// per isolate/page and calls methods on it. Receipt chain, transition auth,
/// and consumed-once nonce registry all live here — in Rust, not in JS.
#[wasm_bindgen]
pub struct WasmGovernor {
    inner: Governor,
}

#[wasm_bindgen]
impl WasmGovernor {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmGovernor {
        WasmGovernor {
            inner: Governor::new(),
        }
    }

    /// The core gate: AI proposal in, deterministic GovernorDecision out.
    pub fn decide(&self, input_json: &str) -> String {
        let input: DecideInput = match parse_input(input_json) {
            Ok(v) => v,
            Err(e) => return err_json(e),
        };
        let d = self.inner.decide(
            &input.request_id,
            &input.session_id,
            &input.utterance,
            input.proposal,
            &input.evidence,
        );
        ok_json(&d)
    }

    /// Live truth reconciliation: user correction invalidates pending output.
    pub fn revise_claim(&self, input_json: &str) -> String {
        let input: ReviseClaimInput = match parse_input(input_json) {
            Ok(v) => v,
            Err(e) => return err_json(e),
        };
        let r = self.inner.revise_claim(
            &input.request_id,
            &input.session_id,
            &input.claim,
            &input.old_value,
            &input.new_value,
            input.speech_interrupted,
        );
        ok_json(&r)
    }

    /// Newest-first receipt stream (backs /api/receipts).
    pub fn recent_receipts(&self, n: u32) -> String {
        ok_json(&self.inner.recent_receipts(n as usize))
    }

    /// Deterministic provider selection under governor authority. The shell
    /// supplies pre-probed health; everything else (priority order, forbidden
    /// override, max_probes bound, fixture/refusal, nonce minting, receipt)
    /// is the governor's.
    pub fn select_provider(&self, input_json: &str) -> String {
        let input: SelectProviderInput = match parse_input(input_json) {
            Ok(v) => v,
            Err(e) => return err_json(e),
        };
        let probe = |name: &str| -> ProviderOutcome {
            match input.probes.get(name).map(String::as_str) {
                Some("ok") => ProviderOutcome::Ok,
                _ => ProviderOutcome::Err,
            }
        };
        let d = self.inner.select_provider(
            &input.request_id,
            &input.session_id,
            input.requested.as_deref(),
            &input.policy,
            &probe,
        );
        ok_json(&d)
    }

    /// Consumed-once guard: present a ProviderDecision BEFORE acting on it.
    /// Returns the consumption receipt; success=false means REJECT — the
    /// shell must fail closed (refuse), never act on the decision.
    pub fn consume_provider_decision(&self, decision_json: &str) -> String {
        let decision: ProviderDecision = match parse_input(decision_json) {
            Ok(v) => v,
            Err(e) => return err_json(e),
        };
        ok_json(&self.inner.consume_provider_decision(&decision))
    }

    /// Record a classified provider outcome (feeds failover + receipts).
    pub fn record_provider_result(&self, input_json: &str) -> String {
        let input: RecordProviderResultInput = match parse_input(input_json) {
            Ok(v) => v,
            Err(e) => return err_json(e),
        };
        let r = self.inner.record_provider_result(
            &input.request_id,
            &input.session_id,
            &input.provider,
            input.success,
            &input.kind,
            input.duration_ms,
        );
        ok_json(&r)
    }

    /// Record a state transition (governor-authorized only; the vocabulary
    /// is closed in Rust — an unknown transition is VETOed here, not in JS).
    pub fn record_state_transition(&self, input_json: &str) -> String {
        let input: RecordStateTransitionInput = match parse_input(input_json) {
            Ok(v) => v,
            Err(e) => return err_json(e),
        };
        let r = self.inner.record_state_transition(
            &input.request_id,
            &input.session_id,
            &input.transition,
            &input.detail,
        );
        ok_json(&r)
    }

    /// Verify the hash linkage of a receipt chain (oldest-first JSON array).
    /// Returns {"ok": true} or {"ok": false, "error": "..."}.
    pub fn verify_chain(&self, receipts_json: &str) -> String {
        let chain: Vec<Receipt> = match parse_input(receipts_json) {
            Ok(v) => v,
            Err(e) => return err_json(e),
        };
        match governor::verify_chain_linkage(&chain) {
            Ok(()) => r#"{"ok":true}"#.to_string(),
            Err(e) => err_json(format!("broken chain: {e}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Stateless free functions (no governor handle needed)
// ---------------------------------------------------------------------------

/// Deterministic intent classifier — the ONE classifier every transport
/// uses. Returns {"intent": "Question"|"ClaimAssertion"|"HypothesisAssertion"|
/// "AuthorityRequest"|"Converse"}.
#[wasm_bindgen]
pub fn classify_intent(input_json: &str) -> String {
    let input: StrInput = match parse_input(input_json) {
        Ok(v) => v,
        Err(e) => return err_json(e),
    };
    ok_json(&serde_json::json!({ "intent": rs_classify_intent(&input.utterance) }))
}

/// Authority-request detector (narrow primitive; the verdict path uses
/// classify_intent — see governor.rs C3).
#[wasm_bindgen]
pub fn claims_authority(input_json: &str) -> String {
    let input: StrInput = match parse_input(input_json) {
        Ok(v) => v,
        Err(e) => return err_json(e),
    };
    ok_json(&serde_json::json!({ "authority": rs_claims_authority(&input.utterance) }))
}

/// Reply guard: the reply must respect the verdict. Returns
/// {"reply": "..."} — possibly the governor's replacement line (a veto).
#[wasm_bindgen]
pub fn validate_reply(input_json: &str) -> String {
    let input: ValidateReplyInput = match parse_input(input_json) {
        Ok(v) => v,
        Err(e) => return err_json(e),
    };
    let status: domain::EpistemicStatus = match input.status.to_uppercase().as_str() {
        "PROVEN" => domain::EpistemicStatus::Proven,
        "DISPROVEN" => domain::EpistemicStatus::Disproven,
        _ => domain::EpistemicStatus::Unknown,
    };
    ok_json(&serde_json::json!({ "reply": rs_validate_reply(&input.reply, status) }))
}

/// sha256(normalize_for_match(utterance)) — the ONLY sanctioned source of
/// input normalization outside Rust. The demo shell uses it for the
/// uncertainty-scaffold move selection (presentation), so the JS mirror of
/// normalize() is deleted.
#[wasm_bindgen]
pub fn normalized_hash(input_json: &str) -> String {
    let input: StrInput = match parse_input(input_json) {
        Ok(v) => v,
        Err(e) => return err_json(e),
    };
    ok_json(&serde_json::json!({ "hash": hash_str(&normalize_for_match(&input.utterance)) }))
}

// ---------------------------------------------------------------------------
// MCP tool surface (src/mcp.rs — the 12 deterministic tools)
// ---------------------------------------------------------------------------

/// The real McpState, living in wasm linear memory. The mcp shells
/// (api/mcp.js, cf-pages/worker.js) do JSON-RPC plumbing only: parse the
/// tool call, invoke here, shape the response. No normalize, no
/// claimsAuthority, no classifyIntentGov, no assessClaim, no commitReceipt
/// in JS — all of that is src/mcp.rs, compiled, not ported.
#[wasm_bindgen]
pub struct WasmMcp {
    inner: mcp::McpState,
}

#[wasm_bindgen]
impl WasmMcp {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmMcp {
        WasmMcp {
            inner: mcp::McpState::new(),
        }
    }

    /// Execute one MCP tool: input is the tool's arguments as JSON.
    /// Returns the tool result JSON (MCP content envelope included).
    pub fn call_tool(&mut self, name: &str, args_json: &str) -> String {
        let args: serde_json::Value = match parse_input(args_json) {
            Ok(v) => v,
            Err(e) => return err_json(e),
        };
        ok_json(&mcp::call_tool(&mut self.inner, name, args))
    }
}

/// The 12 tool descriptors (GET /api/mcp).
#[wasm_bindgen]
pub fn mcp_descriptors() -> String {
    ok_json(&mcp::tool_descriptors())
}
