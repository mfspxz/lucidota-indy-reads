//! Reference oracle for the JS governor-port equivalence harness (NIH-FIND-5).
//! Reads the harness intents.json from stdin, prints reference outputs as JSON.
//! Uses the REAL Rust governor functions — never a reimplementation.
//!
//! Two modes, detected from the input shape:
//!   - intents mode: {"turns": [...], "replies": [...]} (original)
//!   - mcp mode: {"steps": [{"id","tool","arguments"}]} — drives the REAL
//!     indy_reads::mcp::{McpState, call_tool, tool_descriptors} through a
//!     scripted stateful sequence with $step.field substitution.

use indy_reads::domain::EpistemicStatus;
use indy_reads::governor::{claims_authority, evaluate_utterance, validate_reply};
use indy_reads::mcp::{call_tool, tool_descriptors, McpState};
use serde::Deserialize;
use serde_json::{Map, Value};
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

#[derive(Deserialize)]
struct McpScript {
    steps: Vec<McpStep>,
}

#[derive(Deserialize)]
struct McpStep {
    id: String,
    tool: String,
    #[serde(default)]
    arguments: Value,
}

/// Resolve "$step.field" references against prior step payloads (parsed inner
/// JSON of each tool result). Non-reference strings pass through unchanged.
fn substitute(args: &Value, results: &Map<String, Value>) -> Value {
    match args {
        Value::String(s) if s.starts_with('$') => {
            let path = &s[1..];
            let mut it = path.splitn(2, '.');
            let (sid, field) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
            results
                .get(sid)
                .and_then(|p| p.get(field))
                .cloned()
                .unwrap_or_else(|| Value::String(format!("<unresolved:{}>", s)))
        }
        Value::Array(a) => Value::Array(a.iter().map(|v| substitute(v, results)).collect()),
        Value::Object(o) => {
            let mut m = Map::new();
            for (k, v) in o {
                m.insert(k.clone(), substitute(v, results));
            }
            Value::Object(m)
        }
        other => other.clone(),
    }
}

/// Parse the inner payload of a tool result envelope
/// {content:[{text: "<json>"}], isError}.
fn inner_payload(res: &Value) -> Value {
    res.get("content")
        .and_then(|c| c.get(0))
        .and_then(|t| t.get("text"))
        .and_then(|s| s.as_str())
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(Value::Null)
}

fn run_mcp_script(script: &McpScript) -> Value {
    let mut state = McpState::new();
    let mut results: Map<String, Value> = Map::new();
    let mut out = Vec::new();
    for step in &script.steps {
        let args = substitute(&step.arguments, &results);
        let res = call_tool(&mut state, &step.tool, args);
        let is_err = res
            .get("isError")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let payload = inner_payload(&res);
        results.insert(step.id.clone(), payload.clone());
        out.push(serde_json::json!({
            "id": step.id,
            "tool": step.tool,
            "isError": is_err,
            "payload": payload,
        }));
    }
    serde_json::json!({ "descriptors": tool_descriptors(), "steps": out })
}

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    let raw: Value = serde_json::from_str(&input).unwrap();
    // MCP script mode: input carries "steps".
    if raw.get("steps").is_some() {
        let script: McpScript = serde_json::from_value(raw).unwrap();
        println!("{}", run_mcp_script(&script));
        return;
    }
    let intents: Intents = serde_json::from_value(raw).unwrap();

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
