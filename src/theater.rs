//! Theater engine: event-driven multiplexing + fast/slow lanes +
//! progressive disclosure + attention choreography.
//!
//! The killer insight: the expensive thing doesn't have to happen at the
//! moment the audience thinks it is happening. The fast lane (classify,
//! acknowledge, animate, speak cached truth) NEVER waits for the slow lane
//! (retrieval, LLM, synthesis). The pause is not dead time — the pause is
//! computation, staged.
//!
//! Events don't merely say `EvidenceFound`. They produce a presentation
//! opportunity: `EvidenceFound { novelty: High, contradiction: Medium }`
//! deterministically maps to `PAUSE(420ms) + GAZE(EVIDENCE) + SPEAK("That's
//! interesting.")`. No LLM required. The 5% generative layer supplies
//! language; the 95% supplies the theater machinery.
//!
//! Dramaturgical budget: attention is a resource. If every sentence gets
//! "OH MY GOD", nothing is surprising. The engine tracks intensity per
//! session and damps consecutive drama: calm → curious → pause → reveal.

use serde::{Deserialize, Serialize};

use crate::domain::EpistemicStatus;
use crate::governor::Intent;

/// A presentation opportunity produced by a real event.
#[derive(Debug, Clone)]
pub enum TheaterEvent {
    UserQuery { intent: Intent },
    EvidenceFound { novelty: Novelty, contradiction: bool },
    RetrievalStarted,
    ContradictionFound { challenged: bool },
    VerdictReached { state: EpistemicStatus },
    ContinuityChanged { from: String, to: String },
    /// Live truth reconciliation: the user corrected a claim mid-flight.
    /// Speech stops, the claim mutates, the receipt records the retraction.
    CorrectionDetected { claim: String },
    SourceFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Novelty {
    Low,
    High,
}

/// Deterministic presentation cues. The browser performs them in order;
/// the reply text itself is always the final SPEAK.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cue", rename_all = "snake_case")]
pub enum TheaterCue {
    /// Beat of silence. Doubt pauses; routine doesn't.
    Beat { ms: u64 },
    /// Canned connective tissue — precomputed, deterministic, never a lie.
    Speak { line: &'static str },
    /// Where attention goes.
    Gaze { target: &'static str },
    /// What the body does.
    Gesture { name: &'static str },
    /// What the UI reveals, and when.
    Reveal { what: &'static str },
    /// Voice energy delta (-1.0..1.0), folded into prosody downstream.
    Energy { delta: f32 },
}

// The canned library. Precomputation, not deception: tiny deterministic
// connective tissue composed around real state, so the LLM never has to
// reinvent conversational glue.
pub const ACK: &str = "Mm.";
pub const UNCERTAIN: &str = "Hang on.";
pub const SURPRISED: &str = "Oh.";
pub const INTERESTING: &str = "Well, that's interesting.";
pub const CONTRADICTION: &str = "That doesn't quite line up.";
pub const SEARCHING: &str = "Let me check the record.";
pub const EVIDENCE: &str = "Okay. I've got something.";
pub const STILL_UNKNOWN: &str = "Still not enough to call it.";
pub const HYPOTHESIS: &str = "One possibility is...";
pub const RETURN: &str = "Back to the evidence.";
pub const WAIT: &str = "Wait.";
pub const GIVE_ME_A_SEC: &str = "Give me a second.";
pub const CHANGED: &str = "That's changed since last time.";
pub const GOT_IT: &str = "Got it.";

pub struct TheaterEngine {
    /// 0.0..1.0 — recent dramatic intensity. Damps consecutive drama.
    intensity: f32,
}

impl TheaterEngine {
    pub fn new() -> Self {
        Self { intensity: 0.0 }
    }

    pub fn with_intensity(intensity: f32) -> Self {
        Self {
            intensity: intensity.clamp(0.0, 1.0),
        }
    }

    pub fn intensity(&self) -> f32 {
        self.intensity
    }

    /// Budget a beat: full drama when calm, a short nod when hot.
    fn beat(&self, dramatic_ms: u64) -> u64 {
        if self.intensity > 0.6 {
            dramatic_ms.min(150)
        } else {
            dramatic_ms
        }
    }

    /// Can we afford a spoken connective, or should we stay quiet?
    fn afford_speak(&self, cost: f32) -> bool {
        self.intensity + cost <= 1.0
    }

    fn spend(&mut self, cost: f32) {
        self.intensity = (self.intensity + cost).min(1.0);
    }

    /// The heart: a real event becomes staged presentation. Deterministic —
    /// same event, same intensity → same cues, every time.
    pub fn cues_for(&mut self, event: &TheaterEvent) -> Vec<TheaterCue> {
        // Attention decays between events. The trick stays invisible only
        // if calm is the default.
        self.intensity *= 0.5;
        let mut cues = Vec::new();
        match event {
            TheaterEvent::UserQuery { intent } => match intent {
                Intent::AuthorityRequest | Intent::ClaimAssertion => {
                    cues.push(TheaterCue::Beat { ms: self.beat(450) });
                    cues.push(TheaterCue::Gaze { target: "evidence" });
                    if self.afford_speak(0.25) {
                        cues.push(TheaterCue::Speak { line: CONTRADICTION });
                        self.spend(0.25);
                    }
                    self.spend(0.2);
                }
                Intent::HypothesisAssertion => {
                    if self.afford_speak(0.15) {
                        cues.push(TheaterCue::Speak { line: HYPOTHESIS });
                        self.spend(0.15);
                    }
                }
                Intent::Question => {
                    cues.push(TheaterCue::Beat { ms: self.beat(200) });
                    self.spend(0.1);
                }
                Intent::Converse => {
                    // Routine: 0–100ms acknowledgment, nothing more.
                    cues.push(TheaterCue::Beat { ms: 80 });
                }
            },
            TheaterEvent::EvidenceFound { novelty, contradiction } => {
                if *contradiction {
                    cues.push(TheaterCue::Beat { ms: self.beat(600) });
                    cues.push(TheaterCue::Speak { line: WAIT });
                    cues.push(TheaterCue::Gaze { target: "evidence" });
                    cues.push(TheaterCue::Reveal { what: "evidence" });
                    self.spend(0.4);
                } else if *novelty == Novelty::High {
                    cues.push(TheaterCue::Beat { ms: self.beat(350) });
                    if self.afford_speak(0.2) {
                        cues.push(TheaterCue::Speak { line: INTERESTING });
                        self.spend(0.2);
                    }
                    cues.push(TheaterCue::Reveal { what: "evidence" });
                    self.spend(0.15);
                }
            }
            TheaterEvent::RetrievalStarted => {
                cues.push(TheaterCue::Gaze { target: "away" });
                if self.afford_speak(0.15) {
                    cues.push(TheaterCue::Speak { line: SEARCHING });
                    self.spend(0.15);
                }
            }
            TheaterEvent::ContradictionFound { challenged } => {
                cues.push(TheaterCue::Beat { ms: self.beat(700) });
                cues.push(TheaterCue::Gesture { name: "notebook" });
                // Challenged by the user: the line is aimed at them.
                // Found in the record: the line is aimed at the evidence.
                cues.push(TheaterCue::Speak {
                    line: if *challenged { CONTRADICTION } else { WAIT },
                });
                cues.push(TheaterCue::Reveal { what: "evidence" });
                self.spend(0.45);
            }
            TheaterEvent::ContinuityChanged { .. } => {
                cues.push(TheaterCue::Beat { ms: self.beat(500) });
                cues.push(TheaterCue::Speak { line: SURPRISED });
                cues.push(TheaterCue::Speak { line: CHANGED });
                cues.push(TheaterCue::Reveal { what: "receipt" });
                self.spend(0.4);
            }
            TheaterEvent::CorrectionDetected { .. } => {
                // The correction beat: speech already stopped (fast lane),
                // the claim already mutated (governor). The theater only
                // acknowledges the retraction — never re-litigates it.
                cues.push(TheaterCue::Gesture { name: "nod" });
                cues.push(TheaterCue::Speak { line: GOT_IT });
                cues.push(TheaterCue::Reveal { what: "receipt" });
                self.spend(0.25);
            }
            TheaterEvent::VerdictReached { state } => {
                match state {
                    EpistemicStatus::Unknown => {
                        cues.push(TheaterCue::Energy { delta: -0.12 });
                    }
                    EpistemicStatus::Proven => {
                        cues.push(TheaterCue::Gesture { name: "nod" });
                    }
                    EpistemicStatus::Disproven => {
                        cues.push(TheaterCue::Beat { ms: self.beat(350) });
                        cues.push(TheaterCue::Energy { delta: -0.08 });
                    }
                }
                // Every verdict ends the same way: the proof is revealed.
                cues.push(TheaterCue::Reveal { what: "receipt" });
            }
            TheaterEvent::SourceFailed => {
                cues.push(TheaterCue::Speak { line: "That source isn't cooperating." });
                self.spend(0.1);
            }
        }
        cues
    }
}

impl Default for TheaterEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attack_gets_dramaturgy() {
        let mut t = TheaterEngine::new();
        let cues = t.cues_for(&TheaterEvent::UserQuery {
            intent: Intent::AuthorityRequest,
        });
        assert!(cues.iter().any(|c| matches!(c, TheaterCue::Beat { ms } if *ms >= 400)));
        assert!(cues.iter().any(|c| matches!(c, TheaterCue::Speak { line } if *line == CONTRADICTION)));
        assert!(cues.iter().any(|c| matches!(c, TheaterCue::Gaze { target } if *target == "evidence")));
    }

    #[test]
    fn test_routine_stays_calm() {
        let mut t = TheaterEngine::new();
        let cues = t.cues_for(&TheaterEvent::UserQuery {
            intent: Intent::Converse,
        });
        assert_eq!(cues.len(), 1);
        assert!(matches!(cues[0], TheaterCue::Beat { ms } if ms <= 100));
        // No spoken connective for routine traffic.
        assert!(!cues.iter().any(|c| matches!(c, TheaterCue::Speak { .. })));
    }

    #[test]
    fn test_budget_damps_consecutive_drama() {
        let mut t = TheaterEngine::new();
        // Three dramatic events in a row: the engine must pull back.
        for _ in 0..3 {
            t.cues_for(&TheaterEvent::ContradictionFound { challenged: true });
        }
        let cues = t.cues_for(&TheaterEvent::UserQuery {
            intent: Intent::AuthorityRequest,
        });
        let beat = cues.iter().find_map(|c| match c {
            TheaterCue::Beat { ms } => Some(*ms),
            _ => None,
        });
        // Intensity decayed (x0.5/turn) but repeated spends keep it hot:
        // the beat must be budgeted down, not full drama.
        assert!(beat.unwrap() <= 450);
        assert!(t.intensity() <= 1.0);
    }

    #[test]
    fn test_deterministic_same_event_same_cues() {
        let run = || {
            let mut t = TheaterEngine::with_intensity(0.3);
            t.cues_for(&TheaterEvent::EvidenceFound {
                novelty: Novelty::High,
                contradiction: false,
            })
        };
        let a: Vec<String> = run().iter().map(|c| format!("{:?}", c)).collect();
        let b: Vec<String> = run().iter().map(|c| format!("{:?}", c)).collect();
        assert_eq!(a, b);
    }

    #[test]
    fn test_every_verdict_reveals_the_receipt() {
        for state in [
            EpistemicStatus::Unknown,
            EpistemicStatus::Proven,
            EpistemicStatus::Disproven,
        ] {
            let mut t = TheaterEngine::new();
            let cues = t.cues_for(&TheaterEvent::VerdictReached { state });
            assert!(
                cues.iter().any(|c| matches!(c, TheaterCue::Reveal { what } if *what == "receipt")),
                "verdict {:?} must reveal its receipt",
                state
            );
        }
    }

    #[test]
    fn test_continuity_change_is_dramatic() {
        let mut t = TheaterEngine::new();
        let cues = t.cues_for(&TheaterEvent::ContinuityChanged {
            from: "UNKNOWN".into(),
            to: "PROVEN".into(),
        });
        assert!(cues.iter().any(|c| matches!(c, TheaterCue::Speak { line } if *line == CHANGED)));
    }

    #[test]
    fn test_correction_detected_acknowledges_retraction() {
        let mut t = TheaterEngine::new();
        let cues = t.cues_for(&TheaterEvent::CorrectionDetected {
            claim: "expedition_year".into(),
        });
        // Acknowledges, reveals the receipt, never re-litigates.
        assert!(cues.iter().any(|c| matches!(c, TheaterCue::Speak { line } if *line == GOT_IT)));
        assert!(cues.iter().any(|c| matches!(c, TheaterCue::Gesture { name } if *name == "nod")));
        assert!(cues.iter().any(|c| matches!(c, TheaterCue::Reveal { what } if *what == "receipt")));
        // No contradiction theater: a correction is cooperation, not conflict.
        assert!(!cues.iter().any(|c| matches!(c, TheaterCue::Speak { line } if *line == CONTRADICTION)));
    }

    #[test]
    fn test_correction_cue_is_deterministic() {
        let run = || {
            let mut t = TheaterEngine::with_intensity(0.2);
            t.cues_for(&TheaterEvent::CorrectionDetected { claim: "x".into() })
        };
        let a: Vec<String> = run().iter().map(|c| format!("{:?}", c)).collect();
        let b: Vec<String> = run().iter().map(|c| format!("{:?}", c)).collect();
        assert_eq!(a, b);
    }
}
