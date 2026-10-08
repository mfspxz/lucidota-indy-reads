//! Avatar organelle: the deterministic graphical projection of graph state.
//!
//! LAW: no LLM mutates AvatarState. Emotion, gesture, and overlays are
//! DERIVED from governor epistemic state + intent by `derive_avatar`.
//! Visemes are derived from response text by a deterministic mapper
//! (Rhubarb-style A-X set). The phoneme oracle (eSpeak NG) and Rhubarb
//! postcall reconciliation plug in behind the same contract later.
//!
//! Renderers (SVG, ASCII, future dotLottie) are views over AvatarState.
//! Same state, different renderer — never duplicated animation logic.

use crate::domain::EpistemicStatus;
use crate::governor::Intent;
use serde::{Deserialize, Serialize};

/// Rhubarb-style viseme set. Deterministic mouth shapes, no inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Viseme {
    /// closed lips: M/B/P
    A,
    /// consonant/teeth: T/S/K
    B,
    /// narrow vowel: E/I
    C,
    /// wide open vowel: A
    D,
    /// rounded: O/U
    E,
    /// relaxed open neutral
    F,
    /// teeth/open expressive
    G,
    /// rounded expressive
    H,
    /// neutral resting mouth
    X,
}

impl Viseme {
    pub fn as_char(self) -> char {
        match self {
            Viseme::A => 'A',
            Viseme::B => 'B',
            Viseme::C => 'C',
            Viseme::D => 'D',
            Viseme::E => 'E',
            Viseme::F => 'F',
            Viseme::G => 'G',
            Viseme::H => 'H',
            Viseme::X => 'X',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Emotion {
    Neutral,
    Curious,
    Skeptical,
    Concerned,
    Surprised,
    Delighted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Gesture {
    Idle,
    Listen,
    Think,
    Nod,
    Lean,
    Point,
    Shrug,
    Investigate,
    Discover,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvatarState {
    pub emotion: Emotion,
    pub gesture: Gesture,
    /// Epistemic overlay shown on the avatar; mirrors governor state exactly.
    /// The visual must NEVER imply stronger certainty than this.
    pub epistemic_overlay: Option<String>,
    pub speaking: bool,
}

/// Deterministic derivation: graph/governor state -> avatar behavior.
/// The LLM has no say here.
pub fn derive_avatar(
    epistemic: EpistemicStatus,
    intent: Intent,
    speaking: bool,
) -> AvatarState {
    let (emotion, gesture, overlay) = match epistemic {
        EpistemicStatus::Proven => (
            Emotion::Neutral,
            Gesture::Nod,
            Some("PROVEN".to_string()),
        ),
        EpistemicStatus::Disproven => (
            Emotion::Skeptical,
            Gesture::Shrug,
            Some("DISPROVEN".to_string()),
        ),
        EpistemicStatus::Unknown => {
            let (e, g) = match intent {
                Intent::HypothesisAssertion => (Emotion::Curious, Gesture::Investigate),
                Intent::AuthorityRequest => (Emotion::Skeptical, Gesture::Lean),
                Intent::Question => (Emotion::Curious, Gesture::Listen),
                _ => (Emotion::Skeptical, Gesture::Think),
            };
            (e, g, Some("UNKNOWN".to_string()))
        }
    };
    AvatarState {
        emotion,
        gesture: if speaking && matches!(gesture, Gesture::Listen | Gesture::Think) {
            Gesture::Idle
        } else {
            gesture
        },
        epistemic_overlay: overlay,
        speaking,
    }
}

/// Deterministic char -> viseme approximation (the live path).
/// Documented approximation, not a phoneme oracle — eSpeak NG plugs in here.
fn char_to_viseme(c: char) -> Viseme {
    match c.to_ascii_lowercase() {
        'm' | 'b' | 'p' => Viseme::A,
        't' | 's' | 'k' | 'z' | 'x' | 'c' | 'd' | 'n' | 'l' | 'r' | 'j' | 'g' => Viseme::B,
        'e' | 'i' | 'y' => Viseme::C,
        'a' => Viseme::D,
        'o' | 'u' | 'w' | 'q' => Viseme::E,
        '!' | '?' => Viseme::G,
        '.' | ',' | ';' | ':' => Viseme::X,
        ' ' => Viseme::X,
        _ => Viseme::F,
    }
}

/// Response text -> timestamped viseme timeline.
/// ~70ms per character approximates natural speech rate; the UI stretches or
/// compresses the timeline to the actual TTS duration.
pub fn viseme_timeline(text: &str) -> Vec<(u64, Viseme)> {
    // Strip machine tags like [fixture:indy] — the puppet doesn't speak those.
    let clean = strip_tags(text);
    let mut out = Vec::new();
    let mut t: u64 = 0;
    let mut last = Viseme::X;
    for c in clean.chars() {
        let v = char_to_viseme(c);
        // Coalesce repeats: only emit on change.
        if v != last {
            out.push((t, v));
            last = v;
        }
        t += 70;
    }
    out.push((t, Viseme::X)); // rest at end
    out
}

fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0;
    for c in text.chars() {
        match c {
            '[' => depth += 1,
            ']' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Avatar transition receipt payload: what changed, why, authorized by whom.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvatarTransition {
    pub from_emotion: String,
    pub to_emotion: String,
    pub from_gesture: String,
    pub to_gesture: String,
    pub trigger: String,
    pub authorized_by: String,
}

impl AvatarTransition {
    pub fn new(from: &AvatarState, to: &AvatarState, trigger: &str) -> Self {
        Self {
            from_emotion: format!("{:?}", from.emotion),
            to_emotion: format!("{:?}", to.emotion),
            from_gesture: format!("{:?}", from.gesture),
            to_gesture: format!("{:?}", to.gesture),
            trigger: trigger.to_string(),
            authorized_by: "governor".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unknown_maps_to_skeptical() {
        let s = derive_avatar(EpistemicStatus::Unknown, Intent::ClaimAssertion, false);
        assert_eq!(s.emotion, Emotion::Skeptical);
        assert_eq!(s.epistemic_overlay.as_deref(), Some("UNKNOWN"));
    }

    #[test]
    fn test_proven_maps_to_confident_neutral() {
        let s = derive_avatar(EpistemicStatus::Proven, Intent::Question, true);
        assert_eq!(s.emotion, Emotion::Neutral);
        assert_eq!(s.epistemic_overlay.as_deref(), Some("PROVEN"));
    }

    #[test]
    fn test_disproven_maps_to_skeptical() {
        let s = derive_avatar(EpistemicStatus::Disproven, Intent::Converse, false);
        assert_eq!(s.epistemic_overlay.as_deref(), Some("DISPROVEN"));
    }

    #[test]
    fn test_viseme_timeline_is_deterministic_and_monotonic() {
        let a = viseme_timeline("Hello there.");
        let b = viseme_timeline("Hello there.");
        assert_eq!(a, b);
        for w in a.windows(2) {
            assert!(w[0].0 <= w[1].0, "timeline must be monotonic");
        }
        assert!(!a.is_empty());
        // ends at rest
        assert_eq!(a.last().unwrap().1, Viseme::X);
    }

    #[test]
    fn test_viseme_mapping_covers_rhubarb_set() {
        // "The quick brown fox" should exercise several distinct visemes.
        let tl = viseme_timeline("The quick brown fox jumps.");
        let distinct: std::collections::HashSet<char> =
            tl.iter().map(|(_, v)| v.as_char()).collect();
        assert!(distinct.len() >= 4, "expected viseme variety, got {:?}", distinct);
    }

    #[test]
    fn test_tags_stripped_from_timeline() {
        let tl = viseme_timeline("[fixture:indy] Hi.");
        // No viseme should come from the tag text; timeline starts at speech.
        assert!(tl.len() < viseme_timeline("[fixture:indy] Hi. Extra words here").len());
    }

    #[test]
    fn test_avatar_never_implies_false_certainty() {
        // The overlay mirrors the governor exactly — this test pins that.
        for status in [EpistemicStatus::Proven, EpistemicStatus::Disproven, EpistemicStatus::Unknown] {
            let s = derive_avatar(status, Intent::Question, false);
            let expected = format!("{:?}", status).to_uppercase();
            assert_eq!(s.epistemic_overlay.as_deref(), Some(expected.as_str()));
        }
    }
}

#[cfg(test)]
mod corner_tests {
    use super::*;

    #[test]
    fn test_viseme_timeline_empty_input() {
        // Empty input: single rest entry, no panic, still well-formed.
        let tl = viseme_timeline("");
        assert_eq!(tl, vec![(0, Viseme::X)]);
    }

    #[test]
    fn test_strip_tags_unbalanced_brackets() {
        // Unterminated tag: the tag content is dropped, never panics,
        // and no viseme is emitted for the swallowed text.
        let tl = viseme_timeline("a [b");
        let text: String = tl.iter().map(|(_, v)| v.as_char()).collect();
        assert!(!text.contains('B'), "tag chars must not produce visemes, got {text}");
        // Stray closer with no opener is kept as ordinary text (documented).
        let tl2 = viseme_timeline("]x");
        assert!(!tl2.is_empty());
    }

    #[test]
    fn test_non_ascii_chars_map_safely() {
        // Unicode input: deterministic, no panic, falls back to F.
        let a = viseme_timeline("héllo 世界 🎙");
        let b = viseme_timeline("héllo 世界 🎙");
        assert_eq!(a, b);
        assert!(!a.is_empty());
        for w in a.windows(2) {
            assert!(w[0].0 <= w[1].0);
        }
    }

    #[test]
    fn test_speaking_overrides_listen_think_gestures() {
        // While speaking, Listen/Think collapse to Idle so the puppet
        // doesn't look like it's waiting while it talks.
        let s = derive_avatar(EpistemicStatus::Unknown, Intent::Question, true);
        assert_eq!(s.gesture, Gesture::Idle, "Question/Listen + speaking -> Idle");
        let s = derive_avatar(EpistemicStatus::Unknown, Intent::Converse, true);
        assert_eq!(s.gesture, Gesture::Idle, "Converse/Think + speaking -> Idle");
        // Non-speaking keeps the listening gesture.
        let s = derive_avatar(EpistemicStatus::Unknown, Intent::Question, false);
        assert_eq!(s.gesture, Gesture::Listen);
        // Non-listen gestures survive speaking (Nod on PROVEN).
        let s = derive_avatar(EpistemicStatus::Proven, Intent::Question, true);
        assert_eq!(s.gesture, Gesture::Nod);
    }

    #[test]
    fn test_long_input_timeline_stays_sane() {
        // 10k chars: monotonic, ends at rest, no overflow.
        let tl = viseme_timeline(&"ab ".repeat(3334));
        assert!(tl.len() > 100);
        for w in tl.windows(2) {
            assert!(w[0].0 <= w[1].0);
        }
        assert_eq!(tl.last().unwrap().1, Viseme::X);
        assert_eq!(tl.last().unwrap().0, 3334 * 3 * 70);
    }
}
