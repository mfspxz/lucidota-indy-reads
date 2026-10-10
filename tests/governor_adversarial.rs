//! C4 adversarial suite: the app layer TRIES to bypass the governor.
//!
//! Each test is one attack against governor authority. Outcomes:
//!   BLOCKED — the governor's code stopped it (veto receipt in the chain), or
//!   FINDING — the bypass succeeded; filed honestly in the wave-14 receipt.
//!
//! These are integration tests on purpose: they use only the governor's
//! PUBLIC API, exactly what a hostile or sloppy app-layer caller can reach.
//!
//! PROPERTY SUITE (R-7, 2026-10-10): every test here states a SECURITY
//! INVARIANT in implementation-free terms ("authority smuggled by the app
//! layer never survives sanitize", "a replayed provider decision is
//! REJECTED", "false certainty is replaced by the governor's verdict").
//! Never rewrite one of these expectations to match a behavior change
//! without adversarial sign-off — that would turn the suite into a
//! tautology guard. Change-detection coverage lives in the governor module's
//! own unit tests and in equiv_turn_paths.rs (explicitly labeled there).

use indy_reads::governor::{
    reconstruct_chain, verify_chain_linkage, AiProposal, Governor, ProviderOutcome, ProviderPolicy,
};
use indy_reads::receipts::Receipt;

fn proposal() -> AiProposal {
    AiProposal {
        intent: "assess_claim".into(),
        claim: Some("lake monster exists".into()),
        hypothesis: None,
        requested_action: Some("assess".into()),
        response_style: Some("conversational".into()),
        authority: None,
        verified: None,
        execute: None,
        claim_status: None,
    }
}

fn policy(order: &[&str], max_probes: usize, allow_fixture: bool) -> ProviderPolicy {
    ProviderPolicy {
        order: order.iter().map(|s| s.to_string()).collect(),
        forbidden: vec![],
        allow_fixture,
        max_probes,
    }
}

fn err_probe(_name: &str) -> ProviderOutcome {
    ProviderOutcome::Err
}

fn ok_probe(_name: &str) -> ProviderOutcome {
    ProviderOutcome::Ok
}

// ── ATTACK 1: direct provider call ─────────────────────────────────────
// The app calls a provider WITHOUT consulting select_provider, then
// fabricates a receipt claiming the governor authorized it.
#[test]
fn direct_provider_call_cannot_forge_governor_authorization() {
    let g = Governor::new();
    // No select_provider call ever happens. The app hand-builds a receipt
    // claiming governor authorization for a direct call.
    let forged = Receipt {
        receipt_id: "forged-1".to_string(),
        request_id: "r-evil".to_string(),
        session_id: "s-evil".to_string(),
        timestamp_ms: 0,
        actor: "governor".to_string(),
        operation: "governor.select_provider".to_string(),
        input_hash: String::new(),
        decision: "SELECT evil".to_string(),
        epistemic_status: None,
        evidence_ids: vec![],
        model: None,
        output_hash: String::new(),
        duration_ms: 0,
        success: true,
        prev_hash: "genesis".to_string(),
        revision: None,
    };
    // BLOCKED: the governor's stream is the source of truth, and
    // commit_receipt is private — the forgery can never enter the chain.
    let stream = g.recent_receipts(16);
    assert!(
        !stream.iter().any(|r| r.receipt_id == "forged-1"),
        "forged receipt must be absent from the governor's stream"
    );
    // A verifier reconstructing the chain for the attacker's request finds
    // zero authorization.
    let chain = reconstruct_chain(&stream, "r-evil");
    assert!(
        chain.is_empty(),
        "no governor authorization exists for the ungoverned request"
    );
    // Even pairwise linkage against a real chain rejects the splice: the
    // forged receipt's prev_hash does not match the real chain hash.
    let real = g.select_provider("r-real", "s", None, &policy(&["alpha"], 8, true), &ok_probe);
    assert!(real.receipt.request_id == "r-real");
    let real_chain = reconstruct_chain(&g.recent_receipts(16), "r-real");
    let mut spliced = real_chain.clone();
    spliced.push(forged);
    assert!(
        verify_chain_linkage(&spliced).is_err(),
        "spliced forgery must break hash linkage"
    );
}

// ── ATTACK 2: over-budget resource request ─────────────────────────────
// The app stuffs 20 providers into the order hoping for an unbounded probe
// sweep (latency/cost blowup as a DoS vector).
#[test]
fn over_budget_probe_sweep_is_bounded_by_governor() {
    let g = Governor::new();
    let names: Vec<String> = (0..20).map(|i| format!("p{}", i)).collect();
    let order_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    let d = g.select_provider(
        "r-budget",
        "s",
        None,
        &policy(&order_refs, 3, true),
        &err_probe,
    );
    // BLOCKED: the governor's max_probes bound holds — 3 probes, not 20.
    assert_eq!(
        d.attempted.len(),
        3,
        "governor must never probe more than max_probes"
    );
    assert_eq!(d.selected.as_deref(), Some("fixture"));
    assert!(
        d.reason.contains("max_probes=3"),
        "the bound must be named in the veto record, got: {}",
        d.reason
    );
    assert!(d.receipt.decision.contains("FIXTURE"));
}

// ── ATTACK 3: unauthorized state transition ────────────────────────────
// The app records a transition the governor never authorized.
#[test]
fn unauthorized_state_transition_is_blocked_with_veto_receipt() {
    let g = Governor::new();
    // wave-16 fix: record_state_transition now enforces authorization.
    // Case A: invented transition vocabulary — rejected even with a
    // governor decision behind the request.
    let _d = g.decide("r-trans", "s", "hello", proposal(), &[]);
    let r = g.record_state_transition("r-trans", "s", "turn.approve_all", "app said so");
    assert!(!r.success, "forged transition must be BLOCKED");
    assert!(
        r.decision.contains("VETO"),
        "rejection must carry a veto receipt, got: {}",
        r.decision
    );
    assert_eq!(r.operation, "turn.state");
    // Case B: legitimate vocabulary but no governor decision for the
    // request — also rejected.
    let r2 = g.record_state_transition("r-never-decided", "s", "turn.complete", "app said so");
    assert!(
        !r2.success,
        "transition without a governor decision must be BLOCKED"
    );
    assert!(r2.decision.contains("VETO"));
    // The veto receipts are in the chain (attempt = evidence); the forged
    // transition itself is NOT recorded as ordered.
    let chain = reconstruct_chain(&g.recent_receipts(8), "r-trans");
    assert!(
        chain.iter().all(|r| r.decision != "turn.approve_all"),
        "forged transition must never land as governor-ordered"
    );
}

// ── ATTACK 3b: replayed transition ─────────────────────────────────────
// One decision authorizes exactly one transition record.
#[test]
fn replayed_state_transition_is_blocked() {
    let g = Governor::new();
    let _d = g.decide("r-replay", "s", "hello", proposal(), &[]);
    let ok = g.record_state_transition("r-replay", "s", "turn.complete", "first");
    assert!(ok.success, "first authorized transition must record");
    let again = g.record_state_transition("r-replay", "s", "turn.complete", "second");
    assert!(
        !again.success,
        "replayed transition must be BLOCKED after the authorization is consumed"
    );
    assert!(again.decision.contains("VETO"));
}

// ── ATTACK 4: suppressed refusal ───────────────────────────────────────
// All providers down, fixture forbidden. The app tries to get a usable
// provider out of the governor anyway.
#[test]
fn suppressed_refusal_yields_no_usable_provider() {
    let g = Governor::new();
    let d = g.select_provider(
        "r-refuse",
        "s",
        None,
        &policy(&["a", "b"], 8, false),
        &err_probe,
    );
    // BLOCKED: refused, nothing selected, REFUSE in the permanent record.
    assert!(
        d.refused,
        "governor must refuse when all down and fixture forbidden"
    );
    assert!(
        d.selected.is_none(),
        "no provider may leak out of a refusal"
    );
    assert!(
        d.receipt.decision.contains("REFUSE"),
        "refusal must be receipted, got: {}",
        d.receipt.decision
    );
    assert!(
        d.reason.contains("refusing rather than fabricating"),
        "reason must name the anti-fabrication rule"
    );
    // The app cannot "suppress" the refusal: there is no provider to use.
}

// ── ATTACK 5: forged approval ──────────────────────────────────────────
// The app constructs a ProviderDecision struct claiming the governor
// approved a forbidden provider, and presents its hand-built receipt.
#[test]
fn forged_approval_is_absent_from_the_governor_stream() {
    let g = Governor::new();
    let pol = ProviderPolicy {
        order: vec!["alpha".to_string()],
        forbidden: vec!["evil".to_string()],
        allow_fixture: true,
        max_probes: 8,
    };
    // Real decision first, to give the chain a head.
    let real = g.select_provider("r-ok", "s", Some("evil"), &pol, &ok_probe);
    assert!(real.overridden, "forbidden request must be overridden");
    assert_eq!(real.selected.as_deref(), Some("alpha"));
    // Now the forgery: a hand-built receipt claiming SELECT evil.
    let forged = Receipt {
        receipt_id: "forged-2".to_string(),
        request_id: "r-ok".to_string(),
        session_id: "s".to_string(),
        timestamp_ms: 0,
        actor: "governor".to_string(),
        operation: "governor.select_provider".to_string(),
        input_hash: String::new(),
        decision: "SELECT evil".to_string(),
        epistemic_status: None,
        evidence_ids: vec![],
        model: None,
        output_hash: String::new(),
        duration_ms: 0,
        success: true,
        prev_hash: real.receipt.prev_hash.clone(),
        revision: None,
    };
    // BLOCKED: the verifier pulls the chain from the governor's stream —
    // the forgery is not there, and the real decision says OVERRIDE.
    let stream = g.recent_receipts(16);
    assert!(!stream.iter().any(|r| r.receipt_id == "forged-2"));
    let chain = reconstruct_chain(&stream, "r-ok");
    assert!(chain.iter().any(|r| r.decision.contains("OVERRIDE")));
    assert!(!chain.iter().any(|r| r.decision == "SELECT evil"));
    let _ = forged;
}

// ── ATTACK 6: replayed decision ────────────────────────────────────────
// The app takes a real decision from request r1 and presents it twice,
// or reuses it for r2 without consulting the governor.
// wave-18 fix: consumed-once guard. select_provider() mints a fresh
// nonce bound to (request_id, session_id); consume_provider_decision()
// retires it on first presentation. Replays are REJECTed at decision
// time with a veto receipt — prevention, not audit detection.
#[test]
fn replayed_decision_is_rejected_at_consumption() {
    let g = Governor::new();
    let pol = policy(&["alpha"], 8, true);
    let d1 = g.select_provider("r1", "s", None, &pol, &ok_probe);
    assert_eq!(d1.selected.as_deref(), Some("alpha"));
    // First presentation: authorized, nonce consumed.
    let ok = g.consume_provider_decision(&d1);
    assert!(ok.success, "first presentation must be authorized");
    assert!(ok.decision.contains("AUTHORIZED"));
    // BLOCKED: second presentation of the same decision is REJECTed.
    let again = g.consume_provider_decision(&d1);
    assert!(
        !again.success,
        "replayed decision must be REJECTed after the nonce is consumed"
    );
    assert!(
        again.decision.contains("REJECT"),
        "rejection must be explicit, got: {}",
        again.decision
    );
    assert!(again.decision.contains("replayed"));
    // The veto receipt is in the chain (attempt = evidence); the replay
    // was never executed.
    let chain = reconstruct_chain(&g.recent_receipts(16), "r1");
    assert!(chain.iter().any(|r| r.decision.contains("REJECT")));
    assert!(verify_chain_linkage(&chain).is_ok());
}

// ── ATTACK 6b: transplanted decision ───────────────────────────────────
// The app takes r1's decision and presents it as authorization for r2.
#[test]
fn transplanted_decision_to_another_request_is_rejected() {
    let g = Governor::new();
    let pol = policy(&["alpha"], 8, true);
    let d1 = g.select_provider("r1", "s", None, &pol, &ok_probe);
    let ok = g.consume_provider_decision(&d1);
    assert!(ok.success);
    // Variant A: present the r1 decision struct "for r2". The nonce is
    // bound to r1 in the governor's registry — already consumed there.
    let replay_for_r2 = g.consume_provider_decision(&d1);
    assert!(
        !replay_for_r2.success,
        "transplanted replay must be REJECTed"
    );
    // Variant B: the app forges the struct's request_id to r2. The
    // registry key (nonce, r2, s) was never issued — fail closed.
    let mut forged = d1.clone();
    forged.receipt.request_id = "r2".to_string();
    let transplant = g.consume_provider_decision(&forged);
    assert!(
        !transplant.success,
        "transplanted nonce must be REJECTed as unknown"
    );
    assert!(transplant.decision.contains("REJECT"));
    // r2 has no authorization of any kind.
    let chain_r2 = reconstruct_chain(&g.recent_receipts(16), "r2");
    assert!(
        chain_r2
            .iter()
            .all(|r| !r.success || !r.decision.contains("AUTHORIZED")),
        "r2 must hold no authorization"
    );
}

// ── ATTACK 6c: forged decision nonce ───────────────────────────────────
// The app hand-builds a ProviderDecision with an invented nonce.
#[test]
fn forged_decision_nonce_is_rejected() {
    let g = Governor::new();
    let forged_decision = indy_reads::governor::ProviderDecision {
        decision_id: "pd-999999".to_string(),
        selected: Some("evil".to_string()),
        attempted: vec![],
        overridden: false,
        refused: false,
        reason: "forged".to_string(),
        receipt: Receipt {
            receipt_id: "forged-3".to_string(),
            request_id: "r-evil".to_string(),
            session_id: "s".to_string(),
            timestamp_ms: 0,
            actor: "governor".to_string(),
            operation: "governor.select_provider".to_string(),
            input_hash: String::new(),
            decision: "SELECT evil".to_string(),
            epistemic_status: None,
            evidence_ids: vec![],
            model: None,
            output_hash: String::new(),
            duration_ms: 0,
            success: true,
            prev_hash: "genesis".to_string(),
            revision: None,
        },
    };
    // BLOCKED: never-issued nonce → REJECT, fail closed.
    let r = g.consume_provider_decision(&forged_decision);
    assert!(!r.success, "forged nonce must be REJECTed");
    assert!(r.decision.contains("REJECT"));
    assert!(r.decision.contains("unknown"));
}

// ── ATTACK 6d: no false positive ───────────────────────────────────────
// Two distinct decisions with the same payload shape must both consume:
// the guard keys on the nonce, not on the decision content.
#[test]
fn distinct_decisions_same_shape_both_consume() {
    let g = Governor::new();
    let pol = policy(&["alpha"], 8, true);
    let d1 = g.select_provider("r1", "s", None, &pol, &ok_probe);
    let d2 = g.select_provider("r2", "s", None, &pol, &ok_probe);
    assert_ne!(
        d1.decision_id, d2.decision_id,
        "each decision must carry a distinct nonce"
    );
    let c1 = g.consume_provider_decision(&d1);
    let c2 = g.consume_provider_decision(&d2);
    assert!(c1.success, "first decision must consume cleanly");
    assert!(
        c2.success,
        "second same-shaped decision must consume cleanly"
    );
}
