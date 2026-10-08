//! Indy_Reads domain model: claims, evidence, epistemics, sessions.
//!
//! HARD LAW: the LLM never owns these types. They are constructed and
//! evaluated only by deterministic Rust code.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Ternary epistemic status. UNKNOWN is a valid result, not a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EpistemicStatus {
    Proven,
    Disproven,
    #[default]
    Unknown,
}

/// Where a claim came from. A GeneratedHypothesis is NEVER silently
/// promoted to a ProvenFact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceSource {
    User,
    System,
    ExternalSource,
    GeneratedHypothesis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub id: String,
    pub text: String,
    pub status: EpistemicStatus,
    pub provenance: ProvenanceSource,
    pub evidence_ids: Vec<String>,
}

impl Claim {
    pub fn new(text: impl Into<String>, provenance: ProvenanceSource) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            text: text.into(),
            status: EpistemicStatus::Unknown,
            provenance,
            evidence_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Observation,
    Report,
    Document,
    Image,
    Measurement,
    Statement,
    Inference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    Unverified,
    Verified,
    Contradicted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub id: String,
    pub source: String,
    pub text: String,
    pub kind: EvidenceKind,
    pub status: EvidenceStatus,
    pub hash: String,
    /// True only for explicitly marked demo fixtures. Never presented as real.
    pub fixture: bool,
}

/// Evidence policy: what counts as "sufficient evidence" is EXPLICIT,
/// versioned, and auditable — never the programmer's hidden opinion.
/// The policy is referenced by ID in every epistemic receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidencePolicy {
    pub policy_id: String,
    pub version: String,
    /// Verified supporting evidence items required for PROVEN.
    pub proven_threshold: usize,
    /// Verified contradictory evidence items required for DISPROVEN.
    pub disproven_threshold: usize,
    /// Inferences (hypotheses) never count toward either threshold.
    pub inferences_count: bool,
}

impl Default for EvidencePolicy {
    fn default() -> Self {
        Self {
            policy_id: "research.standard.v1".to_string(),
            version: "1".to_string(),
            proven_threshold: 2,
            disproven_threshold: 2,
            inferences_count: false,
        }
    }
}

/// Deterministic oracle: the LLM cannot override this.
///
/// The epistemic state belongs to the CLAIM, not the conversation, model,
/// or source. Transitions:
///   verified supporting evidence >= proven_threshold => PROVEN
///   verified contradictory evidence >= disproven_threshold => DISPROVEN
///   otherwise => UNKNOWN
///
/// Absence of evidence is NOT evidence of disproof: no supporting evidence
/// yields UNKNOWN, never DISPROVEN.
pub fn assess_claim(
    _claim: &Claim,
    evidence: &[Evidence],
    policy: &EvidencePolicy,
) -> EpistemicStatus {
    let counts = |status: EvidenceStatus| {
        evidence
            .iter()
            .filter(|e| {
                e.status == status
                    && (policy.inferences_count || !matches!(e.kind, EvidenceKind::Inference))
            })
            .count()
    };
    let support = counts(EvidenceStatus::Verified);
    let contra = counts(EvidenceStatus::Contradicted);
    if support >= policy.proven_threshold && support > contra {
        EpistemicStatus::Proven
    } else if contra >= policy.disproven_threshold && contra > support {
        EpistemicStatus::Disproven
    } else {
        EpistemicStatus::Unknown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub claims: HashMap<String, Claim>,
    pub evidence: HashMap<String, Evidence>,
    pub turns: u64,
}

impl Session {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            claims: HashMap::new(),
            evidence: HashMap::new(),
            turns: 0,
        }
    }

    /// Session with a client-supplied id (validated by the caller).
    pub fn with_id(id: String) -> Self {
        Self {
            id,
            claims: HashMap::new(),
            evidence: HashMap::new(),
            turns: 0,
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(status: EvidenceStatus) -> Evidence {
        Evidence {
            id: Uuid::new_v4().to_string(),
            source: "test".into(),
            text: "test".into(),
            kind: EvidenceKind::Report,
            status,
            hash: "abc".into(),
            fixture: true,
        }
    }

    #[test]
    fn test_unknown_is_preserved() {
        let c = Claim::new("lake monster exists", ProvenanceSource::User);
        assert_eq!(
            assess_claim(&c, &[], &EvidencePolicy::default()),
            EpistemicStatus::Unknown
        );
    }

    #[test]
    fn test_user_claim_is_not_verified() {
        // A user assertion alone never promotes a claim.
        let c = Claim::new("x", ProvenanceSource::User);
        let e = ev(EvidenceStatus::Unverified);
        assert_eq!(
            assess_claim(&c, &[e], &EvidencePolicy::default()),
            EpistemicStatus::Unknown
        );
    }

    #[test]
    fn test_verified_evidence_can_promote_claim() {
        let c = Claim::new("x", ProvenanceSource::User);
        let es = vec![ev(EvidenceStatus::Verified), ev(EvidenceStatus::Verified)];
        assert_eq!(
            assess_claim(&c, &es, &EvidencePolicy::default()),
            EpistemicStatus::Proven
        );
    }

    #[test]
    fn test_contradictory_evidence_disproves_claim() {
        let c = Claim::new("x", ProvenanceSource::User);
        let es = vec![
            ev(EvidenceStatus::Contradicted),
            ev(EvidenceStatus::Contradicted),
        ];
        assert_eq!(
            assess_claim(&c, &es, &EvidencePolicy::default()),
            EpistemicStatus::Disproven
        );
    }

    #[test]
    fn test_policy_thresholds_are_configurable() {
        // "Sufficient evidence" is explicit policy, not hardcoded opinion.
        let c = Claim::new("x", ProvenanceSource::User);
        let es = vec![ev(EvidenceStatus::Verified)];
        let strict = EvidencePolicy {
            policy_id: "strict.v1".into(),
            version: "1".into(),
            proven_threshold: 3,
            disproven_threshold: 3,
            inferences_count: false,
        };
        assert_eq!(assess_claim(&c, &es, &strict), EpistemicStatus::Unknown);
        let lax = EvidencePolicy {
            policy_id: "lax.v1".into(),
            version: "1".into(),
            proven_threshold: 1,
            disproven_threshold: 1,
            inferences_count: false,
        };
        assert_eq!(assess_claim(&c, &es, &lax), EpistemicStatus::Proven);
    }

    #[test]
    fn test_absence_of_evidence_is_not_disproof() {
        // NO SUPPORTING EVIDENCE -> UNKNOWN, never DISPROVEN.
        let c = Claim::new("x", ProvenanceSource::User);
        assert_eq!(
            assess_claim(&c, &[], &EvidencePolicy::default()),
            EpistemicStatus::Unknown
        );
    }

    #[test]
    fn test_inference_does_not_count_as_support() {
        let c = Claim::new("x", ProvenanceSource::GeneratedHypothesis);
        let mut e1 = ev(EvidenceStatus::Verified);
        e1.kind = EvidenceKind::Inference;
        let mut e2 = ev(EvidenceStatus::Verified);
        e2.kind = EvidenceKind::Inference;
        // Inferences (hypotheses) cannot promote to PROVEN.
        assert_eq!(
            assess_claim(&c, &[e1, e2], &EvidencePolicy::default()),
            EpistemicStatus::Unknown
        );
    }
}

#[cfg(test)]
mod corner_tests {
    use super::*;

    fn ev(status: EvidenceStatus) -> Evidence {
        Evidence {
            id: uuid::Uuid::new_v4().to_string(),
            source: "test".into(),
            text: "test".into(),
            kind: EvidenceKind::Report,
            status,
            hash: "abc".into(),
            fixture: true,
        }
    }

    #[test]
    fn test_assess_claim_tie_stays_unknown() {
        // Equal verified support and contradiction: the oracle cannot
        // decide, so it must NOT flip either way.
        let c = Claim::new("x", ProvenanceSource::User);
        let es = vec![
            ev(EvidenceStatus::Verified),
            ev(EvidenceStatus::Verified),
            ev(EvidenceStatus::Contradicted),
            ev(EvidenceStatus::Contradicted),
        ];
        assert_eq!(
            assess_claim(&c, &es, &EvidencePolicy::default()),
            EpistemicStatus::Unknown
        );
    }

    #[test]
    fn test_assess_claim_majority_wins() {
        let c = Claim::new("x", ProvenanceSource::User);
        let prov = vec![
            ev(EvidenceStatus::Verified),
            ev(EvidenceStatus::Verified),
            ev(EvidenceStatus::Verified),
            ev(EvidenceStatus::Contradicted),
            ev(EvidenceStatus::Contradicted),
        ];
        assert_eq!(
            assess_claim(&c, &prov, &EvidencePolicy::default()),
            EpistemicStatus::Proven
        );
        let dis = vec![
            ev(EvidenceStatus::Verified),
            ev(EvidenceStatus::Verified),
            ev(EvidenceStatus::Contradicted),
            ev(EvidenceStatus::Contradicted),
            ev(EvidenceStatus::Contradicted),
        ];
        assert_eq!(
            assess_claim(&c, &dis, &EvidencePolicy::default()),
            EpistemicStatus::Disproven
        );
    }

    #[test]
    fn test_inferences_count_when_policy_allows() {
        // Default policy excludes inferences; an explicit policy may include them.
        let c = Claim::new("x", ProvenanceSource::GeneratedHypothesis);
        let mk = || {
            let mut e = ev(EvidenceStatus::Verified);
            e.kind = EvidenceKind::Inference;
            e
        };
        let es = vec![mk(), mk()];
        assert_eq!(
            assess_claim(&c, &es, &EvidencePolicy::default()),
            EpistemicStatus::Unknown
        );
        let permissive = EvidencePolicy {
            inferences_count: true,
            ..EvidencePolicy::default()
        };
        assert_eq!(assess_claim(&c, &es, &permissive), EpistemicStatus::Proven);
    }

    #[test]
    fn test_session_with_id_preserves_id_and_starts_empty() {
        let s = Session::with_id("client-123".to_string());
        assert_eq!(s.id, "client-123");
        assert!(s.claims.is_empty());
        assert!(s.evidence.is_empty());
        assert_eq!(s.turns, 0);
    }

    #[test]
    fn test_claim_new_with_empty_text_stays_unknown() {
        // Domain does not police content; the transport layer validates.
        let c = Claim::new("", ProvenanceSource::User);
        assert_eq!(
            assess_claim(&c, &[], &EvidencePolicy::default()),
            EpistemicStatus::Unknown
        );
    }
}
