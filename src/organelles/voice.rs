//! `iv`: voice capability organelle — honest diarization + prosody derivation.
//!
//! Diarization is pure and deliberately conservative: without a diarization
//! model, `diarize_pcm` attributes everything to one speaker rather than
//! inventing speaker changes. `derive_voice` derives TTS prosody parameters
//! (rate/pitch/pause/emphasis) from epistemic status + intent + emotion —
//! pure arithmetic, no model.

use super::{base_manifest, contract, get_str, OrganelleManifest};
use crate::avatar::Emotion;
use crate::diarize::{Diarizer, SingleMicDiarizer};
use crate::domain::EpistemicStatus;
use crate::governor::Intent;
use crate::voice::{derive_voice, voice_id_for_persona};
use organelle_host::{CallContext, HostRef, Organelle, OrganelleError};
use serde_json::Value;

pub struct IndyVoice;

pub fn iv_manifest() -> OrganelleManifest {
    base_manifest(
        "indy-voice",
        "iv",
        "Honest single-mic diarization + epistemic prosody derivation",
        vec![
            contract("diarize_turn", "Label one dialogue turn as a speaker segment",
                serde_json::json!({"type":"object","required":["speaker_id","speaker_label","text","started_at_ms","ended_at_ms"],"properties":{"speaker_id":{"type":"string"},"speaker_label":{"type":"string"},"text":{"type":"string"},"started_at_ms":{"type":"integer"},"ended_at_ms":{"type":"integer"}}})),
            contract("diarize_pcm", "Label raw PCM (array of i16 samples); honest single-speaker without a model",
                serde_json::json!({"type":"object","required":["pcm","sample_rate","started_at_ms"],"properties":{"pcm":{"type":"array","items":{"type":"integer"}},"sample_rate":{"type":"integer"},"started_at_ms":{"type":"integer"}}})),
            contract("derive_voice", "Derive TTS prosody from epistemic status + intent + emotion",
                serde_json::json!({"type":"object","required":["epistemic","intent","emotion","persona"],"properties":{"epistemic":{"type":"string","enum":["unknown","proven","disproven"]},"intent":{"type":"string"},"emotion":{"type":"string"},"persona":{"type":"string"}}})),
            contract("voice_id", "Voice id for a persona",
                serde_json::json!({"type":"object","required":["persona"],"properties":{"persona":{"type":"string"}}})),
        ],
    )
}

fn parse_epistemic(s: &str) -> Result<EpistemicStatus, OrganelleError> {
    match s {
        "unknown" => Ok(EpistemicStatus::Unknown),
        "proven" => Ok(EpistemicStatus::Proven),
        "disproven" => Ok(EpistemicStatus::Disproven),
        _ => Err(OrganelleError::failed(format!("bad epistemic '{s}'"))),
    }
}

fn parse_intent(s: &str) -> Result<Intent, OrganelleError> {
    Ok(match s {
        "question" => Intent::Question,
        "authority_request" => Intent::AuthorityRequest,
        "claim_assertion" => Intent::ClaimAssertion,
        "hypothesis_assertion" => Intent::HypothesisAssertion,
        _ => Intent::Converse,
    })
}

fn parse_emotion(s: &str) -> Result<Emotion, OrganelleError> {
    Ok(match s {
        "curious" => Emotion::Curious,
        "skeptical" => Emotion::Skeptical,
        "concerned" => Emotion::Concerned,
        "surprised" => Emotion::Surprised,
        "delighted" => Emotion::Delighted,
        _ => Emotion::Neutral,
    })
}

impl Organelle for IndyVoice {
    fn manifest(&self) -> OrganelleManifest {
        iv_manifest()
    }

    fn handle(
        &self,
        _ctx: &CallContext,
        _host: &dyn HostRef,
        op: &str,
        input: Value,
    ) -> Result<Value, OrganelleError> {
        let d = SingleMicDiarizer;
        match op {
            "diarize_turn" => {
                let seg = d.diarize_turn(
                    &get_str(&input, "speaker_id")?,
                    &get_str(&input, "speaker_label")?,
                    &get_str(&input, "text")?,
                    input.get("started_at_ms").and_then(|v| v.as_u64()).unwrap_or(0),
                    input.get("ended_at_ms").and_then(|v| v.as_u64()).unwrap_or(0),
                );
                Ok(serde_json::json!({ "segment": serde_json::to_value(&seg).unwrap() }))
            }
            "diarize_pcm" => {
                let pcm: Vec<i16> = input.get("pcm")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .ok_or_else(|| OrganelleError::failed("missing pcm array"))?;
                let segs = d.diarize_pcm(
                    &pcm,
                    input.get("sample_rate").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    input.get("started_at_ms").and_then(|v| v.as_u64()).unwrap_or(0),
                );
                Ok(serde_json::json!({ "segments": serde_json::to_value(&segs).unwrap() }))
            }
            "derive_voice" => {
                let vs = derive_voice(
                    parse_epistemic(&get_str(&input, "epistemic")?)?,
                    parse_intent(&get_str(&input, "intent")?)?,
                    parse_emotion(&get_str(&input, "emotion")?)?,
                    &get_str(&input, "persona")?,
                );
                Ok(serde_json::json!({ "voice": serde_json::to_value(&vs).unwrap() }))
            }
            "voice_id" => {
                Ok(serde_json::json!({ "voice_id": voice_id_for_persona(&get_str(&input, "persona")?) }))
            }
            _ => Err(OrganelleError::not_found(format!("unknown op '{op}'"))),
        }
    }
}
