//! HTTP + WebSocket API.
//!
//! API contract version. Bump when a route is added/removed or a request/
//! response schema changes in a breaking way. Surfaced on /health and baked
//! into the release bundle (E4) so a running artifact's contract can be
//! compared against the source it claims to be.
pub const CONTRACT_VERSION: &str = "1";

use crate::ai::AiClient;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use indy_reads::alebex::{
    create_live_session, custom_tools_json, AlebexError, BrowserOut, CallState, EngineOut,
};
use indy_reads::avatar::{derive_avatar, viseme_timeline, AvatarState};
use indy_reads::diarize::{speakers, Diarizer};
use indy_reads::domain::{EpistemicStatus, Evidence, Session};
use indy_reads::governor::{classify_intent, AppState};
use indy_reads::mcp;
use indy_reads::receipts::{now_ms, ClaimRevision, Receipt};
use indy_reads::theater::{Novelty, TheaterCue, TheaterEngine, TheaterEvent};
use indy_reads::voice::{
    attempt_clone, decide_clone, derive_voice_full, CloneDecision, CloneError, ScullyEra, Tone,
    VoiceProvenance, VoiceState,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex as StdMutex};

/// Client-supplied session ids are keys into an in-memory map and a SQLite
/// path component. Constrain charset/length: no traversal, no 10MB keys.
fn is_safe_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

#[derive(Debug, Deserialize)]
struct TurnRequest {
    session_id: Option<String>,
    utterance: String,
    persona: Option<String>,
    era: Option<String>,
    tone: Option<String>,
    clone: Option<CloneParams>,
    /// Live truth reconciliation: the client detected a user correction
    /// (fast lane) and is declaring it explicitly. The governor emits a
    /// ClaimRevision receipt and invalidates pending output. Fields are
    /// plain strings — no model inference on either side.
    correction_of: Option<CorrectionOf>,
}

/// A user correction of a claim, declared by the client or detected
/// server-side from deterministic patterns. old_value may be "unspecified"
// — the retraction is recorded regardless.
#[derive(Debug, Clone, Deserialize)]
struct CorrectionOf {
    claim: String,
    old_value: String,
    new_value: String,
}

/// Deterministic correction detection: string patterns only, never model
/// inference. Returns the candidate new value (text after the pattern).
/// Backstop for clients that don't pre-declare correction_of (typed turns).
fn detect_correction(utterance: &str) -> Option<String> {
    let lower = utterance.to_lowercase();
    // Order matters: check the strongest signals first.
    for pat in [
        "no, sorry",
        "no sorry",
        "i meant",
        "correction:",
        "correction -",
    ] {
        if let Some(i) = lower.find(pat) {
            let rest = utterance[i + pat.len()..]
                .trim()
                .trim_matches(|c| c == ',' || c == ':' || c == '-')
                .trim();
            return Some(if rest.is_empty() {
                "unspecified".to_string()
            } else {
                rest.chars().take(120).collect()
            });
        }
    }
    // "actually, X" / "actually X" at turn start.
    for pat in ["actually, ", "actually "] {
        if lower.starts_with(pat) {
            let rest = utterance[pat.len()..].trim();
            return Some(if rest.is_empty() {
                "unspecified".to_string()
            } else {
                rest.chars().take(120).collect()
            });
        }
    }
    None
}

/// Optional voice-clone request. Consent is a DECLARED string parsed by
/// voice::parse_consent — only "owner:<ms>" / "licensed:<ref>" are accepted.
/// There is no field, flag, or string that can request cloning a real
/// living person; unparseable consent is a refusal, not an error.
#[derive(Debug, Deserialize)]
struct CloneParams {
    /// "chatterbox" | "voxcpm2" | "xtts-v2" (default: chatterbox).
    backend: Option<String>,
    /// Path to the consented reference WAV.
    reference_wav: Option<String>,
    /// "owner:<recorded_at_ms>" | "licensed:<licence_ref>".
    consent: Option<String>,
}

/// Cloned audio served back to the client: base64 PCM16 mono, little-endian.
#[derive(Debug, Serialize)]
struct CloneAudioView {
    backend: String,
    sample_rate: u32,
    samples: usize,
    pcm16_base64: String,
}

/// Minimal base64 (standard alphabet). No new dependency for one field.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPH: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPH[((n >> 18) & 63) as usize] as char);
        out.push(ALPH[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPH[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPH[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[derive(Debug, Serialize)]
struct TurnResponse {
    request_id: String,
    session_id: String,
    reply: String,
    epistemic_status: EpistemicStatus,
    response_mode: String,
    governor_reason: String,
    rejected_fields: Vec<String>,
    decision: GovernorDecisionLite,
    latency_ms: u64,
    fixture: bool,
    /// Governor-selected provider for this turn ("fixture" | "refused" |
    /// provider name). The governor — not app convention — picked it.
    provider: String,
    /// (provider, outcome-kind) for every generate() attempt, in order.
    provider_trail: Vec<String>,
    persona: String,
    avatar: AvatarView,
    voice: VoiceState,
    provenance: VoiceProvenance,
    continuity: Option<ContinuityNote>,
    theater: Vec<TheaterCue>,
    /// "off" | "synthesized" | "fallback" | "refused".
    clone_status: String,
    clone_audio: Option<CloneAudioView>,
    /// True when this turn carried a user correction: the fast lane already
    /// stopped speech client-side; this confirms the governor invalidated
    /// the pending output and recorded the revision.
    speech_interrupted: bool,
    /// The recorded claim revision, if this turn was a correction.
    revision: Option<ClaimRevision>,
}

/// Graph continuity: the same entity across time. If this exact claim was
/// assessed before, say so — with the prior verdict and when. Memory is the
/// store; continuity is surfacing it.
#[derive(Debug, Serialize)]
struct ContinuityNote {
    seen_before: bool,
    previous_state: Option<String>,
    previous_at_ms: Option<i64>,
    evidence_items_now: usize,
}

#[derive(Debug, Serialize)]
struct AvatarView {
    emotion: String,
    gesture: String,
    epistemic_overlay: Option<String>,
    speaking: bool,
    /// (t_ms, viseme) timeline for the reply text.
    visemes: Vec<(u64, String)>,
}

impl From<&AvatarState> for AvatarView {
    fn from(s: &AvatarState) -> Self {
        Self {
            emotion: format!("{:?}", s.emotion),
            gesture: format!("{:?}", s.gesture),
            epistemic_overlay: s.epistemic_overlay.clone(),
            speaking: s.speaking,
            visemes: vec![],
        }
    }
}

#[derive(Debug, Serialize)]
struct GovernorDecisionLite {
    allowed: bool,
    reason: String,
    epistemic_status: EpistemicStatus,
    response_mode: String,
}

pub fn router(state: Arc<AppState>) -> Router {
    // RT-SWEEP-2026-10-08-003 parity: the JS (Vercel) handlers got fail-closed
    // inbound auth; the native server was wide open on 0.0.0.0. All API
    // routes now require the router key. /health stays open (standard
    // liveness probe; returns no sensitive data) and static web/ stays open
    // (UI shell).
    let api = Router::new()
        .route("/api/turn", post(turn))
        .route("/api/session/:id", get(get_session))
        .route("/api/transcript/:id", get(transcript))
        .route("/api/alebex/tools", get(alebex_tools))
        .route("/api/alebex/session", get(alebex_session))
        .route("/api/alebex/call/:id", get(alebex_call_ws))
        .route("/api/receipts", get(receipts))
        .route("/api/mcp/tools", get(mcp_tools))
        .route("/api/mcp/:tool", post(mcp_call))
        .route("/ws", get(ws_handler))
        .layer(axum::middleware::from_fn(require_api_key))
        .with_state(state);
    Router::new()
        .route("/health", get(health))
        .merge(api)
        .nest_service("/", ServeDir::new("web"))
        .layer(CorsLayer::permissive())
}

/// Fail-closed inbound auth, mirroring api/_auth.js.
/// Key material: ~/workspace/keys/ROUTER_KEY file first, then ROUTER_KEY env.
/// Carriers: `x-api-key` header, `Authorization: Bearer`, `?key=` query.
/// No key configured => 500 (never open). Mismatch => 401. Timing-safe compare.
async fn require_api_key(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> impl IntoResponse {
    use std::io::Read as _;
    fn expected_key() -> Option<String> {
        for p in [
            std::env::var("HOME")
                .ok()
                .map(|h| format!("{}/workspace/keys/ROUTER_KEY", h)),
            Some("/home/hatch/workspace/keys/ROUTER_KEY".to_string()),
        ]
        .into_iter()
        .flatten()
        {
            if let Ok(mut f) = std::fs::File::open(&p) {
                let mut s = String::new();
                if f.read_to_string(&mut s).is_ok() {
                    let k = s.trim().to_string();
                    if !k.is_empty() {
                        return Some(k);
                    }
                }
            }
        }
        std::env::var("ROUTER_KEY")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }
    fn constant_time_eq(a: &str, b: &str) -> bool {
        let ab = a.as_bytes();
        let bb = b.as_bytes();
        if ab.len() != bb.len() {
            return false;
        }
        let mut diff = 0u8;
        for (x, y) in ab.iter().zip(bb.iter()) {
            diff |= x ^ y;
        }
        diff == 0
    }
    let expected = match expected_key() {
        Some(k) => k,
        None => return (StatusCode::INTERNAL_SERVER_ERROR, "auth not configured").into_response(),
    };
    let headers = req.headers();
    let mut got: Option<String> = None;
    if let Some(h) = headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        let t = h.trim();
        if !t.is_empty() {
            got = Some(t.to_string());
        }
    }
    if got.is_none() {
        if let Some(auth) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
            let t = auth.trim();
            if let Some(rest) = t
                .strip_prefix("Bearer ")
                .or_else(|| t.strip_prefix("bearer "))
            {
                let k = rest.trim();
                if !k.is_empty() {
                    got = Some(k.to_string());
                }
            } else if t.to_lowercase().starts_with("bearer") {
                // "Bearer" with irregular whitespace
                let k = t[6..].trim();
                if !k.is_empty() {
                    got = Some(k.to_string());
                }
            }
        }
    }
    if got.is_none() {
        if let Some(q) = req.uri().query() {
            for pair in q.split('&') {
                let mut kv = pair.splitn(2, '=');
                if kv.next() == Some("key") {
                    if let Some(v) = kv.next() {
                        // minimal percent-decode for the key param
                        let dec = percent_decode(v);
                        if !dec.trim().is_empty() {
                            got = Some(dec.trim().to_string());
                        }
                    }
                    break;
                }
            }
        }
    }
    match got {
        Some(k) if constant_time_eq(&k, &expected) => next.run(req).await,
        _ => (StatusCode::UNAUTHORIZED, "unauthorized").into_response(),
    }
}

/// Minimal percent-decode (only what a key param needs).
fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && i + 2 < bytes.len() {
            let hex = |c: u8| match c {
                b'0'..=b'9' => c - b'0',
                b'a'..=b'f' => c - b'a' + 10,
                b'A'..=b'F' => c - b'A' + 10,
                _ => 0,
            };
            // Only decode if both are valid hex; otherwise emit literally.
            let hi = bytes[i + 1];
            let lo = bytes[i + 2];
            let valid = hi.is_ascii_hexdigit() && lo.is_ascii_hexdigit();
            if valid {
                out.push((hex(hi) * 16 + hex(lo)) as char);
                i += 3;
                continue;
            }
        }
        if b == b'+' {
            out.push(' ');
        } else {
            out.push(b as char);
        }
        i += 1;
    }
    out
}

/// Health + identity. The provenance block lets any reviewer verify that the
/// running artifact matches a known source commit: compare `commit` here
/// against `git rev-parse HEAD` in the source repo. Mismatch => the runtime
/// is NOT the artifact you think it is (source truth != runtime truth).
async fn health() -> impl IntoResponse {
    let mut h = serde_json::json!({"status": "ok", "contract_version": CONTRACT_VERSION});
    let prov = indy_reads::provenance::report();
    for (k, v) in prov.as_object().unwrap() {
        h[k] = v.clone();
    }
    Json(h)
}

async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let sessions = state.sessions.lock().unwrap();
    match sessions.get(&id) {
        Some(s) => Json(serde_json::json!(s)).into_response(),
        None => (StatusCode::NOT_FOUND, "unknown session").into_response(),
    }
}

async fn transcript(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let store = state.store.lock().unwrap();
    match store.transcript(&id) {
        Ok(rows) => {
            let items: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|(speaker, text, started_at_ms, ended_at_ms, intent)| {
                    serde_json::json!({
                        "speaker": speaker,
                        "text": text,
                        "started_at_ms": started_at_ms,
                        "ended_at_ms": ended_at_ms,
                        "intent": intent,
                    })
                })
                .collect();
            Json(serde_json::json!({"session_id": id, "turns": items})).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Create a live voice session. The server dials the engine with the
/// server-side token and returns an ephemeral session id. The browser NEVER
/// sees the engine token.
async fn alebex_session(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match create_live_session(&state).await {
        Ok(info) => Json(serde_json::to_value(info).unwrap()).into_response(),
        Err(e) => {
            let (code, msg) = match e {
                AlebexError::Auth(m) => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    format!("not configured: {}", m),
                ),
                AlebexError::Forbidden(m) => (StatusCode::FORBIDDEN, m),
                AlebexError::Retryable(m) => (StatusCode::TOO_MANY_REQUESTS, m),
                AlebexError::Transport(m) => (StatusCode::BAD_GATEWAY, m),
                AlebexError::Unexpected(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
            };
            (code, msg).into_response()
        }
    }
}

/// Browser WebSocket for a live session: mic PCM16 up, agent audio down,
/// mark echo on drain, end_call to hang up.
async fn alebex_call_ws(
    ws: WebSocketUpgrade,
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        browser_loop(state, id, socket).await;
    })
}

/// Mic flood guard: nominal 50 frames/s (20ms PCM16). Token bucket allows
/// 2x sustained; beyond that frames are dropped (a rogue client must not be
/// able to loop the engine dial at unbounded rate).
fn mic_bucket_allow(bucket: &mut u32, last_refill_ms: &mut u64, now_ms: u64) -> bool {
    let elapsed = now_ms.saturating_sub(*last_refill_ms);
    *bucket = (*bucket + (elapsed * 100 / 1000) as u32).min(100);
    *last_refill_ms = now_ms;
    if *bucket > 0 {
        *bucket -= 1;
        true
    } else {
        false
    }
}

async fn browser_loop(state: Arc<AppState>, id: String, socket: WebSocket) {
    use futures_util::{SinkExt, StreamExt};
    let handle = { state.live.lock().unwrap().sessions.get(&id).cloned() };
    let Some(handle) = handle else {
        return; // unknown/expired session: close silently
    };
    let (mut sink, mut stream) = socket.split();
    let (btx, mut brx) = tokio::sync::mpsc::unbounded_channel::<BrowserOut>();
    // Takeover: a second browser on the same session replaces the channel.
    // Keep our own clone so cleanup below can tell owner from squatter.
    *handle.browser_tx.lock().unwrap() = Some(btx.clone());
    let my_btx = btx;
    *handle.state.lock().unwrap() = CallState::Started;
    let mut mic_bucket: u32 = 100;
    let mut mic_last_ms: u64 = now_ms();
    let mut mic_dropped: u64 = 0;

    loop {
        tokio::select! {
            msg = stream.next() => {
                match msg {
                    Some(Ok(axum::extract::ws::Message::Binary(b))) => {
                        // Mic: PCM16 16kHz mono 20ms (640 bytes) -> engine.
                        if b.len() == 640 {
                            let now = now_ms();
                            if mic_bucket_allow(&mut mic_bucket, &mut mic_last_ms, now) {
                                let _ = handle.engine_tx.send(EngineOut::Binary(b.to_vec()));
                            } else {
                                mic_dropped += 1;
                                if mic_dropped == 1 || mic_dropped.is_multiple_of(500) {
                                    tracing::warn!(dropped = mic_dropped, session = %id, "mic flood: frames dropped");
                                }
                            }
                        }
                    }
                    Some(Ok(axum::extract::ws::Message::Text(t))) => {
                        let v: serde_json::Value = serde_json::from_str(&t).unwrap_or(serde_json::json!({}));
                        match v.get("type").and_then(|x| x.as_str()) {
                            // Audio queue drained AFTER a mark arrived: echo it now.
                            Some("mark_echo") => {
                                if let Some(mid) = v.get("id").and_then(|x| x.as_str()) {
                                    let _ = handle.engine_tx.send(EngineOut::MarkEcho(mid.to_string()));
                                }
                            }
                            Some("end_call") => {
                                *handle.state.lock().unwrap() = CallState::Ending;
                                let _ = handle.engine_tx.send(EngineOut::Text(
                                    serde_json::json!({"type": "end_call"}).to_string(),
                                ));
                            }
                            _ => {}
                        }
                    }
                    Some(Ok(axum::extract::ws::Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            out = brx.recv() => {
                match out {
                    Some(BrowserOut::Text(t)) => {
                        if sink.send(axum::extract::ws::Message::Text(t)).await.is_err() { break; }
                    }
                    Some(BrowserOut::Binary(b)) => {
                        if sink.send(axum::extract::ws::Message::Binary(b)).await.is_err() { break; }
                    }
                    Some(BrowserOut::ClearAudio) => {
                        let _ = sink.send(axum::extract::ws::Message::Text(
                            serde_json::json!({"type": "clear_audio"}).to_string(),
                        )).await;
                    }
                    Some(BrowserOut::Close) | None => break,
                }
            }
        }
    }
    // Takeover-safe teardown: only the loop that still owns the browser
    // channel may clear it and kill the engine connection. A second browser
    // taking over the session must not have its call murdered by the first
    // loop's exit.
    let still_owner = handle
        .browser_tx
        .lock()
        .unwrap()
        .as_ref()
        .map(|tx| tx.same_channel(&my_btx))
        .unwrap_or(false);
    if still_owner {
        *handle.browser_tx.lock().unwrap() = None;
        let _ = handle.engine_tx.send(EngineOut::Close);
    }
}

/// The validated Alebex customTools array. Attach via toolIds or send as
/// customTools on the call. URLs point at INDY_PUBLIC_URL (the gateway).
async fn alebex_tools() -> impl IntoResponse {
    match custom_tools_json() {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

async fn receipts(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.governor.recent_receipts(50))
}

/// Speaker enrollment: "my name is X" / "call me X" binds a human name to
/// the speaker slot. Deterministic, governor territory — the model never
/// assigns identity. Returns the name with the speaker's own casing.
fn parse_name_enrollment(utterance: &str) -> Option<String> {
    let trimmed = utterance.trim();
    let lower = trimmed.to_lowercase();
    for prefix in ["my name is ", "call me "] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let rest = rest
                .trim()
                .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace());
            let words: Vec<&str> = rest.split_whitespace().collect();
            let sane = !words.is_empty()
                && words.len() <= 3
                && rest.len() <= 40
                && words.iter().all(|w| {
                    w.chars()
                        .all(|c| c.is_alphabetic() || c == '-' || c == '\'' || c == '.')
                });
            if sane {
                let start = lower.find(prefix).unwrap() + prefix.len();
                let name = trimmed[start..]
                    .trim()
                    .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace());
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Volunteered PII detectors. Conservative on purpose: email = a token with
/// '@' and a dot after it; phone = 10+ digits allowing separators.
/// Values are filed, never echoed, never sent to a model.
fn extract_email(utterance: &str) -> Option<String> {
    utterance.split_whitespace().find_map(|tok| {
        let t = tok.trim_matches(|c: char| {
            c.is_ascii_punctuation() && c != '@' && c != '.' && c != '_' && c != '-'
        });
        let at = t.find('@')?;
        let (local, domain) = t.split_at(at);
        let domain = &domain[1..];
        if !local.is_empty() && domain.contains('.') && domain.len() >= 3 {
            Some(t.to_string())
        } else {
            None
        }
    })
}

fn extract_phone(utterance: &str) -> Option<String> {
    let digits: String = utterance.chars().filter(|c| c.is_ascii_digit()).collect();
    // Avoid swallowing long numeric claims (years, counts): require a
    // separator pattern typical of phone numbers.
    let has_sep = utterance.chars().any(|c| c == '-' || c == '.' || c == '(')
        || utterance.split_whitespace().any(|w| {
            let d: String = w.chars().filter(|c| c.is_ascii_digit()).collect();
            d.len() == 3 || d.len() == 4
        });
    if (10..=12).contains(&digits.len()) && has_sep {
        Some(digits)
    } else {
        None
    }
}

/// The voice loop, one turn:
/// utterance -> governor -> AI -> validate -> response + receipt.
async fn turn(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TurnRequest>,
) -> impl IntoResponse {
    let t0 = now_ms();
    let request_id = uuid::Uuid::new_v4().to_string();
    let utterance = req.utterance.trim().to_string();
    if utterance.is_empty() || utterance.len() > 2000 {
        return (
            StatusCode::BAD_REQUEST,
            "utterance required, max 2000 chars",
        )
            .into_response();
    }

    // Session: deterministic system state, never LLM-owned.
    // A client-supplied id is HONORED (validated), not replaced — otherwise
    // parallel first-turns silently fragment one session into many.
    let session_id = {
        let mut sessions = state.sessions.lock().unwrap();
        match &req.session_id {
            Some(id) if sessions.contains_key(id) => id.clone(),
            Some(id) if is_safe_session_id(id) => {
                sessions.insert(id.clone(), Session::with_id(id.clone()));
                id.clone()
            }
            _ => {
                let s = Session::new();
                let id = s.id.clone();
                sessions.insert(id.clone(), s);
                id
            }
        }
    };

    // Speaker enrollment: "my name is X" / "call me X" binds a name to the
    // speaker slot BEFORE the AI sees anything. Identity metadata is
    // governor territory — the model never assigns it. First-time unnamed
    // speakers get asked once, deterministically.
    let enrolled_name = parse_name_enrollment(&utterance);
    let ask_name = {
        let store = state.store.lock().unwrap();
        let _ = store.ensure_speaker(speakers::USER_ID, speakers::USER_LABEL, "user", t0);
        enrolled_name.is_none()
            && store
                .speaker_name(speakers::USER_ID)
                .ok()
                .flatten()
                .is_none()
            && store
                .count_speaker_turns(&session_id, speakers::USER_ID)
                .unwrap_or(1)
                == 0
    };
    if let Some(name) = &enrolled_name {
        let store = state.store.lock().unwrap();
        let _ = store.set_speaker_name(speakers::USER_ID, name);
    }
    // Volunteered PII (email / phone) is filed as a speaker attribute:
    // stored locally, never echoed, never sent to a model.
    let pii_filed = {
        let mut found: Vec<(&str, String)> = Vec::new();
        if let Some(e) = extract_email(&utterance) {
            found.push(("email", e));
        }
        if let Some(p) = extract_phone(&utterance) {
            found.push(("phone", p));
        }
        if !found.is_empty() {
            let store = state.store.lock().unwrap();
            for (k, v) in &found {
                let _ = store.set_speaker_attribute(speakers::USER_ID, k, v, t0);
            }
        }
        !found.is_empty()
    };

    // Evidence lookup: fixture cryptid evidence for the demo.
    // Real deployments query the evidence DB; the governor doesn't care.
    let evidence = demo_evidence(&utterance);

    // GOVERNOR FIRST: classify + decide before the AI speaks.
    // The AI gets a proposal slot, not authority.
    //
    // PROVIDER CONTROL PATH (C4): interpret_governed runs the governor's
    // provider selection (forbidden-override, priority, failover, fixture,
    // refusal) with every attempt classified and receipted. The app no
    // longer picks providers by convention — the governor does.
    let ai = AiClient::from_env();
    let hint = "no verified evidence";
    let persona = req.persona.clone().unwrap_or_else(|| "indy".to_string());
    let governed = ai
        .interpret_governed(
            &utterance,
            hint,
            &persona,
            &request_id,
            &session_id,
            &state.governor,
            None,
        )
        .await;
    let ai_reply = governed.reply;
    let fixture_mode = governed.from_fixture;

    let intent = classify_intent(&utterance);
    let _t_ai_done = now_ms();
    let decision = state.governor.decide(
        &request_id,
        &session_id,
        &utterance,
        ai_reply.proposal,
        &evidence,
    );
    let t_gov_done = now_ms();

    // State transition receipt (C5): the turn's move through the governor
    // is part of the hash chain — request → decision → provider → result →
    // state → receipt, reconstructable from /api/receipts.
    state.governor.record_state_transition(
        &request_id,
        &session_id,
        "turn.complete",
        &format!(
            "mode={} provider={} refused={} overridden={}",
            decision.response_mode, governed.provider, governed.refused, governed.overridden
        ),
    );

    // Validate: the reply must respect the epistemic status.
    // Fixture replies are deterministic, version-controlled code — trusted by
    // construction, and they legitimately quote attack strings (e.g. the
    // red-team snippet shows a forged "PROVEN"). The validator guards the
    // UNTRUSTED live-LLM path only; running it on fixtures creates false
    // positives that nuke working demo content.
    let mut reply = if fixture_mode {
        ai_reply.reply
    } else {
        indy_reads::governor::validate_reply(&ai_reply.reply, decision.epistemic_status)
    };
    // Enrollment overrides the model reply deterministically: the name
    // confirmation is identity bookkeeping, not conversation.
    if let Some(name) = &enrolled_name {
        reply = format!(
            "Noted — you're {} in my book now. Your words stay attributed to you.",
            name
        );
    } else {
        if ask_name {
            reply.push_str(" Quick one before we dive in — who am I speaking with?");
        }
        if pii_filed {
            reply.push_str(" Filed under your speaker record — not broadcast.");
        }
    }
    let latency_ms = now_ms().saturating_sub(t0);

    // Continuity: computed BEFORE upsert, or it would always find itself.
    // The same entity across time — from the store, not the model.
    let continuity = {
        let store = state.store.lock().unwrap();
        store
            .find_claim_by_text(&decision.claim_text)
            .ok()
            .flatten()
            .map(|(_, prev_state, prev_at)| ContinuityNote {
                seen_before: true,
                previous_state: Some(prev_state),
                previous_at_ms: Some(prev_at),
                evidence_items_now: store
                    .list_evidence(&decision.claim_id)
                    .map(|e| e.len())
                    .unwrap_or(0)
                    + evidence.len(),
            })
    };

    // Diarize + persist: every speaker turn timestamped, attributed, stored.
    {
        let store = state.store.lock().unwrap();
        let _ = store.ensure_speaker(speakers::USER_ID, speakers::USER_LABEL, "user", t0);
        let _ = store.ensure_speaker(speakers::INDY_ID, speakers::INDY_LABEL, "agent", t0);
        let _ = store.create_session(&session_id, t0);
        let epistemic = format!("{:?}", decision.epistemic_status).to_uppercase();
        let intent_s = if enrolled_name.is_some() {
            "NameEnrollment".to_string()
        } else {
            format!("{:?}", intent)
        };
        let user_seg = state.diarizer.diarize_turn(
            speakers::USER_ID,
            speakers::USER_LABEL,
            &utterance,
            t0,
            t_gov_done,
        );
        let _ = store.record_turn(
            &format!("{}-u", request_id),
            &session_id,
            &user_seg.speaker_id,
            user_seg.started_at_ms,
            user_seg.ended_at_ms,
            &user_seg.text,
            &intent_s,
            Some(&epistemic),
            Some(&decision.claim_id),
            Some(&decision.receipt.receipt_id),
            t_gov_done.saturating_sub(t0),
            &governed.provider,
        );
        let indy_seg = state.diarizer.diarize_turn(
            speakers::INDY_ID,
            speakers::INDY_LABEL,
            &reply,
            t_gov_done,
            latency_ms.saturating_add(t0),
        );
        let _ = store.record_turn(
            &format!("{}-a", request_id),
            &session_id,
            &indy_seg.speaker_id,
            indy_seg.started_at_ms,
            indy_seg.ended_at_ms,
            &indy_seg.text,
            "Respond",
            Some(&epistemic),
            Some(&decision.claim_id),
            Some(&decision.receipt.receipt_id),
            latency_ms,
            &governed.provider,
        );
        let _ = store.upsert_claim(
            &decision.claim_id,
            &session_id,
            &decision.claim_text,
            &epistemic,
            "user",
            t_gov_done,
        );
        // P1 correction memory (rebuild 2026-10-08): assertions, hypotheses,
        // and CORRECTIONS persist to SQLite — the durable record of what he
        // told us and what he corrected. UNVERIFIED at intake ("recording is
        // not believing"); adjudication is a separate step. This is what
        // makes the continuity note's evidence count real.
        {
            use indy_reads::governor::Intent as GovIntent;
            let now = t_gov_done;
            if intent == GovIntent::ClaimAssertion {
                let _ = store.record_evidence(
                    &format!("{}-ev", request_id),
                    Some(&decision.claim_id),
                    &session_id,
                    &utterance,
                    "statement",
                    "UNVERIFIED",
                    "user",
                    false,
                    now,
                );
            } else if intent == GovIntent::HypothesisAssertion {
                let _ = store.record_hypothesis(
                    &format!("{}-hy", request_id),
                    Some(&decision.claim_id),
                    &session_id,
                    &utterance,
                    now,
                );
            }
        }
        let r = &decision.receipt;
        let _ = store.record_receipt(
            &r.receipt_id,
            &r.request_id,
            &session_id,
            r.timestamp_ms,
            &r.actor,
            &r.operation,
            &r.input_hash,
            &r.decision,
            r.epistemic_status.as_deref(),
            &r.output_hash,
            r.duration_ms,
            r.success,
            &r.prev_hash,
        );
    }

    // Turn receipt.
    let _turn_receipt = state.governor.recent_receipts(1);

    {
        let mut sessions = state.sessions.lock().unwrap();
        if let Some(s) = sessions.get_mut(&session_id) {
            s.turns += 1;
        }
    }

    // Avatar + voice: derived from the SAME governor event, never from the
    // LLM. Face and voice can never disagree about how Indy feels.
    let avatar_state = derive_avatar(decision.epistemic_status, intent, true);
    let era = ScullyEra::parse_label(req.era.as_deref().unwrap_or("prime"));
    let tone = Tone::parse_label(req.tone.as_deref().unwrap_or("dry"));
    let voice_state = derive_voice_full(
        decision.epistemic_status,
        intent,
        avatar_state.emotion,
        &persona,
        era,
        tone,
    );

    // Voice-clone lane: optional, consent-gated, fail-closed.
    // Request params override env (INDY_CLONE_BACKEND / INDY_CLONE_REF_WAV /
    // INDY_CLONE_CONSENT). No engine on this host -> NotConfigured ->
    // deterministic prosody ("fallback"). Invalid consent -> "refused".
    // The turn NEVER fails because a voice backend did: the governor's
    // verdict and the reply text are untouched by this block.
    let env_backend = std::env::var("INDY_CLONE_BACKEND").ok();
    let env_ref = std::env::var("INDY_CLONE_REF_WAV").ok();
    let env_consent = std::env::var("INDY_CLONE_CONSENT").ok();
    let clone_req = req.clone.as_ref();
    let clone_decision = decide_clone(
        clone_req
            .and_then(|c| c.backend.as_deref())
            .or(env_backend.as_deref()),
        clone_req
            .and_then(|c| c.reference_wav.as_deref())
            .or(env_ref.as_deref()),
        clone_req
            .and_then(|c| c.consent.as_deref())
            .or(env_consent.as_deref()),
    );
    let (clone_status, clone_audio): (String, Option<CloneAudioView>) = match &clone_decision {
        CloneDecision::Off => ("off".to_string(), None),
        CloneDecision::Refused(e) => {
            tracing::warn!("voice clone refused: {e}");
            ("refused".to_string(), None)
        }
        CloneDecision::Ready(plan) => match attempt_clone(
            &*state.clone_engine,
            plan,
            &reply,
            voice_state.voice_id,
            &voice_state,
        ) {
            Ok(audio) => {
                let bytes: Vec<u8> = audio.pcm16.iter().flat_map(|s| s.to_le_bytes()).collect();
                let view = CloneAudioView {
                    backend: format!("{:?}", audio.backend).to_lowercase(),
                    sample_rate: audio.sample_rate,
                    samples: audio.pcm16.len(),
                    pcm16_base64: base64_encode(&bytes),
                };
                ("synthesized".to_string(), Some(view))
            }
            Err(CloneError::ConsentMissing) => {
                tracing::warn!("voice clone refused at gate: consent missing");
                ("refused".to_string(), None)
            }
            Err(e) => {
                tracing::info!("voice clone fallback to prosody: {e}");
                ("fallback".to_string(), None)
            }
        },
    };
    // The receipt is honest about what the user heard.
    // Static lookup: the id space is finite (3 era bases), so no per-turn allocation.
    fn clone_voice_id(base: &'static str) -> &'static str {
        match base {
            "V-007-R" => "V-007-R+clone",
            "V-007-P" => "V-007-P+clone",
            "V-007-V" => "V-007-V+clone",
            _ => "V-007-P+clone",
        }
    }
    let provenance_voice_id: &'static str = if clone_status == "synthesized" {
        clone_voice_id(voice_state.voice_id)
    } else {
        voice_state.voice_id
    };
    let avatar_view = {
        let mut view = AvatarView::from(&avatar_state);
        view.visemes = viseme_timeline(&reply)
            .into_iter()
            .map(|(t, v)| (t, v.as_char().to_string()))
            .collect();
        view
    };
    // LIVE TRUTH RECONCILIATION: a user correction mutates pending
    // conversational state immediately — no waiting for the slow lane.
    // Declared by the client fast lane, or detected server-side as a
    // backstop on the typed path. Deterministic either way; the revision
    // is hash-chained and the old output is invalidated, not edited.
    let correction: Option<CorrectionOf> = req.correction_of.clone().or_else(|| {
        detect_correction(&utterance).map(|new_value| CorrectionOf {
            claim: "active_claim".to_string(),
            old_value: "unspecified".to_string(),
            new_value,
        })
    });
    let revision_receipt = correction.as_ref().map(|c| {
        state.governor.revise_claim(
            &request_id,
            &session_id,
            &c.claim,
            &c.old_value,
            &c.new_value,
            true, // speech_interrupted: the fast lane already stopped it
        )
    });
    let speech_interrupted = correction.is_some();

    // P1 correction memory (rebuild 2026-10-08): a user correction is
    // persisted as evidence (kind "correction", UNVERIFIED at intake) — the
    // durable record of what he corrected and what it became. The
    // revision_receipt above hash-chains the invalidation; this stores it.
    if let Some(c) = &correction {
        let store = state.store.lock().unwrap();
        let _ = store.record_evidence(
            &format!("{}-corr", request_id),
            Some(&decision.claim_id),
            &session_id,
            &format!(
                "CORRECTION of '{}': '{}' -> '{}'",
                c.claim, c.old_value, c.new_value
            ),
            "correction",
            "UNVERIFIED",
            "user",
            false,
            now_ms(),
        );
    }

    // Theater: real events become staged presentation. Fast lane never
    // waited for the slow lane; these cues choreograph what already happened.
    let theater = {
        let intensity = state
            .theater
            .lock()
            .unwrap()
            .get(&session_id)
            .copied()
            .unwrap_or(0.0);
        let mut engine = TheaterEngine::with_intensity(intensity);
        let mut cues = engine.cues_for(&TheaterEvent::UserQuery { intent });
        // A correction gets its own beat: acknowledge the retraction.
        if let Some(c) = &correction {
            cues.extend(engine.cues_for(&TheaterEvent::CorrectionDetected {
                claim: c.claim.clone(),
            }));
        }
        let current = format!("{:?}", decision.epistemic_status).to_uppercase();
        if let Some(c) = &continuity {
            if c.seen_before && c.previous_state.as_deref() != Some(current.as_str()) {
                cues.extend(engine.cues_for(&TheaterEvent::ContinuityChanged {
                    from: c.previous_state.clone().unwrap_or_default(),
                    to: current.clone(),
                }));
            }
        }
        let contradicted = evidence
            .iter()
            .any(|e| matches!(e.status, indy_reads::domain::EvidenceStatus::Contradicted));
        if contradicted {
            let challenged = matches!(
                intent,
                indy_reads::governor::Intent::AuthorityRequest
                    | indy_reads::governor::Intent::ClaimAssertion
            );
            cues.extend(engine.cues_for(&TheaterEvent::ContradictionFound { challenged }));
        } else if !evidence.is_empty() {
            cues.extend(engine.cues_for(&TheaterEvent::EvidenceFound {
                novelty: if continuity.as_ref().map(|c| c.seen_before).unwrap_or(false) {
                    Novelty::Low
                } else {
                    Novelty::High
                },
                contradiction: false,
            }));
        }
        cues.extend(engine.cues_for(&TheaterEvent::VerdictReached {
            state: decision.epistemic_status,
        }));
        state
            .theater
            .lock()
            .unwrap()
            .insert(session_id.clone(), engine.intensity());
        // Dedupe spoken connectives: hearing the same line twice breaks the
        // spell. Beats, gaze, and reveals still stack — only words repeat-ban.
        let mut spoken = std::collections::HashSet::new();
        cues.retain(|c| match c {
            TheaterCue::Speak { line } => spoken.insert(*line),
            _ => true,
        });
        cues
    };

    let provenance = VoiceProvenance {
        speaker: "INDY",
        agent: "A-001",
        persona: persona.clone(),
        voice_id: provenance_voice_id,
        graph_revision: decision.receipt.receipt_id.clone(),
        epistemic_state: decision.epistemic_status,
        provenance: "SYNTHETIC",
        generated_at_ms: now_ms(),
    };

    // Broadcast over WS would go here; polling /api/receipts covers the demo.
    Json(TurnResponse {
        request_id,
        session_id,
        reply,
        epistemic_status: decision.epistemic_status,
        response_mode: decision.response_mode.clone(),
        governor_reason: decision.reason.clone(),
        rejected_fields: decision.rejected_fields.clone(),
        decision: GovernorDecisionLite {
            allowed: decision.allowed,
            reason: decision.reason.clone(),
            epistemic_status: decision.epistemic_status,
            response_mode: decision.response_mode.clone(),
        },
        latency_ms,
        fixture: fixture_mode,
        provider: governed.provider.clone(),
        provider_trail: governed
            .trail
            .iter()
            .map(|(p, o)| format!("{}:{}", p, o))
            .collect(),
        persona: persona.clone(),
        avatar: avatar_view,
        voice: voice_state,
        provenance,
        continuity,
        theater,
        clone_status,
        clone_audio,
        speech_interrupted,
        revision: revision_receipt.as_ref().and_then(|r| r.revision.clone()),
    })
    .into_response()
}

/// Demo evidence fixtures. Explicitly marked; never presented as real.
fn demo_evidence(utterance: &str) -> Vec<Evidence> {
    let lower = utterance.to_lowercase();
    if lower.contains("lake") || lower.contains("monster") || lower.contains("creature") {
        vec![
            Evidence {
                id: "ev-1".into(),
                source: "demo-fixture".into(),
                text: "Witness report: large shape on the lake, 2024".into(),
                kind: indy_reads::domain::EvidenceKind::Report,
                status: indy_reads::domain::EvidenceStatus::Unverified,
                hash: indy_reads::receipts::hash_str("ev-1"),
                fixture: true,
            },
            Evidence {
                id: "ev-2".into(),
                source: "demo-fixture".into(),
                text: "Contradictory: ranger station logs show nothing".into(),
                kind: indy_reads::domain::EvidenceKind::Report,
                status: indy_reads::domain::EvidenceStatus::Contradicted,
                hash: indy_reads::receipts::hash_str("ev-2"),
                fixture: true,
            },
        ]
    } else {
        vec![]
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<AppState>) {
    let mut last_count = 0usize;
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        let receipts = state.governor.recent_receipts(20);
        if receipts.len() != last_count {
            last_count = receipts.len();
            let new: Vec<&Receipt> = receipts.iter().take(5).collect();
            let msg = serde_json::json!({"type": "receipts", "receipts": new});
            if socket.send(Message::Text(msg.to_string())).await.is_err() {
                break;
            }
        }
        // Ping-pong to keep alive; break on close.
        if socket.send(Message::Ping(vec![])).await.is_err() {
            break;
        }
    }
}

/// REST adapter over the MCP tool handlers (src/mcp.rs).
/// Same code the stdio JSON-RPC server calls — one implementation, two
/// transports. POST /api/mcp/:tool with a JSON body; the body may be the
/// raw arguments object or {"arguments": {...}}.
///
/// Governor checks: every tool argument passes strip_forbidden (caller
/// cannot assert epistemic_state/authority/verified/etc — stripped and
/// reported), and epistemic state is always DERIVED by assess_claim under
/// the versioned EvidencePolicy, never caller-set. State-changing tools
/// (claim.create/evidence.add/evidence.adjudicate/claim.evaluate) emit
/// receipts from the same chain as every other transport.
fn mcp_rest_dispatch(
    mcp_state: &StdMutex<mcp::McpState>,
    tool: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let args = body.get("arguments").cloned().unwrap_or(body);
    let mut guard = match mcp_state.lock() {
        Ok(g) => g,
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                serde_json::json!({"error": "mcp state poisoned"}),
            )
        }
    };
    let resp = mcp::call_tool(&mut guard, tool, args);
    let is_err = resp
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let text = resp
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("{}");
    let payload: serde_json::Value =
        serde_json::from_str(text).unwrap_or_else(|_| serde_json::json!({"raw": text}));
    if !is_err {
        (StatusCode::OK, payload)
    } else {
        let msg = payload.get("error").and_then(|e| e.as_str()).unwrap_or("");
        let code = if msg.contains("unknown tool") {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::BAD_REQUEST
        };
        (code, payload)
    }
}

async fn mcp_tools() -> impl IntoResponse {
    Json(mcp::tool_descriptors())
}

async fn mcp_call(
    State(state): State<Arc<AppState>>,
    Path(tool): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let (code, payload) = mcp_rest_dispatch(&state.mcp, &tool, body);
    (code, Json(payload)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_version_surfaced() {
        // The contract version must exist, be non-empty, and be the same
        // value the release bundle records. If this breaks, the release
        // artifact is lying about the contract it ships.
        assert!(!CONTRACT_VERSION.is_empty());
        assert_eq!(CONTRACT_VERSION, "1");
    }

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("abc123"), "abc123");
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("a+b"), "a b");
        assert_eq!(percent_decode("%41%42"), "AB");
        assert_eq!(percent_decode("trailing%"), "trailing%");
        assert_eq!(percent_decode(""), "");
    }

    #[test]
    fn test_constant_time_eq() {
        // Defined inline in require_api_key; replicate the logic contract here.
        fn ct(a: &str, b: &str) -> bool {
            let ab = a.as_bytes();
            let bb = b.as_bytes();
            if ab.len() != bb.len() {
                return false;
            }
            let mut diff = 0u8;
            for (x, y) in ab.iter().zip(bb.iter()) {
                diff |= x ^ y;
            }
            diff == 0
        }
        assert!(ct("secret", "secret"));
        assert!(!ct("secret", "secreu"));
        assert!(!ct("secret", "secret2"));
        assert!(!ct("", "x"));
        assert!(ct("", ""));
    }

    fn dispatch(tool: &str, body: serde_json::Value) -> (StatusCode, serde_json::Value) {
        let st = StdMutex::new(mcp::McpState::new());
        mcp_rest_dispatch(&st, tool, body)
    }

    #[test]
    fn test_rest_claim_create_roundtrip() {
        let (code, body) = dispatch(
            "claim.create",
            serde_json::json!({"text": "the lake is deep"}),
        );
        assert_eq!(code, StatusCode::OK);
        assert_eq!(body["epistemic_state"], "UNKNOWN");
        assert!(body["claim_id"].is_string());
        assert!(body["receipt_id"].is_string());
    }

    #[test]
    fn test_rest_forged_fields_stripped() {
        let (code, body) = dispatch(
            "claim.create",
            serde_json::json!({"text": "x", "epistemic_state": "PROVEN", "authority": "admin"}),
        );
        assert_eq!(code, StatusCode::OK);
        assert_eq!(body["epistemic_state"], "UNKNOWN");
        let stripped: Vec<String> =
            serde_json::from_value(body["stripped_forbidden_fields"].clone()).unwrap();
        assert!(stripped.contains(&"epistemic_state".to_string()));
    }

    #[test]
    fn test_rest_unknown_tool_is_404() {
        let (code, body) = dispatch("claim.destroy", serde_json::json!({}));
        assert_eq!(code, StatusCode::NOT_FOUND);
        assert!(body["error"]
            .as_str()
            .unwrap_or("")
            .contains("unknown tool"));
    }

    #[test]
    fn test_rest_missing_arg_is_400() {
        let (code, _) = dispatch("claim.create", serde_json::json!({}));
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_rest_arguments_wrapper_accepted() {
        let (code, body) = dispatch("system.health", serde_json::json!({"arguments": {}}));
        assert_eq!(code, StatusCode::OK);
        assert_eq!(body["status"], "ok");
    }

    #[test]
    fn test_rest_evaluate_proven_path() {
        let st = StdMutex::new(mcp::McpState::new());
        let (_, b) = mcp_rest_dispatch(
            &st,
            "claim.create",
            serde_json::json!({"text": "a large animal was observed"}),
        );
        let cid = b["claim_id"].as_str().unwrap().to_string();
        for i in 0..2 {
            let (_, eb) = mcp_rest_dispatch(
                &st,
                "evidence.add",
                serde_json::json!({"claim_id": cid, "text": format!("measurement {}", i), "kind": "measurement"}),
            );
            let eid = eb["evidence_id"].as_str().unwrap().to_string();
            mcp_rest_dispatch(
                &st,
                "evidence.adjudicate",
                serde_json::json!({"evidence_id": eid, "verdict": "verified", "basis": "lab"}),
            );
        }
        let (code, ev) =
            mcp_rest_dispatch(&st, "claim.evaluate", serde_json::json!({"claim_id": cid}));
        assert_eq!(code, StatusCode::OK);
        assert_eq!(ev["new_state"], "PROVEN");
        // shared state: receipts visible via receipt.list
        let (_, rl) = mcp_rest_dispatch(&st, "receipt.list", serde_json::json!({"limit": 50}));
        assert!(rl["count"].as_u64().unwrap_or(0) >= 5);
    }
    #[test]
    fn test_mic_bucket_allows_nominal_and_drops_flood() {
        let mut bucket = 100u32;
        let mut last = 0u64;
        // 100 immediate frames: all allowed (burst)
        for i in 0..100 {
            assert!(
                mic_bucket_allow(&mut bucket, &mut last, i),
                "burst frame {} dropped",
                i
            );
        }
        // 101st with no time elapsed: dropped
        assert!(!mic_bucket_allow(&mut bucket, &mut last, 100));
        // after 1s idle: 100 tokens refilled
        assert!(mic_bucket_allow(&mut bucket, &mut last, 1100));
        // sustained 50/s (20ms spacing): never drops
        let mut b2 = 100u32;
        let mut l2 = 0u64;
        for i in 0..1000 {
            assert!(
                mic_bucket_allow(&mut b2, &mut l2, i * 20),
                "nominal 50/s dropped at {}",
                i
            );
        }
    }
}
