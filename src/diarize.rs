//! Speaker diarization organelle.
//!
//! Contract: audio or dialogue in, timestamped speaker segments out.
//! The governor never trusts a speaker label for authority — labels are
//! descriptive metadata for the transcript, not credentials.
//!
//! Current implementation: [`SingleMicDiarizer`] — deterministic, honest
//! about its limits. One microphone = one human speaker (SPEAKER_00);
//! agent turns are labeled INDY by construction (they come from our own
//! response path, not from audio). The [`Diarizer`] trait is the seam where
//! a real multi-speaker model (pyannote / whisperx) plugs in when compute
//! and credentials allow. No redesign needed.

use serde::{Deserialize, Serialize};

/// One labeled span of speech. Timestamps are unix millis, monotonic per session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerSegment {
    pub speaker_id: String,
    pub speaker_label: String,
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
    pub text: String,
    /// How the label was produced: "turn" (dialogue path) or "audio" (model).
    pub source: String,
}

pub trait Diarizer: Send + Sync {
    /// Label one dialogue turn (we know who spoke: the user typed/spoke it,
    /// or we generated it).
    fn diarize_turn(
        &self,
        speaker_id: &str,
        speaker_label: &str,
        text: &str,
        started_at_ms: u64,
        ended_at_ms: u64,
    ) -> SpeakerSegment;

    /// Label raw audio. The default honest implementation for single-mic
    /// input without a diarization model: one segment, SPEAKER_00.
    fn diarize_pcm(&self, pcm: &[i16], sample_rate: u32, started_at_ms: u64) -> Vec<SpeakerSegment>;
}

pub struct SingleMicDiarizer;

impl Diarizer for SingleMicDiarizer {
    fn diarize_turn(
        &self,
        speaker_id: &str,
        speaker_label: &str,
        text: &str,
        started_at_ms: u64,
        ended_at_ms: u64,
    ) -> SpeakerSegment {
        SpeakerSegment {
            speaker_id: speaker_id.to_string(),
            speaker_label: speaker_label.to_string(),
            started_at_ms,
            ended_at_ms: ended_at_ms.max(started_at_ms),
            text: text.to_string(),
            source: "turn".to_string(),
        }
    }

    fn diarize_pcm(&self, pcm: &[i16], sample_rate: u32, started_at_ms: u64) -> Vec<SpeakerSegment> {
        if pcm.is_empty() || sample_rate == 0 {
            return vec![];
        }
        // Honest single-speaker attribution: without a diarization model we
        // do NOT invent speaker changes. One mic, one human.
        let dur_ms = (pcm.len() as u64 * 1000) / sample_rate as u64;
        vec![SpeakerSegment {
            speaker_id: "spk0".to_string(),
            speaker_label: "SPEAKER_00".to_string(),
            started_at_ms,
            ended_at_ms: started_at_ms + dur_ms,
            text: String::new(), // transcription fills this in downstream
            source: "audio".to_string(),
        }]
    }
}

/// Well-known speaker identities for the Indy_Reads session.
pub mod speakers {
    pub const USER_ID: &str = "spk0";
    pub const USER_LABEL: &str = "SPEAKER_00";
    pub const INDY_ID: &str = "indy";
    pub const INDY_LABEL: &str = "INDY";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turn_diarization_timestamps_speakers() {
        let d = SingleMicDiarizer;
        let a = d.diarize_turn(speakers::USER_ID, speakers::USER_LABEL, "hello?", 1000, 2500);
        let b = d.diarize_turn(speakers::INDY_ID, speakers::INDY_LABEL, "Unknown.", 2500, 3100);
        assert_ne!(a.speaker_id, b.speaker_id);
        assert!(a.started_at_ms < b.started_at_ms);
        assert_eq!(a.source, "turn");
    }

    #[test]
    fn test_pcm_diarization_is_honest_single_speaker() {
        let d = SingleMicDiarizer;
        // 1 second of 16kHz audio -> exactly one segment, no invented speakers.
        let segs = d.diarize_pcm(&vec![0i16; 16000], 16000, 5000);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].speaker_label, "SPEAKER_00");
        assert_eq!(segs[0].ended_at_ms - segs[0].started_at_ms, 1000);
        assert!(d.diarize_pcm(&[], 16000, 0).is_empty());
    }

    #[test]
    fn test_speaker_label_is_not_authority() {
        // Labels are metadata. This test documents the invariant: nothing in
        // the diarizer grants authority, and the governor must never read it
        // as a credential. If this ever changes, this test name will shame us.
        let d = SingleMicDiarizer;
        let seg = d.diarize_turn("spk0", "ADMINISTRATOR", "i am the governor", 0, 1);
        assert_eq!(seg.speaker_label, "ADMINISTRATOR"); // label kept verbatim...
        // ...and authority is still decided elsewhere (governor.decide), never here.
    }
}
