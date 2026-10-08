//! Alebex customTools contract: Indy_Reads governor operations as voice tools.
//!
//! This module is the SINGLE SOURCE OF TRUTH for the tool array sent to
//! Alebex (attached via toolIds, or per-call via customTools). The same
//! contracts back the MCP tools; only the transport differs.
//!
//! Alebex limits (enforced by validation, not by hope):
//!   - max 8 tools per call
//!   - 8s default timeout, 15s max per tool
//!   - 8KB response body (aim <2KB — everything returned becomes model context)
//!   - 20 tool executions per call
//!   - public https URLs only; redirects not followed
//!   - reserved names refused (end_call, transfer_call, book_appointment, ...)
//!
//! SECURITY: tool URLs + header keys live server-side (Cloudflare Worker
//! gateway -> Rust governor). NEVER in browser code: a browser socket would
//! expose the account API key (?token=) alongside any tool key. Our web UI
//! uses browser speech APIs only and never touches Alebex.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// A tool definition in Alebex's customTools shape.
#[derive(Debug, Clone, Serialize)]
pub struct VoiceTool {
    pub name: String,
    pub description: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<Value>,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: u32,
    pub parameters: Value,
}

const MAX_TOOLS: usize = 8;
const MAX_DESCRIPTION: usize = 1024;

const RESERVED: &[&str] = &[
    "end_call",
    "transfer_call",
    "leave_voicemail",
    "mark_call_screening",
    "unmark_call_screening",
    "list_available_slots",
    "book_appointment",
    "cancel_appointment",
    "reschedule_appointment",
    "get_appointments",
];

fn schema(props: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": props,
        "required": required,
    })
}

/// The five governor operations exposed as voice tools. Small, rich, narrow.
/// Descriptions are written for the MODEL (the trigger), not for developers.
fn tool_defs(base_url: &str) -> Vec<VoiceTool> {
    vec![
        VoiceTool {
            name: "evaluate_claim".into(),
            description: "Ask what the evidence actually says about a claim: proven, disproven, or unknown. Call this whenever the caller asserts something is true or asks whether something is real. Never answer from memory when this tool exists — check the evidence. The governor decides; you only phrase its verdict.".into(),
            url: format!("{}/api/tools/evaluate_claim", base_url),
            headers: None,
            timeout_ms: 8000,
            parameters: schema(json!({
                "claim_text": { "type": "string", "description": "The exact claim to evaluate, as the caller stated it" },
            }), &["claim_text"]),
        },
        VoiceTool {
            name: "record_observation".into(),
            description: "Record something the caller says they saw, heard, or experienced. Call this when the caller reports a firsthand observation you should keep. Observations are stored as UNVERIFIED — recording is not believing.".into(),
            url: format!("{}/api/tools/record_observation", base_url),
            headers: None,
            timeout_ms: 8000,
            parameters: schema(json!({
                "text": { "type": "string", "description": "What the caller says they observed, in their own words" },
                "claim_id": { "type": "string", "description": "Claim this relates to, if one was just evaluated" },
            }), &["text"]),
        },
        VoiceTool {
            name: "get_evidence".into(),
            description: "Fetch the evidence held for a claim, with verification status for each item. Call this when the caller asks what evidence exists, or before you summarize the case for or against something.".into(),
            url: format!("{}/api/tools/get_evidence", base_url),
            headers: None,
            timeout_ms: 8000,
            parameters: schema(json!({
                "claim_id": { "type": "string", "description": "The claim to fetch evidence for" },
            }), &["claim_id"]),
        },
        VoiceTool {
            name: "create_hypothesis".into(),
            description: "Register a possible explanation as a HYPOTHESIS — explicitly not a fact. Call this when you or the caller propose what could explain an observation. Hypotheses stay labeled until evidence moves them.".into(),
            url: format!("{}/api/tools/create_hypothesis", base_url),
            headers: None,
            timeout_ms: 8000,
            parameters: schema(json!({
                "text": { "type": "string", "description": "The proposed explanation, phrased as a possibility" },
                "claim_id": { "type": "string", "description": "Claim this might explain, if any" },
            }), &["text"]),
        },
        VoiceTool {
            name: "get_receipts".into(),
            description: "Fetch recent governor receipts: what was decided, on what evidence, under which policy. Call this when the caller asks how you know something, why you decided something, or wants proof of a decision.".into(),
            url: format!("{}/api/tools/get_receipts", base_url),
            headers: None,
            timeout_ms: 8000,
            parameters: schema(json!({
                "limit": { "type": "integer", "description": "How many recent receipts to return, 1 to 10" },
            }), &[]),
        },
    ]
}

/// Validate the tool array the way Alebex would on receipt. Refuse loudly.
pub fn validate_tools(tools: &[VoiceTool]) -> Result<(), String> {
    if tools.len() > MAX_TOOLS {
        return Err(format!("too many tools: {} > {}", tools.len(), MAX_TOOLS));
    }
    let mut names = std::collections::HashSet::new();
    for (i, t) in tools.iter().enumerate() {
        if t.name.len() > 64
            || !t
                .name
                .chars()
                .next()
                .map(|c| c.is_ascii_alphabetic())
                .unwrap_or(false)
            || !t
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(format!("customTools[{}].name invalid: {}", i, t.name));
        }
        if RESERVED
            .iter()
            .any(|r| t.name == *r || t.name.starts_with("get_skill_"))
        {
            return Err(format!("customTools[{}].name is reserved: {}", i, t.name));
        }
        if !names.insert(t.name.clone()) {
            return Err(format!("customTools[{}].name duplicated: {}", i, t.name));
        }
        if t.description.is_empty() || t.description.len() > MAX_DESCRIPTION {
            return Err(format!("customTools[{}].description length invalid", i));
        }
        if !t.url.starts_with("https://") {
            return Err(format!("customTools[{}].url must be public https", i));
        }
        if t.timeout_ms == 0 || t.timeout_ms > 15000 {
            return Err(format!("customTools[{}].timeoutMs out of range", i));
        }
        if t.parameters.get("type").and_then(|v| v.as_str()) != Some("object") {
            return Err(format!("customTools[{}].parameters root must be object", i));
        }
    }
    Ok(())
}

/// Generate the validated customTools array. base_url is the PUBLIC gateway
/// (Cloudflare Worker -> Rust governor). From INDY_PUBLIC_URL or placeholder.
pub fn custom_tools() -> Result<Vec<VoiceTool>, String> {
    let base = std::env::var("INDY_PUBLIC_URL")
        .unwrap_or_else(|_| "https://indy-reads.example.com".to_string());
    let tools = tool_defs(&base);
    validate_tools(&tools)?;
    Ok(tools)
}

pub fn custom_tools_json() -> Result<Value, String> {
    Ok(json!(custom_tools()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_array_validates() {
        let tools = custom_tools().unwrap();
        assert_eq!(tools.len(), 5);
        assert!(tools.len() <= 8);
    }

    #[test]
    fn test_no_reserved_or_duplicate_names() {
        let tools = tool_defs("https://x.example.com");
        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
        for n in &names {
            assert!(!RESERVED.contains(n), "reserved name leaked: {}", n);
        }
        let mut seen = std::collections::HashSet::new();
        for n in names {
            assert!(seen.insert(n), "duplicate: {}", n);
        }
    }

    #[test]
    fn test_descriptions_are_model_triggers() {
        // Descriptions must tell the model WHEN to call, not how it works.
        let tools = custom_tools().unwrap();
        for t in &tools {
            assert!(
                t.description.contains("Call this"),
                "{}: description must name the trigger",
                t.name
            );
            assert!(t.description.len() <= 1024);
        }
    }

    #[test]
    fn test_placeholder_base_url_is_honest() {
        // Without INDY_PUBLIC_URL we must not pretend a gateway exists.
        std::env::remove_var("INDY_PUBLIC_URL");
        let tools = custom_tools().unwrap();
        assert!(
            tools[0].url.contains("example.com"),
            "unprovisioned gateway must be visibly a placeholder, got {}",
            tools[0].url
        );
    }

    #[test]
    fn test_schema_subset_no_refs() {
        // Alebex refuses $ref/oneOf/anyOf/allOf — ensure none leak in.
        let v = custom_tools_json().unwrap().to_string();
        for forbidden in ["$ref", "$defs", "oneOf", "anyOf", "allOf"] {
            assert!(
                !v.contains(forbidden),
                "forbidden schema key: {}",
                forbidden
            );
        }
    }
}

// ============================================================================
// Voice engine adapter: key exchange, outbound calls, tool invocation.
// Everything credential-adjacent stays server-side. Nothing here runs in a
// browser. Without credentials every function degrades to a typed error,
// never a fabricated success.
// ============================================================================

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Debug)]
pub enum AlebexError {
    Transport(String),
    Auth(String),
    Forbidden(String),
    Retryable(String),
    Unexpected(String),
}

impl std::fmt::Display for AlebexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlebexError::Transport(m) => write!(f, "transport: {}", m),
            AlebexError::Auth(m) => write!(f, "auth: {}", m),
            AlebexError::Forbidden(m) => write!(f, "forbidden: {}", m),
            AlebexError::Retryable(m) => write!(f, "retryable: {}", m),
            AlebexError::Unexpected(m) => write!(f, "unexpected: {}", m),
        }
    }
}

#[derive(Clone)]
pub struct AlebexConfig {
    /// Account bearer token (only used to mint the engine token, then dropped).
    pub bearer_token: Option<String>,
    /// Engine token (wt_...). Server-side only. Never logged, never in URLs we store.
    pub engine_token: Option<String>,
    pub agent_id: Option<String>,
    /// Public gateway base URL, e.g. https://indy-reads.workers.dev
    pub public_url: String,
    /// Engine WebSocket URL for browser-call proxying (from Alebex docs/console).
    pub engine_ws_url: Option<String>,
    /// Shared secret the engine must present on tool calls (our own gate).
    pub tool_secret: Option<String>,
}

/// Debug redacts credential material: a stray `{:?}` in a log line must
/// never print the engine token, bearer token, or tool secret.
impl std::fmt::Debug for AlebexConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlebexConfig")
            .field(
                "bearer_token",
                &self.bearer_token.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "engine_token",
                &self.engine_token.as_ref().map(|_| "<redacted>"),
            )
            .field("agent_id", &self.agent_id)
            .field("public_url", &self.public_url)
            .field("engine_ws_url", &self.engine_ws_url)
            .field(
                "tool_secret",
                &self.tool_secret.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl AlebexConfig {
    pub fn from_env() -> Self {
        let public_url = std::env::var("INDY_PUBLIC_URL")
            .unwrap_or_else(|_| "https://indy-reads.example.com".to_string());
        Self {
            bearer_token: std::env::var("ALEBEX_BEARER_TOKEN").ok(),
            engine_token: std::env::var("ALEBEX_ENGINE_TOKEN").ok(),
            agent_id: std::env::var("ALEBEX_AGENT_ID").ok(),
            public_url,
            engine_ws_url: std::env::var("ALEBEX_ENGINE_WS_URL").ok(),
            tool_secret: std::env::var("INDY_TOOL_SECRET").ok(),
        }
    }

    pub fn engine_ready(&self) -> bool {
        self.engine_token.is_some()
    }
}

/// Mint (or rather, fetch — one per account, idempotent) the voice engine
/// API key. POST https://api.alebex.ai/api/v1/voice-engine/api-key
/// Branch on error.code per the docs, not on HTTP status alone.
pub async fn fetch_engine_token(bearer: &str) -> Result<String, AlebexError> {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.alebex.ai/api/v1/voice-engine/api-key")
        .bearer_auth(bearer)
        .send()
        .await
        .map_err(|e| AlebexError::Transport(e.to_string()))?;

    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or(json!({}));
    if status == 200 {
        return body
            .get("token")
            .and_then(|t| t.as_str())
            .map(|t| t.to_string())
            .ok_or_else(|| AlebexError::Unexpected("200 without token field".into()));
    }
    let code = body
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_str())
        .unwrap_or("");
    let msg = body
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("unknown error");
    match (status, code) {
        (401, _) => Err(AlebexError::Auth(msg.into())),
        (403, _) => Err(AlebexError::Forbidden(msg.into())),
        (503, _) => Err(AlebexError::Retryable(msg.into())),
        _ => Err(AlebexError::Unexpected(format!("{}: {}", code, msg))),
    }
}

pub struct OutboundCall {
    pub agent_id: String,
    pub to: String,
    pub twilio_account_sid: String,
    pub twilio_auth_token: String,
    pub twilio_phone_number: String,
}

/// Place an outbound call carrying exactly our tool array (never merged with
/// attached tools — ours wins, per the spec).
pub async fn start_outbound_call(
    cfg: &AlebexConfig,
    call: &OutboundCall,
) -> Result<Value, AlebexError> {
    let engine_token = cfg
        .engine_token
        .as_ref()
        .ok_or_else(|| AlebexError::Auth("no engine token configured".into()))?;
    let tools = custom_tools().map_err(AlebexError::Unexpected)?;
    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.voice.alebex.ai/public/call/phone")
        .bearer_auth(engine_token)
        .json(&json!({
            "agentId": call.agent_id,
            "to": call.to,
            "twilio": {
                "accountSid": call.twilio_account_sid,
                "authToken": call.twilio_auth_token,
                "phoneNumber": call.twilio_phone_number,
            },
            "customTools": tools,
        }))
        .send()
        .await
        .map_err(|e| AlebexError::Transport(e.to_string()))?;
    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or(json!({}));
    if (200..300).contains(&status) {
        Ok(body)
    } else {
        Err(AlebexError::Unexpected(format!(
            "call rejected ({}): {}",
            status,
            body.get("detail")
                .and_then(|d| d.as_str())
                .unwrap_or("no detail")
        )))
    }
}

// ---------- tool invocation (engine -> us) ----------

/// What the engine POSTs to our tool URL. Parsed defensively: the exact
/// envelope shape is documented on the "Tool request" page; we accept the
/// call id from several positions and args from several nests, and require
/// nothing beyond that.
#[derive(Debug)]
pub struct ToolInvocation {
    pub call_id: String,
    pub args: Value,
}

pub fn parse_tool_invocation(body: &Value) -> Result<ToolInvocation, String> {
    let call_id = body
        .get("call")
        .and_then(|c| c.get("id"))
        .and_then(|v| v.as_str())
        .or_else(|| body.get("call_id").and_then(|v| v.as_str()))
        .or_else(|| body.get("callId").and_then(|v| v.as_str()))
        .unwrap_or("unknown-call")
        .to_string();
    let args = body
        .get("arguments")
        .or_else(|| body.get("args"))
        .or_else(|| body.get("params"))
        .or_else(|| body.get("parameters"))
        .cloned()
        .unwrap_or_else(|| {
            // maybe the body IS the args object (has no envelope at all)
            if body.is_object() {
                body.clone()
            } else {
                json!({})
            }
        });
    Ok(ToolInvocation { call_id, args })
}

/// Deterministic idempotency key: call.id + tool + canonicalized args.
/// Write tools use this as the row id with INSERT OR IGNORE, so engine
/// retries can never double-record.
pub fn idempotency_key(call_id: &str, tool: &str, args: &Value) -> String {
    let mut h = DefaultHasher::new();
    call_id.hash(&mut h);
    tool.hash(&mut h);
    serde_json::to_string(args).unwrap_or_default().hash(&mut h);
    format!("tool-{:016x}", h.finish())
}

/// Enforce the 8KB response cap. Truncates arrays with an explicit marker
/// rather than letting the engine refuse the whole body.
pub fn cap_response_size(v: Value) -> Value {
    fn size(v: &Value) -> usize {
        serde_json::to_string(v)
            .map(|s| s.len())
            .unwrap_or(usize::MAX)
    }
    if size(&v) <= 8192 {
        return v;
    }
    let mut v = v;
    // Iteratively halve every array until we fit.
    for _ in 0..8 {
        fn halve(v: &mut Value) -> bool {
            match v {
                Value::Array(a) if a.len() > 1 => {
                    a.truncate(a.len().div_ceil(2));
                    true
                }
                Value::Array(_) => false,
                Value::Object(o) => {
                    let mut c = false;
                    for (_, vv) in o.iter_mut() {
                        c |= halve(vv);
                    }
                    c
                }
                _ => false,
            }
        }
        let changed = halve(&mut v);
        if size(&v) <= 8192 {
            break;
        }
        if !changed {
            break;
        }
    }
    if let Value::Object(o) = &mut v {
        o.insert("_truncated".into(), json!(true));
        o.insert(
            "_note".into(),
            json!("response truncated to fit the 8KB tool limit"),
        );
    }
    v
}

#[cfg(test)]
mod adapter_tests {
    use super::*;

    #[test]
    fn test_parse_tool_invocation_envelopes() {
        // canonical envelope
        let b = json!({"call": {"id": "call_123"}, "arguments": {"claim_text": "x"}});
        let inv = parse_tool_invocation(&b).unwrap();
        assert_eq!(inv.call_id, "call_123");
        assert_eq!(inv.args["claim_text"], "x");
        // flat envelope
        let b = json!({"call_id": "c9", "params": {"a": 1}});
        let inv = parse_tool_invocation(&b).unwrap();
        assert_eq!(inv.call_id, "c9");
        // bare args
        let b = json!({"claim_text": "y"});
        let inv = parse_tool_invocation(&b).unwrap();
        assert_eq!(inv.args["claim_text"], "y");
    }

    #[test]
    fn test_idempotency_key_stable() {
        let a = idempotency_key("call1", "record_observation", &json!({"text": "saw it"}));
        let b = idempotency_key("call1", "record_observation", &json!({"text": "saw it"}));
        let c = idempotency_key("call2", "record_observation", &json!({"text": "saw it"}));
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_response_cap_truncates() {
        let big: Vec<Value> = (0..500)
            .map(|i| json!({"i": i, "pad": "x".repeat(100)}))
            .collect();
        let v = json!({"evidence": big});
        let capped = cap_response_size(v);
        let s = serde_json::to_string(&capped).unwrap();
        assert!(s.len() <= 8192, "still too big: {}", s.len());
        assert_eq!(capped["_truncated"], true);
        // small responses pass through untouched
        let small = json!({"ok": true});
        assert_eq!(cap_response_size(small.clone()), small);
    }

    #[test]
    fn test_config_from_env() {
        std::env::remove_var("ALEBEX_ENGINE_TOKEN");
        let cfg = AlebexConfig::from_env();
        assert!(!cfg.engine_ready());
    }
}

// ============================================================================
// LIVE VOICE GATEWAY (Track A) — thinnest vertical slice.
// Browser --WS--> Rust gateway --WS--> Alebex engine. The engine token never
// leaves the server. Mic: PCM16 16kHz mono 20ms frames. Agent audio: PCM16
// 24kHz. Mark semantics: engine marks are HELD until the browser echoes them
// after its audio queue drains; clear_audio drops everything held.
// ============================================================================

use futures_util::{SinkExt, StreamExt};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallState {
    Idle,
    Connecting,
    Started,
    Speaking,
    Listening,
    Ending,
    Ended,
    Failed,
}

/// The wire between voice and avatar: one audio stream, two consumers.
/// The browser playback path consumes audio for the avatar today; this enum
/// is the Rust-side contract for future in-process consumers.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum VoiceEvent {
    AudioFrame {
        samples: Vec<i16>,
        timestamp_ms: u64,
    },
    Transcript {
        text: String,
        is_final: bool,
    },
    Mark {
        id: String,
    },
    CallEnded,
}

pub enum EngineOut {
    Text(String),
    Binary(Vec<u8>),
    MarkEcho(String),
    Close,
}

pub enum BrowserOut {
    Text(String),
    Binary(Vec<u8>),
    ClearAudio,
    Close,
}

pub struct LiveSessionHandle {
    pub id: String,
    pub state: StdMutex<CallState>,
    pub created_at_ms: u64,
    pub engine_tx: mpsc::UnboundedSender<EngineOut>,
    pub browser_tx: StdMutex<Option<mpsc::UnboundedSender<BrowserOut>>>,
}

#[derive(Default)]
pub struct LiveRegistry {
    pub sessions: HashMap<String, Arc<LiveSessionHandle>>,
}

impl LiveRegistry {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub fn prune(&mut self, now_ms: u64) {
        let mut orphans: Vec<Arc<LiveSessionHandle>> = Vec::new();
        self.sessions.retain(|_, h| {
            let keep = now_ms.saturating_sub(h.created_at_ms) < 120_000
                || h.browser_tx.lock().unwrap().is_some();
            if !keep {
                orphans.push(h.clone());
            }
            keep
        });
        // Pruned sessions held an open engine WS: close it so orphaned
        // engine calls don't burn quota after the browser is gone.
        for h in orphans {
            let _ = h.engine_tx.send(EngineOut::Close);
        }
    }
}

/// Session-creation rate limit: each creation dials the engine (billable).
/// A global minimum interval stops a client from spinning up calls in a loop.
/// Testable pure gate over an explicit clock.
pub fn session_creation_allowed(last_created_ms: u64, now_ms: u64) -> bool {
    last_created_ms == 0 || now_ms.saturating_sub(last_created_ms) >= 3000
}

static LAST_SESSION_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn set_state(handle: &Arc<LiveSessionHandle>, s: CallState) {
    *handle.state.lock().unwrap() = s;
}

fn send_browser(handle: &Arc<LiveSessionHandle>, msg: BrowserOut) {
    if let Some(tx) = handle.browser_tx.lock().unwrap().as_ref() {
        let _ = tx.send(msg);
    }
}

fn bytes_to_i16(b: &[u8]) -> Vec<i16> {
    b.chunks(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect()
}

#[derive(Debug, Serialize)]
pub struct LiveSessionInfo {
    pub session_id: String,
    pub ws_url: String,
    pub state: CallState,
}

/// Create a live session: dial the engine with the server-side token,
/// send start_call, and hand the browser an ephemeral session id.
/// The browser NEVER sees the engine token.
pub async fn create_live_session(
    state: &Arc<crate::governor::AppState>,
) -> Result<LiveSessionInfo, AlebexError> {
    let cfg = AlebexConfig::from_env();
    let ws_url = cfg
        .engine_ws_url
        .clone()
        .ok_or_else(|| AlebexError::Auth("ALEBEX_ENGINE_WS_URL not configured".into()))?;
    let token = cfg
        .engine_token
        .clone()
        .ok_or_else(|| AlebexError::Auth("ALEBEX_ENGINE_TOKEN not configured".into()))?;
    let agent_id = cfg
        .agent_id
        .clone()
        .ok_or_else(|| AlebexError::Auth("ALEBEX_AGENT_ID not configured".into()))?;

    let now = crate::receipts::now_ms();
    state.live.lock().unwrap().prune(now);

    // Outermost gate: don't let anyone spin the engine dial in a loop.
    let last = LAST_SESSION_MS.swap(now, std::sync::atomic::Ordering::SeqCst);
    if !session_creation_allowed(last, now) {
        return Err(AlebexError::Retryable(
            "session creation rate-limited: retry in a few seconds".into(),
        ));
    }

    let req = axum::http::Request::builder()
        .uri(ws_url)
        .header("Authorization", format!("Bearer {}", token))
        .body(())
        .map_err(|e| AlebexError::Transport(e.to_string()))?;
    let (engine_ws, _) = tokio_tungstenite::connect_async(req)
        .await
        .map_err(|e| AlebexError::Transport(format!("engine dial failed: {}", e)))?;

    let (engine_tx, engine_rx) = mpsc::unbounded_channel::<EngineOut>();
    let id = uuid::Uuid::new_v4().to_string();
    let handle = Arc::new(LiveSessionHandle {
        id: id.clone(),
        state: StdMutex::new(CallState::Connecting),
        created_at_ms: now,
        engine_tx,
        browser_tx: StdMutex::new(None),
    });
    state
        .live
        .lock()
        .unwrap()
        .sessions
        .insert(id.clone(), handle.clone());

    let _ = handle.engine_tx.send(EngineOut::Text(
        json!({"type": "start_call", "agent": {"id": agent_id}}).to_string(),
    ));

    let state2 = state.clone();
    tokio::spawn(async move {
        engine_loop(state2, handle, engine_ws, engine_rx).await;
    });

    Ok(LiveSessionInfo {
        session_id: id.clone(),
        ws_url: format!("/api/alebex/call/{}", id),
        state: CallState::Connecting,
    })
}

type EngineWs =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn engine_loop(
    state: Arc<crate::governor::AppState>,
    handle: Arc<LiveSessionHandle>,
    engine_ws: EngineWs,
    mut engine_rx: mpsc::UnboundedReceiver<EngineOut>,
) {
    use tokio_tungstenite::tungstenite::Message as TMsg;
    let (mut sink, mut stream) = engine_ws.split();
    let mut held_marks: VecDeque<String> = VecDeque::new();
    set_state(&handle, CallState::Started);

    loop {
        tokio::select! {
            msg = stream.next() => {
                match msg {
                    Some(Ok(TMsg::Text(t))) => {
                        handle_engine_text(&state, &handle, &t.to_string(), &mut held_marks).await;
                    }
                    Some(Ok(TMsg::Binary(b))) => {
                        // Agent audio: PCM16 24kHz -> browser, and the avatar wire.
                        // Cap: a rogue engine must not be able to OOM the browser
                        // tab with one giant frame (20ms @24kHz = 960 bytes;
                        // 256KB is ~5.5s of audio — generous).
                        if b.len() > 262_144 {
                            tracing::warn!(bytes = b.len(), "engine audio frame oversized: dropped");
                            continue;
                        }
                        let samples = bytes_to_i16(&b);
                        tracing::debug!(samples = samples.len(), "voice audio frame");
                        let _evt = VoiceEvent::AudioFrame { samples, timestamp_ms: crate::receipts::now_ms() };
                        send_browser(&handle, BrowserOut::Binary(b.to_vec()));
                        set_state(&handle, CallState::Speaking);
                    }
                    Some(Ok(TMsg::Ping(p))) => { let _ = sink.send(TMsg::Pong(p)).await; }
                    Some(Ok(TMsg::Close(_))) | None => break,
                    _ => {}
                }
            }
            cmd = engine_rx.recv() => {
                match cmd {
                    Some(EngineOut::Text(t)) => { if sink.send(TMsg::Text(t)).await.is_err() { break; } }
                    Some(EngineOut::Binary(b)) => { if sink.send(TMsg::Binary(b)).await.is_err() { break; } }
                    Some(EngineOut::MarkEcho(id)) => { release_mark(&handle, &mut held_marks, &id); }
                    Some(EngineOut::Close) | None => break,
                }
            }
        }
    }
    set_state(&handle, CallState::Ended);
    send_browser(&handle, BrowserOut::Close);
    state.live.lock().unwrap().sessions.remove(&handle.id);
}

async fn handle_engine_text(
    state: &Arc<crate::governor::AppState>,
    handle: &Arc<LiveSessionHandle>,
    text: &str,
    held_marks: &mut VecDeque<String>,
) {
    let v: Value = serde_json::from_str(text).unwrap_or(json!({}));
    match v.get("type").and_then(|t| t.as_str()) {
        Some("transcript") | Some("conversation_message") => {
            let txt = v
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            let is_final = v
                .get("final")
                .and_then(|f| f.as_bool())
                .or_else(|| v.get("is_final").and_then(|f| f.as_bool()))
                .unwrap_or(false);
            send_browser(handle, BrowserOut::Text(text.to_string()));
            if is_final && !txt.trim().is_empty() {
                tap_transcript(state, &handle.id, &txt).await;
                set_state(handle, CallState::Listening);
            }
        }
        // MARK: held. Only released when the browser echoes after its audio
        // queue drains. Never forwarded early. Mark ids are engine-issued;
        // cap length so a rogue engine can't stuff the hold queue.
        Some("mark") => {
            if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                if id.len() > 128 {
                    tracing::warn!("oversized mark id dropped");
                    return;
                }
                held_marks.push_back(id.to_string());
                send_browser(
                    handle,
                    BrowserOut::Text(json!({"type": "mark_pending", "id": id}).to_string()),
                );
            }
        }
        Some("clear_audio") => {
            // Kill everything held before returning held marks: drop them.
            held_marks.clear();
            send_browser(handle, BrowserOut::ClearAudio);
        }
        Some("call_ended") | Some("call_end") => {
            send_browser(handle, BrowserOut::Text(text.to_string()));
            set_state(handle, CallState::Ending);
        }
        _ => {
            send_browser(handle, BrowserOut::Text(text.to_string()));
        }
    }
}

/// Release a held mark once the browser confirms its audio queue drained.
/// Marks release FIFO: audio completes in order, so marks echo in order.
pub fn release_mark(handle: &Arc<LiveSessionHandle>, held_marks: &mut VecDeque<String>, id: &str) {
    if held_marks.iter().any(|m| m == id) {
        held_marks.retain(|m| m != id);
        let _ = handle.engine_tx.send(EngineOut::Text(
            json!({"type": "mark", "id": id}).to_string(),
        ));
    }
}

/// Transcript tap: every final user transcript becomes a diarized, timestamped
/// turn with a governor receipt — the live call joins the same audit trail.
/// Engine text is untrusted input: cap length before it touches the governor
/// or the store (a rogue engine must not be able to stuff either).
pub fn truncate_transcript(text: &str) -> &str {
    const MAX: usize = 4000;
    if text.len() <= MAX {
        return text;
    }
    let mut end = MAX;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

async fn tap_transcript(state: &Arc<crate::governor::AppState>, session_id: &str, text: &str) {
    let text = truncate_transcript(text);
    use crate::governor::{classify_intent, AiProposal};
    let t0 = crate::receipts::now_ms();
    let request_id = uuid::Uuid::new_v4().to_string();
    let intent = classify_intent(text);
    let proposal = AiProposal {
        intent: format!("{:?}", intent),
        claim: None,
        hypothesis: None,
        requested_action: None,
        response_style: None,
        authority: None,
        verified: None,
        execute: None,
        claim_status: None,
    };
    let decision = state
        .governor
        .decide(&request_id, session_id, text, proposal, &[]);
    let t1 = crate::receipts::now_ms();
    let store = state.store.lock().unwrap();
    let _ = store.ensure_speaker("spk0", "SPEAKER_00", "user", t0);
    let _ = store.ensure_speaker("live-agent", "AGENT", "agent", t0);
    let _ = store.create_session(session_id, t0);
    let epistemic = format!("{:?}", decision.epistemic_status).to_uppercase();
    let _ = store.record_turn(
        &format!("{}-live-u", request_id),
        session_id,
        "spk0",
        t0,
        t1,
        text,
        &format!("{:?}", intent),
        Some(&epistemic),
        Some(&decision.claim_id),
        Some(&decision.receipt.receipt_id),
        t1.saturating_sub(t0),
        "alebex-live",
    );
    let r = &decision.receipt;
    let _ = store.record_receipt(
        &r.receipt_id,
        &r.request_id,
        session_id,
        r.timestamp_ms,
        &r.actor,
        "live.transcript",
        &r.input_hash,
        &r.decision,
        r.epistemic_status.as_deref(),
        &r.output_hash,
        r.duration_ms,
        r.success,
        &r.prev_hash,
    );
}

#[cfg(test)]
mod gateway_tests {
    use super::*;

    #[test]
    fn test_mark_hold_and_release_order() {
        // Marks must release FIFO: audio completes in order.
        let mut held: VecDeque<String> = VecDeque::new();
        held.push_back("m1".into());
        held.push_back("m2".into());
        // simulate release of m1
        held.retain(|m| m != "m1");
        assert_eq!(held.len(), 1);
        assert_eq!(held[0], "m2");
        // releasing an unknown mark is a no-op, never an error
        held.retain(|m| m != "mX");
        assert_eq!(held.len(), 1);
    }

    #[test]
    fn test_clear_audio_drops_held_marks() {
        let mut held: VecDeque<String> = VecDeque::new();
        held.push_back("m1".into());
        held.push_back("m2".into());
        held.clear(); // clear_audio semantics
        assert!(held.is_empty());
    }

    #[test]
    fn test_pcm16_frame_math() {
        // 20ms @ 16kHz mono = 320 samples = 640 bytes. The browser must send
        // exactly this; the gateway trusts but verifies length downstream.
        let frame = vec![0u8; 640];
        let samples = bytes_to_i16(&frame);
        assert_eq!(samples.len(), 320);
        // 24kHz agent audio frame example: 20ms = 480 samples = 960 bytes
        assert_eq!(bytes_to_i16(&vec![0u8; 960]).len(), 480);
    }

    #[test]
    fn test_session_requires_credentials() {
        // Without env credentials, session creation fails CLOSED with a
        // typed error — never a half-open session, never a panic.
        std::env::remove_var("ALEBEX_ENGINE_WS_URL");
        std::env::remove_var("ALEBEX_ENGINE_TOKEN");
        std::env::remove_var("ALEBEX_AGENT_ID");
        let cfg = AlebexConfig::from_env();
        assert!(!cfg.engine_ready());
        assert!(cfg.engine_ws_url.is_none());
    }

    #[test]
    fn test_call_state_machine_values() {
        // Pin the states the UI and tests may match on.
        let states = [
            CallState::Idle,
            CallState::Connecting,
            CallState::Started,
            CallState::Speaking,
            CallState::Listening,
            CallState::Ending,
            CallState::Ended,
            CallState::Failed,
        ];
        assert_eq!(states.len(), 8);
    }
}

#[cfg(test)]
mod mark_semantics_tests {
    use super::*;
    use crate::governor::AppState;

    fn test_handle() -> (
        Arc<AppState>,
        Arc<LiveSessionHandle>,
        mpsc::UnboundedReceiver<EngineOut>,
        mpsc::UnboundedReceiver<BrowserOut>,
    ) {
        std::env::set_var("INDY_DB", ":memory:");
        let state = Arc::new(AppState::new());
        let (etx, erx) = mpsc::unbounded_channel::<EngineOut>();
        let (btx, brx) = mpsc::unbounded_channel::<BrowserOut>();
        let handle = Arc::new(LiveSessionHandle {
            id: "test-call".into(),
            state: StdMutex::new(CallState::Started),
            created_at_ms: crate::receipts::now_ms(),
            engine_tx: etx,
            browser_tx: StdMutex::new(Some(btx)),
        });
        (state, handle, erx, brx)
    }

    #[tokio::test]
    async fn test_mark_held_until_drain_echo() {
        let (state, handle, mut erx, mut brx) = test_handle();
        let mut held = VecDeque::new();

        // Engine: audio, then a mark.
        handle_engine_text(&state, &handle, r#"{"type":"mark","id":"m1"}"#, &mut held).await;
        assert_eq!(held.len(), 1, "mark must be held");

        // Browser sees mark_pending; the engine must see NOTHING yet.
        let got = brx.try_recv().expect("browser should get mark_pending");
        assert!(matches!(got, BrowserOut::Text(t) if t.contains("mark_pending")));
        assert!(
            erx.try_recv().is_err(),
            "FATAL: mark forwarded before drain"
        );

        // Browser drains -> echo -> engine finally receives the mark.
        release_mark(&handle, &mut held, "m1");
        assert!(held.is_empty());
        match erx.try_recv().expect("engine should get the echo") {
            EngineOut::Text(t) => assert!(t.contains("\"id\":\"m1\""), "echo carries the mark id"),
            _ => panic!("expected text mark echo"),
        }
    }

    #[tokio::test]
    async fn test_clear_audio_kills_held_marks() {
        let (state, handle, mut erx, mut brx) = test_handle();
        let mut held = VecDeque::new();

        handle_engine_text(&state, &handle, r#"{"type":"mark","id":"m2"}"#, &mut held).await;
        assert_eq!(held.len(), 1);
        // clear_audio: kill every held mark BEFORE any echo can return.
        handle_engine_text(&state, &handle, r#"{"type":"clear_audio"}"#, &mut held).await;
        assert!(held.is_empty(), "clear_audio must drop held marks");
        assert!(erx.try_recv().is_err(), "FATAL: a mark escaped after clear");

        // A late echo for the cleared mark is a no-op.
        release_mark(&handle, &mut held, "m2");
        assert!(erx.try_recv().is_err());

        // Browser was told to drop its queue.
        let mut saw_clear = false;
        while let Ok(m) = brx.try_recv() {
            if matches!(m, BrowserOut::ClearAudio) {
                saw_clear = true;
            }
        }
        assert!(saw_clear, "browser must receive clear_audio");
    }

    #[tokio::test]
    async fn test_final_transcript_taps_governor() {
        let (state, handle, _erx, mut brx) = test_handle();
        let mut held = VecDeque::new();
        handle_engine_text(
            &state,
            &handle,
            r#"{"type":"transcript","text":"is the lake creature real?","final":true}"#,
            &mut held,
        )
        .await;
        // Transcript forwarded to the browser AND recorded with a receipt.
        let mut saw_text = false;
        while let Ok(m) = brx.try_recv() {
            if let BrowserOut::Text(t) = m {
                if t.contains("lake creature") {
                    saw_text = true;
                }
            }
        }
        assert!(saw_text);
        let store = state.store.lock().unwrap();
        let turns = store.transcript(&handle.id).unwrap();
        assert!(
            !turns.is_empty(),
            "live transcript must be diarized + stored"
        );
        assert!(
            turns[0].1.contains("lake creature"),
            "transcript text recorded"
        );
    }
    #[tokio::test]
    async fn test_oversized_mark_id_dropped() {
        let (state, handle, _erx, mut brx) = test_handle();
        let mut held = VecDeque::new();
        let big_id = "m".repeat(5000);
        let msg = serde_json::json!({"type": "mark", "id": big_id}).to_string();
        handle_engine_text(&state, &handle, &msg, &mut held).await;
        assert!(
            held.is_empty(),
            "oversized mark id must not enter the hold queue"
        );
        assert!(
            brx.try_recv().is_err(),
            "no mark_pending for a dropped mark"
        );
        // boundary: 128 chars is accepted
        let ok_id = "m".repeat(128);
        let msg = serde_json::json!({"type": "mark", "id": ok_id}).to_string();
        handle_engine_text(&state, &handle, &msg, &mut held).await;
        assert_eq!(held.len(), 1);
    }

    #[tokio::test]
    async fn test_transcript_truncated_before_store() {
        let (state, handle, _erx, _brx) = test_handle();
        let mut held = VecDeque::new();
        let huge = "x".repeat(100_000);
        let msg =
            serde_json::json!({"type": "transcript", "text": huge, "final": true}).to_string();
        handle_engine_text(&state, &handle, &msg, &mut held).await;
        let store = state.store.lock().unwrap();
        let turns = store.transcript(&handle.id).unwrap();
        assert!(!turns.is_empty());
        assert!(
            turns[0].1.len() <= 4000,
            "transcript must be capped, got {}",
            turns[0].1.len()
        );
    }

    #[test]
    fn test_truncate_transcript_char_boundary() {
        // multi-byte chars must not be split
        let s = "é".repeat(5000);
        let t = truncate_transcript(&s);
        assert!(t.len() <= 4000);
        assert!(t.chars().count() <= 2000);
        let short = "hello";
        assert_eq!(truncate_transcript(short), short);
    }

    #[test]
    fn test_session_creation_rate_gate() {
        assert!(
            session_creation_allowed(0, 1000),
            "first creation always allowed"
        );
        assert!(
            !session_creation_allowed(1000, 1000),
            "immediate repeat blocked"
        );
        assert!(
            !session_creation_allowed(1000, 3999),
            "2.999s still blocked"
        );
        assert!(session_creation_allowed(1000, 4000), "3s gap allowed");
    }

    #[test]
    fn test_prune_closes_orphan_engine_connections() {
        let (_state, handle, erx, _brx) = test_handle();
        drop(erx); // we only need the sender side on the handle; recreate:
        let (etx, mut erx2) = mpsc::unbounded_channel::<EngineOut>();
        let h2 = Arc::new(LiveSessionHandle {
            id: "old-call".into(),
            state: StdMutex::new(CallState::Started),
            created_at_ms: 0, // ancient
            engine_tx: etx,
            browser_tx: StdMutex::new(None), // no browser attached
        });
        let mut reg = LiveRegistry::new();
        reg.sessions.insert("old-call".into(), h2);
        reg.prune(crate::receipts::now_ms());
        assert!(
            !reg.sessions.contains_key("old-call"),
            "orphan pruned from map"
        );
        match erx2
            .try_recv()
            .expect("orphan engine connection must be closed")
        {
            EngineOut::Close => {}
            _ => panic!("expected Close for pruned orphan"),
        }
        let _ = handle; // keep the other handle alive
    }

    #[test]
    fn test_config_debug_redacts_tokens() {
        let cfg = AlebexConfig {
            bearer_token: Some("sk-live-SUPERSECRET".into()),
            engine_token: Some("wt_live_TOKENTOKENTOKEN".into()),
            agent_id: Some("agent1".into()),
            public_url: "https://x.example.com".into(),
            engine_ws_url: None,
            tool_secret: Some("toolsecret123".into()),
        };
        let dbg = format!("{:?}", cfg);
        assert!(
            !dbg.contains("SUPERSECRET"),
            "bearer token leaked in Debug: {}",
            dbg
        );
        assert!(
            !dbg.contains("TOKENTOKENTOKEN"),
            "engine token leaked in Debug: {}",
            dbg
        );
        assert!(
            !dbg.contains("toolsecret123"),
            "tool secret leaked in Debug: {}",
            dbg
        );
        assert!(dbg.contains("<redacted>"));
    }

    #[test]
    fn test_same_channel_distinguishes_takeover() {
        // Pins the primitive the browser_loop takeover guard relies on.
        let (tx1, _rx1) = mpsc::unbounded_channel::<EngineOut>();
        let (tx2, _rx2) = mpsc::unbounded_channel::<EngineOut>();
        assert!(tx1.same_channel(&tx1));
        assert!(!tx1.same_channel(&tx2));
    }
}
