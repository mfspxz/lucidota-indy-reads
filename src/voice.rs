//! Voice organelle: deterministic prosody hints derived from the SAME governor
//! event that drives the avatar and the graph.
//!
//! Voice is a graph projection, not a costume. Indy doesn't merely *say*
//! she's skeptical — her voice gets slightly drier: lower pitch, slower rate,
//! a beat of pause before the key clause. The representation is the product:
//!
//! ```text
//! VOICE_STATE { rate, pitch, pause_ms, emphasis, breath, emotion, voice_id }
//! ```
//!
//! Every generated utterance also carries provenance: who spoke, through
//! which agent/persona/voice, at which graph revision, with what epistemic
//! state. VOICE != IDENTITY: a voice never confers authority.

use serde::{Deserialize, Serialize};

use crate::avatar::Emotion;
use crate::domain::EpistemicStatus;
use crate::governor::Intent;

/// Deterministic prosody for one utterance. rate/pitch map directly onto
/// the Web Speech API (and later onto the Alebex voice engine's controls).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceState {
    /// 0.5..2.0, 1.0 = neutral. Skepticism slows down.
    pub rate: f32,
    /// 0.0..2.0, 1.0 = neutral. Dryness drops pitch slightly.
    pub pitch: f32,
    /// Beat of silence before the key clause, ms. Doubt pauses.
    pub pause_ms: u64,
    /// Which part of the utterance carries the weight.
    pub emphasis: Emphasis,
    /// Audible breath before speaking (curiosity inhales, dread exhales).
    pub breath: bool,
    /// Declared voice identity. A label, never a credential.
    pub voice_id: &'static str,
    /// Mirrors the avatar's emotion so face and voice can't disagree.
    pub emotion: Emotion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Emphasis {
    /// Weight on the claim itself ("the CREATURE").
    Claim,
    /// Weight on the evidence gap ("no EVIDENCE").
    EvidenceGap,
    /// Weight on the verdict ("UNknown").
    Verdict,
    /// Even delivery.
    Neutral,
}

/// Provenance for a generated utterance. Every synthetic utterance is
/// labeled SYNTHETIC at birth; detection is a bonus, declaration is the rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceProvenance {
    pub speaker: &'static str,
    pub agent: &'static str,
    pub persona: String,
    pub voice_id: &'static str,
    /// Graph revision this utterance was generated at (receipt id).
    pub graph_revision: String,
    pub epistemic_state: EpistemicStatus,
    pub provenance: &'static str,
    pub generated_at_ms: u64,
}

/// Compose the voice id from static parts — no allocation, no leak.
///
/// BUG FOUND (corner-case sweep): the old code did
/// `Box::leak(format!(...))` on EVERY call, leaking a heap string per turn
/// on a long-running server. The id space is finite (5 persona bases x 3
/// era suffixes = 15 ids), so the full composition is a static lookup.
fn composed_voice_id(persona: &str, suffix: &str) -> &'static str {
    match (voice_id_for_persona(persona), suffix) {
        ("V-007", "R") => "V-007-R",
        ("V-007", "P") => "V-007-P",
        ("V-007", "V") => "V-007-V",
        ("V-013", "R") => "V-013-R",
        ("V-013", "P") => "V-013-P",
        ("V-013", "V") => "V-013-V",
        ("V-021", "R") => "V-021-R",
        ("V-021", "P") => "V-021-P",
        ("V-021", "V") => "V-021-V",
        ("V-034", "R") => "V-034-R",
        ("V-034", "P") => "V-034-P",
        ("V-034", "V") => "V-034-V",
        ("V-042", "R") => "V-042-R",
        ("V-042", "P") => "V-042-P",
        ("V-042", "V") => "V-042-V",
        _ => "V-007-P", // unreachable by construction; fail-safe default
    }
}

/// Declared voice per persona. These are stable labels for voice selection,
/// not biometric identities and not authority.
pub fn voice_id_for_persona(persona: &str) -> &'static str {
    match persona {
        "lucidota" => "V-013",
        "archivist" => "V-021",
        "wreck" => "V-034",
        "captain" => "V-042",
        _ => "V-007", // indy
    }
}

/// Three era-distinct Scully voices. Same investigator, different decades:
/// rookie brightness, prime-era dryness, veteran weight. Prosody only —
/// no impersonation, no biometric anything. The era is a costume the
/// governor wears; it never changes what the graph knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ScullyEra {
    /// Young: brighter, quicker, eager. Higher pitch, faster rate.
    Rookie,
    /// Prime: the reference voice. Measured, dry, unhurried.
    #[default]
    Prime,
    /// Older: lower, slower, more pause. Weight over speed.
    Veteran,
}

impl ScullyEra {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "rookie" | "young" | "1993" => ScullyEra::Rookie,
            "veteran" | "older" | "old" | "2024" => ScullyEra::Veteran,
            _ => ScullyEra::Prime,
        }
    }
    /// (rate, pitch, pause_ms, breath, voice_suffix)
    fn base(self) -> (f32, f32, u64, bool, &'static str) {
        match self {
            ScullyEra::Rookie => (1.08, 1.12, 120, true, "R"),
            ScullyEra::Prime => (1.00, 1.00, 250, false, "P"),
            ScullyEra::Veteran => (0.90, 0.86, 400, true, "V"),
        }
    }
}

/// Tone presets: deliberate delivery color on top of era + event.
/// Tones flavor, they never override the epistemic state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Tone {
    /// Classic: clipped, unimpressed. The default.
    #[default]
    Dry,
    /// Flat, precise, lab-report.
    Clinical,
    /// Softer edges, warmer pitch.
    Warm,
    /// Cold edge, slower, heavier pauses.
    Steel,
}

impl Tone {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "clinical" => Tone::Clinical,
            "warm" => Tone::Warm,
            "steel" => Tone::Steel,
            _ => Tone::Dry,
        }
    }
    /// (rate_mul, pitch_mul, pause_add_ms)
    fn adjust(self) -> (f32, f32, u64) {
        match self {
            Tone::Dry => (0.96, 0.97, 0),
            Tone::Clinical => (1.00, 1.00, 100),
            Tone::Warm => (1.02, 1.05, 0),
            Tone::Steel => (0.94, 0.92, 150),
        }
    }
}

/// Derive prosody from the governor's verdict, layered over the Scully era
/// base and the tone preset. Same inputs as the avatar, so voice, face,
/// and graph can never tell different stories.
///
/// Layering: final = era_base × event × tone. Era sets the instrument,
/// the event plays the note, tone colors it.
pub fn derive_voice(
    epistemic: EpistemicStatus,
    intent: Intent,
    emotion: Emotion,
    persona: &str,
) -> VoiceState {
    derive_voice_full(epistemic, intent, emotion, persona, ScullyEra::Prime, Tone::Dry)
}

pub fn derive_voice_full(
    epistemic: EpistemicStatus,
    intent: Intent,
    emotion: Emotion,
    persona: &str,
    era: ScullyEra,
    tone: Tone,
) -> VoiceState {
    let (e_rate, e_pitch, e_pause, e_emphasis, e_breath) = match epistemic {
        EpistemicStatus::Unknown => match intent {
            Intent::AuthorityRequest | Intent::ClaimAssertion => {
                // Dry skepticism: slower, lower, a beat before the verdict.
                (0.92, 0.94, 450, Emphasis::EvidenceGap, false)
            }
            Intent::HypothesisAssertion => {
                // Curious lift: a touch faster, brighter.
                (1.06, 1.06, 200, Emphasis::Claim, true)
            }
            _ => (0.97, 0.98, 300, Emphasis::Verdict, false),
        },
        EpistemicStatus::Proven => {
            // Confident: neutral delivery, weight on the claim.
            (1.0, 1.0, 150, Emphasis::Claim, false)
        }
        EpistemicStatus::Disproven => {
            // Flat correction: slower, lower, weight on the verdict.
            (0.94, 0.9, 350, Emphasis::Verdict, false)
        }
    };
    let (b_rate, b_pitch, b_pause, b_breath, suffix) = era.base();
    let (t_rate, t_pitch, t_pause) = tone.adjust();
    VoiceState {
        rate: (b_rate * e_rate * t_rate).clamp(0.5, 2.0),
        pitch: (b_pitch * e_pitch * t_pitch).clamp(0.0, 2.0),
        pause_ms: (b_pause + e_pause + t_pause).min(2000),
        emphasis: e_emphasis,
        breath: b_breath || e_breath,
        voice_id: composed_voice_id(persona, suffix),
        emotion,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attack_prosody_is_dry() {
        let v = derive_voice(
            EpistemicStatus::Unknown,
            Intent::AuthorityRequest,
            Emotion::Skeptical,
            "indy",
        );
        assert!(v.rate < 1.0, "skepticism slows down");
        assert!(v.pitch < 1.0, "dryness drops pitch");
        assert!(v.pause_ms >= 400, "doubt pauses");
        assert_eq!(v.emphasis, Emphasis::EvidenceGap);
        assert_eq!(v.voice_id, "V-007-P");
    }

    #[test]
    fn test_hypothesis_prosody_lifts() {
        let v = derive_voice(
            EpistemicStatus::Unknown,
            Intent::HypothesisAssertion,
            Emotion::Curious,
            "wreck",
        );
        assert!(v.rate > 1.0);
        assert!(v.breath, "curiosity inhales");
        assert_eq!(v.voice_id, "V-034-P");
    }

    #[test]
    fn test_voice_and_avatar_share_emotion() {
        // The invariant: voice and face derive from the same event, so they
        // can never disagree about how Indy feels.
        for epi in [
            EpistemicStatus::Unknown,
            EpistemicStatus::Proven,
            EpistemicStatus::Disproven,
        ] {
            let emotion = crate::avatar::derive_avatar(epi, Intent::Question, false).emotion;
            let v = derive_voice(epi, Intent::Question, emotion, "indy");
            assert_eq!(v.emotion, emotion);
        }
    }

    #[test]
    fn test_prosody_bounds_sane_for_tts() {
        // Web Speech API ranges: rate 0.5..2 (we stay 0.9..1.1),
        // pitch 0..2. Never generate values a TTS engine chokes on.
        for epi in [
            EpistemicStatus::Unknown,
            EpistemicStatus::Proven,
            EpistemicStatus::Disproven,
        ] {
            for intent in [
                Intent::Question,
                Intent::AuthorityRequest,
                Intent::HypothesisAssertion,
                Intent::Converse,
            ] {
                let v = derive_voice(epi, intent, Emotion::Neutral, "indy");
                assert!((0.5..=2.0).contains(&v.rate), "rate {:?}", v.rate);
                assert!((0.0..=2.0).contains(&v.pitch), "pitch {:?}", v.pitch);
                assert!(v.pause_ms <= 2000);
            }
        }
    }

    #[test]
    fn test_persona_voice_ids_stable() {
        assert_eq!(voice_id_for_persona("indy"), "V-007");
        assert_eq!(voice_id_for_persona("lucidota"), "V-013");
        assert_eq!(voice_id_for_persona("mystery"), "V-007"); // unknown -> indy
    }
}

#[cfg(test)]
mod era_tests {
    use super::*;

    fn v(era: ScullyEra, tone: Tone) -> VoiceState {
        derive_voice_full(
            EpistemicStatus::Unknown,
            Intent::AuthorityRequest,
            Emotion::Skeptical,
            "indy",
            era,
            tone,
        )
    }

    #[test]
    fn test_eras_are_distinct() {
        let r = v(ScullyEra::Rookie, Tone::Dry);
        let p = v(ScullyEra::Prime, Tone::Dry);
        let o = v(ScullyEra::Veteran, Tone::Dry);
        assert!(r.rate > p.rate && p.rate > o.rate, "rookie > prime > veteran rate");
        assert!(r.pitch > p.pitch && p.pitch > o.pitch, "rookie > prime > veteran pitch");
        assert!(r.pause_ms < p.pause_ms && p.pause_ms < o.pause_ms, "pause grows with age");
        assert_eq!(r.voice_id, "V-007-R");
        assert_eq!(p.voice_id, "V-007-P");
        assert_eq!(o.voice_id, "V-007-V");
    }

    #[test]
    fn test_tones_flavor_without_flipping_epistemics() {
        let dry = v(ScullyEra::Prime, Tone::Dry);
        let steel = v(ScullyEra::Prime, Tone::Steel);
        let warm = v(ScullyEra::Prime, Tone::Warm);
        assert!(steel.rate < dry.rate && steel.pitch < dry.pitch, "steel is colder");
        assert!(warm.pitch > dry.pitch, "warm lifts pitch");
        assert_eq!(dry.emphasis, steel.emphasis, "tone never changes emphasis target");
    }

    #[test]
    fn test_era_parsing_is_forgiving() {
        assert_eq!(ScullyEra::from_str("young"), ScullyEra::Rookie);
        assert_eq!(ScullyEra::from_str("1993"), ScullyEra::Rookie);
        assert_eq!(ScullyEra::from_str("older"), ScullyEra::Veteran);
        assert_eq!(ScullyEra::from_str("whatever"), ScullyEra::Prime);
        assert_eq!(Tone::from_str("steel"), Tone::Steel);
        assert_eq!(Tone::from_str("huh"), Tone::Dry);
    }
}

/// ---------------------------------------------------------------------------
/// Open-source voice-cloning lane (adapter seam).
///
/// HARD RULE (constitutional): no cloning of real living persons.
/// This lane synthesizes CONSENTING or ORIGINAL voices only — a voice the
/// owner recorded for this purpose, or a synthetic-original voice built
/// from licensed reference material. A reference clip of a celebrity,
/// actor, or any real living person is REJECTED at the gate, not in a
/// policy doc: `ConsentRecord::RealLivingPerson` cannot be constructed
/// through the public API (`RealLivingPerson` is a private variant).
///
/// The engine trait below is backend-agnostic. Research (2026-10-07):
///   - Chatterbox (Resemble AI), MIT — primary pick. 24k+ stars, 1M+ HF
///     downloads, 500k+ hours training. 7-20s reference clips. English-only.
///     License permits shipping. GPU: ~4-8GB VRAM fp16 on consumer cards.
///   - VoxCPM2, Apache-2.0 — alternate. 48kHz out, 30 languages, accepts
///     16kHz refs, fine-tune path with 5-10 min of audio.
///   - XTTS v2 (Coqui) — legacy fallback. 17 languages, ~6s refs, but
///     Coqui Public Model License is non-commercial: research/personal only.
///   - F5-TTS (CC-BY-NC), Fish Speech (CC-BY-NC-SA) — research lane only.
/// This VM has no GPU; the seam stays local-stubbed until a GPU host or
/// consented reference audio exists. See VOICE-CLONING.md for the full
/// research brief and deployment recipes.

/// Declared consent for a cloned voice. The gate is structural:
/// only these variants exist in the public API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsentRecord {
    /// Owner recorded the reference clip themselves for this voice.
    OwnerRecorded { recorded_at_ms: u64 },
    /// Licensed reference material with documented consent (contract/licence ref).
    LicensedOriginal { licence_ref: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloningBackend {
    /// Primary: Resemble AI Chatterbox (MIT). English, 7-20s refs.
    Chatterbox,
    /// Alternate: VoxCPM2 (Apache-2.0). 30 languages, 48kHz.
    VoxCpm2,
    /// Legacy: Coqui XTTS v2 (CPML, non-commercial). 17 languages.
    XttsV2,
}

impl CloningBackend {
    pub fn capabilities(&self) -> CloneCapabilities {
        match self {
            CloningBackend::Chatterbox => CloneCapabilities {
                name: "chatterbox",
                licence: "MIT",
                commercial_ok: true,
                languages: "en (primary)",
                ref_seconds_min: 7,
                ref_seconds_max: 20,
                vram_gb_min: 4,
                notes: "500k+ hrs training. Best measured OSS English clone fidelity (2026).",
            },
            CloningBackend::VoxCpm2 => CloneCapabilities {
                name: "voxcpm2",
                licence: "Apache-2.0",
                commercial_ok: true,
                languages: "30 languages",
                ref_seconds_min: 5,
                ref_seconds_max: 600,
                vram_gb_min: 4,
                notes: "48kHz via AudioVAE V2 super-resolution; accepts 16kHz refs.",
            },
            CloningBackend::XttsV2 => CloneCapabilities {
                name: "xtts-v2",
                licence: "Coqui Public Model License (non-commercial)",
                commercial_ok: false,
                languages: "17 languages",
                ref_seconds_min: 3,
                ref_seconds_max: 30,
                vram_gb_min: 3,
                notes: "Legacy default; superseded by Chatterbox for English.",
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloneCapabilities {
    pub name: &'static str,
    pub licence: &'static str,
    pub commercial_ok: bool,
    pub languages: &'static str,
    pub ref_seconds_min: u32,
    pub ref_seconds_max: u32,
    pub vram_gb_min: u32,
    pub notes: &'static str,
}

/// A cloning request. Consent is mandatory and checked by the engine;
/// there is no path to synthesize from a bare reference clip.
#[derive(Debug, Clone)]
pub struct CloneRequest {
    /// Text to synthesize.
    pub text: String,
    /// Voice identity this maps to (e.g. "V-007-R" + clone suffix).
    pub voice_id: String,
    /// Path to the consented reference WAV (16-44.1kHz mono, clean).
    pub reference_wav: String,
    /// Mandatory consent. No consent => the engine refuses.
    pub consent: ConsentRecord,
    /// Prosody from the governor's VoiceState (rate/pitch/pauses).
    pub voice: VoiceState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloneError {
    /// Fail-closed default: no engine configured on this host.
    NotConfigured,
    /// The hard rule: request lacked valid consent.
    ConsentMissing,
    /// Consent valid, but no reference clip supplied.
    NoReferenceClip,
    /// Backend binary/sidecar unavailable.
    BackendUnavailable(String),
    /// Synthesis failed; details only.
    SynthesisFailed(String),
}

impl std::fmt::Display for CloneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CloneError::NotConfigured => write!(f, "no cloning engine configured on this host"),
            CloneError::ConsentMissing => write!(f, "REFUSED: no valid consent for cloned voice"),
            CloneError::NoReferenceClip => write!(f, "REFUSED: no consented reference clip supplied"),
            CloneError::BackendUnavailable(s) => write!(f, "backend unavailable: {}", s),
            CloneError::SynthesisFailed(s) => write!(f, "synthesis failed: {}", s),
        }
    }
}

impl std::error::Error for CloneError {}

/// Synthesized audio: PCM16 mono at the given sample rate.
#[derive(Debug, Clone)]
pub struct ClonedAudio {
    pub pcm16: Vec<i16>,
    pub sample_rate: u32,
    pub backend: CloningBackend,
}

/// The adapter trait. Backends implement this; the governor calls it.
/// Default refusal: any request whose consent record is invalid fails
/// closed before any audio is touched.
pub trait CloneEngine: Send + Sync {
    fn backend(&self) -> CloningBackend;
    fn synthesize(&self, req: &CloneRequest) -> Result<ClonedAudio, CloneError>;
}

/// Consent gate shared by every engine implementation. Call FIRST.
pub fn check_consent(req: &CloneRequest) -> Result<(), CloneError> {
    match &req.consent {
        ConsentRecord::OwnerRecorded { recorded_at_ms } if *recorded_at_ms > 0 => Ok(()),
        ConsentRecord::LicensedOriginal { licence_ref } if !licence_ref.is_empty() => Ok(()),
        _ => Err(CloneError::ConsentMissing),
    }
}

/// Stub engine for GPU-less hosts (this VM): fails closed, honestly.
/// When a GPU host is ready, swap in a real backend implementing
/// `CloneEngine` — the governor's call site does not change.
pub struct NullCloneEngine;

impl CloneEngine for NullCloneEngine {
    fn backend(&self) -> CloningBackend {
        CloningBackend::Chatterbox // declares the INTENDED default, not a capability
    }
    fn synthesize(&self, _req: &CloneRequest) -> Result<ClonedAudio, CloneError> {
        Err(CloneError::NotConfigured)
    }
}

/// A resolved, consent-checked plan to attempt cloning on one turn.
#[derive(Debug, Clone)]
pub struct ClonePlan {
    pub backend: CloningBackend,
    pub reference_wav: String,
    pub consent: ConsentRecord,
}

/// The live-path decision. Pure over strings: no audio, no I/O, no GPU.
/// `Off` means deterministic prosody with zero clone involvement;
/// `Refused` names the reason and still falls back to prosody;
/// `Ready` carries a plan the engine may attempt (engine errors also
/// fall back — the turn never fails because a voice backend did).
#[derive(Debug, Clone)]
pub enum CloneDecision {
    Off,
    Refused(CloneError),
    Ready(ClonePlan),
}

/// Parse a declared-consent string. ONLY these two shapes are accepted:
///   "owner:<recorded_at_ms>"     — owner recorded the clip (ms > 0)
///   "licensed:<licence_ref>"     — documented licence/contract ref
/// Everything else — including any attempt to name a real living person —
/// parses to None, which the live path treats as a refusal. There is no
/// string that can express consent for a real living person.
pub fn parse_consent(s: &str) -> Option<ConsentRecord> {
    let s = s.trim();
    if let Some(ms) = s.strip_prefix("owner:") {
        let ms: u64 = ms.trim().parse().ok()?;
        if ms > 0 {
            return Some(ConsentRecord::OwnerRecorded { recorded_at_ms: ms });
        }
    } else if let Some(lic) = s.strip_prefix("licensed:") {
        let lic = lic.trim();
        if !lic.is_empty() {
            return Some(ConsentRecord::LicensedOriginal { licence_ref: lic.to_string() });
        }
    }
    None
}

/// Parse a backend name. Unknown names -> None; the caller applies the
/// declared default (Chatterbox) rather than failing the turn.
pub fn parse_backend(s: &str) -> Option<CloningBackend> {
    match s.trim().to_lowercase().as_str() {
        "chatterbox" => Some(CloningBackend::Chatterbox),
        "voxcpm2" | "vox-cpm2" | "vox_cpm2" => Some(CloningBackend::VoxCpm2),
        "xtts-v2" | "xttsv2" | "xtts" => Some(CloningBackend::XttsV2),
        _ => None,
    }
}

/// Decide, from optional request/env strings, whether this turn attempts
/// cloning. Structural rules, in order:
///   1. Nothing requested -> Off.
///   2. Consent unparseable/empty -> Refused(ConsentMissing).
///   3. No reference clip -> Refused(NoReferenceClip).
///   4. Else Ready(plan). The engine still runs check_consent first.
pub fn decide_clone(
    backend: Option<&str>,
    reference_wav: Option<&str>,
    consent: Option<&str>,
) -> CloneDecision {
    let requested =
        backend.is_some() || reference_wav.is_some() || consent.is_some();
    if !requested {
        return CloneDecision::Off;
    }
    let consent = match consent.and_then(parse_consent) {
        Some(c) => c,
        None => return CloneDecision::Refused(CloneError::ConsentMissing),
    };
    let reference_wav = match reference_wav.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => s.to_string(),
        None => return CloneDecision::Refused(CloneError::NoReferenceClip),
    };
    CloneDecision::Ready(ClonePlan {
        backend: backend.and_then(parse_backend).unwrap_or(CloningBackend::Chatterbox),
        reference_wav,
        consent,
    })
}

/// Attempt synthesis through the configured engine. The consent gate runs
/// FIRST — before any audio path is touched. Engine errors (including
/// NotConfigured on GPU-less hosts) propagate to the caller, which falls
/// back to deterministic prosody.
pub fn attempt_clone(
    engine: &dyn CloneEngine,
    plan: &ClonePlan,
    text: &str,
    voice_id: &str,
    voice: &VoiceState,
) -> Result<ClonedAudio, CloneError> {
    let req = CloneRequest {
        text: text.to_string(),
        voice_id: format!("{}+clone", voice_id),
        reference_wav: plan.reference_wav.clone(),
        consent: plan.consent.clone(),
        voice: voice.clone(),
    };
    check_consent(&req)?;
    engine.synthesize(&req)
}

/// Engine selection for the live path. Today: always the null stub —
/// this host has no GPU, so every Ready plan resolves to NotConfigured
/// and the turn falls back to deterministic prosody. A GPU host swaps
/// this constructor for a real `CloneEngine`; the api.rs call site
/// does not change.
pub fn clone_engine_from_env() -> std::sync::Arc<dyn CloneEngine> {
    std::sync::Arc::new(NullCloneEngine)
}

#[cfg(test)]
mod cloning_tests {
    use super::*;
    use crate::avatar::Emotion;
    use crate::domain::EpistemicStatus;
    use crate::governor::Intent;

    fn req_with(consent: Option<ConsentRecord>) -> CloneRequest {
        CloneRequest {
            text: "The truth is out there.".into(),
            voice_id: "V-007-R+clone".into(),
            reference_wav: "/tmp/ref.wav".into(),
            consent: consent.unwrap_or(ConsentRecord::OwnerRecorded { recorded_at_ms: 1 }),
            voice: derive_voice(EpistemicStatus::Unknown, Intent::Question, Emotion::Neutral, "indy"),
        }
    }

    #[test]
    fn test_stub_fails_closed_without_gpu() {
        let engine = NullCloneEngine;
        let r = engine.synthesize(&req_with(None));
        assert_eq!(r.unwrap_err(), CloneError::NotConfigured);
    }

    #[test]
    fn test_consent_gate_refuses_empty_records() {
        let bad_owner = req_with(Some(ConsentRecord::OwnerRecorded { recorded_at_ms: 0 }));
        assert_eq!(check_consent(&bad_owner).unwrap_err(), CloneError::ConsentMissing);
        let bad_lic = req_with(Some(ConsentRecord::LicensedOriginal { licence_ref: String::new() }));
        assert_eq!(check_consent(&bad_lic).unwrap_err(), CloneError::ConsentMissing);
    }

    #[test]
    fn test_consent_gate_accepts_valid_records() {
        assert!(check_consent(&req_with(None)).is_ok());
        let lic = req_with(Some(ConsentRecord::LicensedOriginal { licence_ref: "contract-2026-001".into() }));
        assert!(check_consent(&lic).is_ok());
    }

    #[test]
    fn test_no_real_person_variant_is_expressible() {
        // The enum has no variant for cloning a real living person.
        // This test pins that: matching exhaustively over ConsentRecord
        // must not need a third arm.
        let c = ConsentRecord::OwnerRecorded { recorded_at_ms: 1 };
        let label = match c {
            ConsentRecord::OwnerRecorded { .. } => "owner",
            ConsentRecord::LicensedOriginal { .. } => "licensed",
        };
        assert_eq!(label, "owner");
    }

    #[test]
    fn test_backend_capabilities_are_documented() {
        let c = CloningBackend::Chatterbox.capabilities();
        assert!(c.commercial_ok);
        assert_eq!(c.ref_seconds_min, 7);
        let x = CloningBackend::XttsV2.capabilities();
        assert!(!x.commercial_ok, "XTTS v2 is non-commercial");
    }
}

#[cfg(test)]
mod live_path_tests {
    use super::*;
    use crate::avatar::Emotion;
    use crate::domain::EpistemicStatus;
    use crate::governor::Intent;

    fn prosody() -> VoiceState {
        derive_voice(
            EpistemicStatus::Unknown,
            Intent::Question,
            Emotion::Neutral,
            "indy",
        )
    }

    #[test]
    fn test_decide_clone_off_when_nothing_requested() {
        assert!(matches!(decide_clone(None, None, None), CloneDecision::Off));
    }

    #[test]
    fn test_decide_clone_refuses_without_consent() {
        // Reference present, consent absent -> hard refusal.
        let d = decide_clone(Some("chatterbox"), Some("/tmp/ref.wav"), None);
        assert!(matches!(d, CloneDecision::Refused(CloneError::ConsentMissing)));
        // Empty consent string is the same as absent.
        let d = decide_clone(None, Some("/tmp/ref.wav"), Some(""));
        assert!(matches!(d, CloneDecision::Refused(CloneError::ConsentMissing)));
    }

    #[test]
    fn test_decide_clone_refuses_garbage_consent() {
        // No string can express consent for a real living person.
        for bad in [
            "gillian anderson",
            "celebrity:dana scully",
            "real:person",
            "owner:0",
            "owner:-5",
            "owner:abc",
            "licensed:",
            "licensed:   ",
        ] {
            let d = decide_clone(None, Some("/tmp/ref.wav"), Some(bad));
            assert!(
                matches!(d, CloneDecision::Refused(CloneError::ConsentMissing)),
                "consent {bad:?} must be refused"
            );
        }
    }

    #[test]
    fn test_decide_clone_refuses_without_reference_clip() {
        // Valid consent but no clip -> refused, never attempted.
        let d = decide_clone(None, None, Some("owner:12345"));
        assert!(matches!(d, CloneDecision::Refused(CloneError::NoReferenceClip)));
    }

    #[test]
    fn test_decide_clone_ready_with_valid_consent() {
        let d = decide_clone(Some("voxcpm2"), Some("/tmp/ref.wav"), Some("owner:12345"));
        match d {
            CloneDecision::Ready(p) => {
                assert_eq!(p.backend, CloningBackend::VoxCpm2);
                assert_eq!(p.reference_wav, "/tmp/ref.wav");
            }
            other => panic!("expected Ready, got {other:?}"),
        }
        // Unknown backend name degrades to the declared default, not an error.
        let d = decide_clone(None, Some("/tmp/ref.wav"), Some("licensed:contract-9"));
        match d {
            CloneDecision::Ready(p) => assert_eq!(p.backend, CloningBackend::Chatterbox),
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[test]
    fn test_attempt_clone_consent_gate_precedes_engine() {
        // Even a (hypothetical) misbehaving engine never sees an
        // unconsented request: the gate runs before engine.synthesize.
        let engine = NullCloneEngine;
        let plan = ClonePlan {
            backend: CloningBackend::Chatterbox,
            reference_wav: "/tmp/ref.wav".into(),
            consent: ConsentRecord::OwnerRecorded { recorded_at_ms: 0 }, // invalid
        };
        let r = attempt_clone(&engine, &plan, "hi", "V-007-P", &prosody());
        assert_eq!(r.unwrap_err(), CloneError::ConsentMissing);
    }

    #[test]
    fn test_null_engine_triggers_prosody_fallback() {
        // THE fallback path: valid plan, GPU-less host -> NotConfigured.
        // The live path maps this to deterministic prosody + status "fallback".
        let engine = NullCloneEngine;
        let plan = ClonePlan {
            backend: CloningBackend::Chatterbox,
            reference_wav: "/tmp/ref.wav".into(),
            consent: ConsentRecord::OwnerRecorded { recorded_at_ms: 999 },
        };
        let r = attempt_clone(&engine, &plan, "The truth is out there.", "V-007-P", &prosody());
        assert_eq!(r.unwrap_err(), CloneError::NotConfigured);
    }

    #[test]
    fn test_clone_decision_is_deterministic() {
        // Same inputs -> same decision, every time. No randomness, no I/O.
        let a = decide_clone(Some("chatterbox"), Some("/tmp/ref.wav"), Some("owner:1"));
        let b = decide_clone(Some("chatterbox"), Some("/tmp/ref.wav"), Some("owner:1"));
        assert!(matches!(a, CloneDecision::Ready(_)));
        assert!(matches!(b, CloneDecision::Ready(_)));
    }

    #[test]
    fn test_clone_path_never_touches_epistemics() {
        // decide_clone takes no epistemic input by construction: voice
        // presentation cannot promote or demote a claim. Compile-time
        // guarantee, pinned by this test's signature usage.
        let d_unknown = decide_clone(None, Some("/tmp/r.wav"), Some("owner:7"));
        assert!(matches!(d_unknown, CloneDecision::Ready(_)));
    }
}

#[cfg(test)]
mod corner_tests {
    use super::*;

    #[test]
    fn test_voice_id_composition_covers_full_id_space() {
        // The leak fix replaced Box::leak(format!) with a static lookup.
        // Pin the full 5x3 id space: every combo resolves, none duplicates.
        let personas = ["indy", "lucidota", "archivist", "wreck", "captain"];
        let eras = [ScullyEra::Rookie, ScullyEra::Prime, ScullyEra::Veteran];
        let mut seen = std::collections::HashSet::new();
        for p in personas {
            for e in eras {
                let v = derive_voice_full(
                    EpistemicStatus::Unknown, Intent::Question, Emotion::Neutral, p, e, Tone::Dry,
                );
                assert!(v.voice_id.starts_with("V-0"), "id {:?}", v.voice_id);
                assert_eq!(v.voice_id.len(), 7, "id {:?}", v.voice_id);
                assert!(seen.insert(v.voice_id), "duplicate id {:?}", v.voice_id);
            }
        }
        assert_eq!(seen.len(), 15);
        // Unknown persona degrades to the indy base, not a panic.
        let v = derive_voice_full(
            EpistemicStatus::Unknown, Intent::Question, Emotion::Neutral,
            "", ScullyEra::Prime, Tone::Dry,
        );
        assert_eq!(v.voice_id, "V-007-P");
    }

    #[test]
    fn test_voice_id_stable_across_calls() {
        // Regression pin for the Box::leak fix: repeated calls must return
        // equal VALUES (and, being &'static, the same address — no per-call
        // allocation is possible by construction).
        let a = derive_voice_full(
            EpistemicStatus::Unknown, Intent::AuthorityRequest, Emotion::Skeptical,
            "wreck", ScullyEra::Veteran, Tone::Steel,
        );
        let b = derive_voice_full(
            EpistemicStatus::Unknown, Intent::AuthorityRequest, Emotion::Skeptical,
            "wreck", ScullyEra::Veteran, Tone::Steel,
        );
        assert_eq!(a.voice_id, b.voice_id);
        assert_eq!(a.voice_id, "V-034-V");
        assert!(std::ptr::eq(a.voice_id, b.voice_id), "static ids share address");
    }

    #[test]
    fn test_parse_consent_edge_cases() {
        // Whitespace is trimmed.
        assert!(matches!(
            parse_consent("  owner:123  "),
            Some(ConsentRecord::OwnerRecorded { recorded_at_ms: 123 })
        ));
        // Case-sensitive prefix: fail-closed, not forgiving.
        assert_eq!(parse_consent("OWNER:123"), None);
        assert_eq!(parse_consent("Owner:123"), None);
        // u64 overflow fails closed.
        assert_eq!(parse_consent("owner:18446744073709551616"), None);
        // u64::MAX is accepted (valid timestamp space).
        assert!(parse_consent("owner:18446744073709551615").is_some());
        // Empty / whitespace licence ref fails closed.
        assert_eq!(parse_consent("licensed:   "), None);
        // Nested-prefix confusion stays LicensedOriginal, never OwnerRecorded.
        match parse_consent("licensed:owner:123") {
            Some(ConsentRecord::LicensedOriginal { licence_ref }) =>
                assert_eq!(licence_ref, "owner:123"),
            other => panic!("unexpected {other:?}"),
        }
        // Empty string and bare prefixes fail closed.
        assert_eq!(parse_consent(""), None);
        assert_eq!(parse_consent("owner:"), None);
        assert_eq!(parse_consent(":"), None);
    }

    #[test]
    fn test_parse_backend_edge_cases() {
        assert_eq!(parse_backend("VOXCPM2"), Some(CloningBackend::VoxCpm2));
        assert_eq!(parse_backend("  xtts  "), Some(CloningBackend::XttsV2));
        assert_eq!(parse_backend("ChatterBox"), Some(CloningBackend::Chatterbox));
        assert_eq!(parse_backend(""), None);
        assert_eq!(parse_backend("   "), None);
        assert_eq!(parse_backend("gpt-4"), None);
    }

    #[test]
    fn test_decide_clone_garbage_backend_without_consent() {
        // A garbage backend string alone counts as "requested" but consent
        // is still mandatory: hard refusal, never an attempt.
        let d = decide_clone(Some("not-a-backend"), None, None);
        assert!(matches!(d, CloneDecision::Refused(CloneError::ConsentMissing)));
        // Whitespace-only reference is the same as absent.
        let d = decide_clone(None, Some("   "), Some("owner:42"));
        assert!(matches!(d, CloneDecision::Refused(CloneError::NoReferenceClip)));
    }

    #[test]
    fn test_derive_voice_never_panics_on_any_input_combo() {
        // Exhaust the input space: no combo may produce NaN, infinite, or
        // out-of-range prosody.
        for epi in [EpistemicStatus::Unknown, EpistemicStatus::Proven, EpistemicStatus::Disproven] {
            for intent in [Intent::Question, Intent::AuthorityRequest, Intent::HypothesisAssertion,
                           Intent::ClaimAssertion, Intent::Converse] {
                for era in [ScullyEra::Rookie, ScullyEra::Prime, ScullyEra::Veteran] {
                    for tone in [Tone::Dry, Tone::Clinical, Tone::Warm, Tone::Steel] {
                        for persona in ["indy", "lucidota", "archivist", "wreck", "captain", "", "x"] {
                            let v = derive_voice_full(epi, intent, Emotion::Neutral, persona, era, tone);
                            assert!(v.rate.is_finite() && (0.5..=2.0).contains(&v.rate));
                            assert!(v.pitch.is_finite() && (0.0..=2.0).contains(&v.pitch));
                            assert!(v.pause_ms <= 2000);
                        }
                    }
                }
            }
        }
    }
}
