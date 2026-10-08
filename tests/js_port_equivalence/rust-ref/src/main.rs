//! Reference oracle for the JS governor-port equivalence harness (NIH-FIND-5).
//! Reads the harness intents.json from stdin, prints reference outputs as JSON.
//! Uses the REAL Rust governor functions — never a reimplementation.

use indy_reads::domain::EpistemicStatus;
use indy_reads::governor::{claims_authority, evaluate_utterance, validate_reply};
use serde::Deserialize;
use std::io::Read;

#[derive(Deserialize)]
struct Intents {
    turns: Vec<Turn>,
    replies: Vec<ReplyCase>,
}

#[derive(Deserialize)]
struct Turn {
    id: String,
    utterance: String,
}

#[derive(Deserialize)]
struct ReplyCase {
    id: String,
    reply: String,
}

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    let intents: Intents = serde_json::from_str(&input).unwrap();

    let mut turns_out = Vec::new();
    for t in &intents.turns {
        let authority = claims_authority(&t.utterance);
        let (intent, decision, epistemic, claim, reply, error) = match evaluate_utterance(&t.utterance) {
            Ok((ev, _)) => (
                ev.intent,
                ev.decision,
                ev.epistemic_state,
                ev.claim_id.is_some(),
                ev.reply,
                None::<String>,
            ),
            Err(e) => (
                String::new(),
                String::new(),
                String::new(),
                false,
                String::new(),
                Some(e),
            ),
        };
        turns_out.push(serde_json::json!({
            "id": t.id,
            "authority": authority,
            "intent": intent,
            "decision": decision,
            "epistemic": epistemic,
            "claim_persisted": claim,
            "reply": reply,
            "error": error,
        }));
    }

    let mut replies_out = Vec::new();
    for r in &intents.replies {
        let out = validate_reply(&r.reply, EpistemicStatus::Unknown);
        replies_out.push(serde_json::json!({
            "id": r.id,
            "vetoed": out != r.reply,
        }));
    }

    println!(
        "{}",
        serde_json::json!({ "turns": turns_out, "replies": replies_out })
    );
}
