//! gmail: read-only Gmail connector for the Indy_Reads governor.
//!
//! Transport: shells out to `hatch_gws_cli gmail` (the connected Gmail
//! skill CLI). No credentials live in this crate — auth is owned by the
//! connector and its OAuth grant. No secrets, no tokens, no hand-rolled auth.
//!
//! READ-ONLY by design. Send/reply/forward/draft/trash/archive are
//! deliberately NOT exposed: the Gmail skill requires the user to approve the
//! exact text of every send, and a voice agent cannot satisfy that. What this
//! connector answers: "search Zack's mail", "read this message for the case".
//!
//! Epistemics: Gmail content enters as evidence with status UNVERIFIED at
//! intake ("recording is not believing"), kind Document, source
//! `gmail:<message-id>`, `fixture: false` (real transport records, not demo
//! fixtures). Verification is a separate adjudication step.
//!
//! Fail-closed: if the CLI binary is missing, the account is not connected,
//! or a call fails, the connector reports an error — never a fabricated
//! empty result. Rate discipline: sequential calls, small default page,
//! hard-capped max (the Gmail budget is weighted per account).

use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::Duration;

/// Account Zack linked to Lucidota (connected 2026-10-08). Used as the
/// default; the connector never invents another account.
pub const DEFAULT_ACCOUNT: &str = "maroonedpilot@gmail.com";

const GMAIL_BIN: &str = "hatch_gws_cli";
const DEFAULT_MAX: u32 = 10;
const HARD_MAX: u32 = 50;
const CALL_TIMEOUT: Duration = Duration::from_secs(90);
const BODY_CHAR_CAP: usize = 8000;

/// Gmail address fields are polymorphic: a plain string, a
/// {"name","email"} dict, or a list of those. Normalize to display text.
fn addr_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(m) => {
            let name = m.get("name").and_then(|x| x.as_str()).unwrap_or("");
            let email = m.get("email").and_then(|x| x.as_str()).unwrap_or("");
            match (name.is_empty(), email.is_empty()) {
                (false, false) => format!("{} <{}>", name, email),
                (false, true) => name.to_string(),
                (true, false) => email.to_string(),
                (true, true) => String::new(),
            }
        }
        serde_json::Value::Array(items) => items
            .iter()
            .map(addr_to_string)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    }
}

fn opt_addr(v: Option<serde_json::Value>) -> String {
    v.map(|x| addr_to_string(&x)).unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmailHit {
    pub id: String,
    pub from: String,
    pub subject: String,
    pub date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmailMessage {
    pub id: String,
    pub thread_id: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub date: String,
    pub body_text: String,
    pub body_truncated: bool,
    pub message_file: Option<String>,
    pub attachment_count: u32,
}

/// How the connector executes the CLI. Swappable so unit tests can inject
/// fixtures instead of touching a real mailbox.
pub type Runner = fn(&[String]) -> Result<String, String>;

fn real_runner(args: &[String]) -> Result<String, String> {
    use std::sync::{mpsc, Arc, Mutex};
    let spawned = Command::new(GMAIL_BIN)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("gmail connector: cannot spawn {}: {}", GMAIL_BIN, e))?;
    // Watchdog: waiter thread picks the Child up out of the shared cell, so
    // the main thread can still kill it on timeout (std has no wait_timeout
    // on stable). Best-effort: if the thread already took the child, the
    // call was finishing anyway.
    let cell: Arc<Mutex<Option<std::process::Child>>> = Arc::new(Mutex::new(None));
    let cell2 = cell.clone();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let child = loop {
            {
                let mut g = match cell2.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                if let Some(c) = g.take() {
                    break c;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let _ = tx.send(child.wait_with_output());
    });
    cell.lock()
        .map_err(|e| format!("gmail connector: lock poisoned: {}", e))?
        .replace(spawned);
    match rx.recv_timeout(CALL_TIMEOUT) {
        Ok(Ok(output)) => {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .map_err(|e| format!("gmail connector: CLI stdout not UTF-8: {}", e))
            } else {
                let err = String::from_utf8_lossy(&output.stderr);
                Err(format!(
                    "gmail connector: {} exited {}: {}",
                    GMAIL_BIN,
                    output.status.code().unwrap_or(-1),
                    err.trim()
                ))
            }
        }
        Ok(Err(e)) => Err(format!("gmail connector: wait failed: {}", e)),
        Err(_) => {
            if let Ok(mut g) = cell.lock() {
                if let Some(mut c) = g.take() {
                    let _ = c.kill();
                }
            }
            Err("gmail connector: call timed out after 90s".to_string())
        }
    }
}

#[derive(Debug, Clone)]
pub struct GmailConnector {
    runner: Runner,
    account: String,
    enabled: bool,
    disabled_reason: Option<String>,
}

impl GmailConnector {
    /// Build against the live CLI, checking connection status once.
    /// Disabled (fail-closed) when the binary is missing or the account is
    /// not connected.
    pub fn new() -> Self {
        let mut c = Self {
            runner: real_runner,
            account: DEFAULT_ACCOUNT.to_string(),
            enabled: false,
            disabled_reason: Some("not checked".to_string()),
        };
        match (c.runner)(&["gmail".into(), "status".into()]) {
            Ok(out) => {
                if out.contains("\"connected\"") {
                    c.enabled = true;
                    c.disabled_reason = None;
                } else {
                    c.disabled_reason = Some("gmail account not connected".to_string());
                }
            }
            Err(e) => {
                c.disabled_reason = Some(e);
            }
        }
        c
    }

    /// Test/backfill constructor with an injected runner.
    pub fn with_runner(runner: Runner) -> Self {
        Self {
            runner,
            account: DEFAULT_ACCOUNT.to_string(),
            enabled: true,
            disabled_reason: None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn disabled_reason(&self) -> Option<&str> {
        self.disabled_reason.as_deref()
    }

    fn require_enabled(&self) -> Result<(), String> {
        if self.enabled {
            Ok(())
        } else {
            Err(format!(
                "gmail connector disabled: {}",
                self.disabled_reason.as_deref().unwrap_or("unknown")
            ))
        }
    }

    fn run(&self, args: &[String]) -> Result<String, String> {
        self.require_enabled()?;
        (self.runner)(args)
    }

    /// Search mail with Gmail search syntax. `max` is clamped to HARD_MAX.
    /// Returns hits (id/from/subject/date) — bodies are fetched with `read`.
    pub fn search(&self, query: &str, max: Option<u32>) -> Result<Vec<GmailHit>, String> {
        let q = query.trim();
        if q.is_empty() {
            return Err("gmail.search: query is empty".to_string());
        }
        if q.len() > 200 {
            return Err("gmail.search: query too long (max 200 chars)".to_string());
        }
        let n = max.unwrap_or(DEFAULT_MAX).min(HARD_MAX).max(1);
        let out = self.run(&[
            "gmail".into(),
            "+triage".into(),
            "--query".into(),
            q.into(),
            "--max".into(),
            n.to_string(),
            "--format".into(),
            "json".into(),
        ])?;
        #[derive(Deserialize)]
        struct TriageHit {
            id: String,
            from: Option<serde_json::Value>,
            subject: Option<String>,
            date: Option<String>,
        }
        #[derive(Deserialize)]
        struct Triage {
            messages: Option<Vec<TriageHit>>,
        }
        let t: Triage =
            serde_json::from_str(&out).map_err(|e| format!("gmail.search: bad CLI JSON: {}", e))?;
        Ok(t
            .messages
            .unwrap_or_default()
            .into_iter()
            .map(|m| GmailHit {
                id: m.id,
                from: opt_addr(m.from),
                subject: m.subject.unwrap_or_default(),
                date: m.date.unwrap_or_default(),
            })
            .collect())
    }

    /// Read one message by id. Body is capped at BODY_CHAR_CAP chars with an
    /// explicit truncation marker — the governor must never mistake a capped
    /// body for a complete one.
    pub fn read(&self, id: &str) -> Result<GmailMessage, String> {
        let id = id.trim();
        if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err("gmail.read: invalid message id".to_string());
        }
        let out = self.run(&[
            "gmail".into(),
            "+read".into(),
            "--id".into(),
            id.into(),
            "--headers".into(),
            "--format".into(),
            "json".into(),
        ])?;
        #[derive(Deserialize)]
        struct MsgFile {
            path: Option<String>,
        }
        #[derive(Deserialize)]
        struct ReadOut {
            id: Option<String>,
            message_id: Option<String>,
            thread_id: Option<String>,
            from: Option<serde_json::Value>,
            to: Option<serde_json::Value>,
            subject: Option<String>,
            date: Option<String>,
            body_text: Option<String>,
            body_text_inline_complete: Option<bool>,
            message_file: Option<MsgFile>,
            attachment_count: Option<u32>,
        }
        let r: ReadOut =
            serde_json::from_str(&out).map_err(|e| format!("gmail.read: bad CLI JSON: {}", e))?;
        let body = r.body_text.unwrap_or_default();
        let (body_text, body_truncated) = if body.chars().count() > BODY_CHAR_CAP {
            let capped: String = body.chars().take(BODY_CHAR_CAP).collect();
            (
                format!(
                    "{}\n\n[…BODY TRUNCATED at {} chars — full text in message_file…]",
                    capped, BODY_CHAR_CAP
                ),
                true,
            )
        } else {
            (
                body,
                !r.body_text_inline_complete.unwrap_or(true),
            )
        };
        Ok(GmailMessage {
            id: r.message_id.or(r.id).unwrap_or_else(|| id.to_string()),
            thread_id: r.thread_id.unwrap_or_default(),
            from: opt_addr(r.from),
            to: opt_addr(r.to),
            subject: r.subject.unwrap_or_default(),
            date: r.date.unwrap_or_default(),
            body_text,
            body_truncated,
            message_file: r.message_file.and_then(|f| f.path),
            attachment_count: r.attachment_count.unwrap_or(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_runner(args: &[String]) -> Result<String, String> {
        let joined = args.join(" ");
        if joined.contains("status") {
            return Ok(r#"{"ok":true,"status":"connected"}"#.to_string());
        }
        if joined.contains("+triage") {
            return Ok(r#"{"messages":[{"id":"abc123","from":"A <a@x.com>","subject":"Test","date":"today"}],"query":"x"}"#.to_string());
        }
        if joined.contains("+read") {
            return Ok(r#"{"message_id":"abc123","thread_id":"t1","from":"A <a@x.com>","to":"B <b@x.com>","subject":"Test","date":"today","body_text":"hello world","body_text_inline_complete":true,"attachment_count":0}"#.to_string());
        }
        Err("unexpected".to_string())
    }

    #[test]
    fn search_parses_triage() {
        let c = GmailConnector::with_runner(fixture_runner);
        let hits = c.search("from:a@x.com", None).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "abc123");
        assert_eq!(hits[0].subject, "Test");
    }

    #[test]
    fn search_rejects_empty_and_long_queries() {
        let c = GmailConnector::with_runner(fixture_runner);
        assert!(c.search("", None).is_err());
        assert!(c.search(&"x".repeat(201), None).is_err());
    }

    #[test]
    fn read_parses_message() {
        let c = GmailConnector::with_runner(fixture_runner);
        let m = c.read("abc123").unwrap();
        assert_eq!(m.body_text, "hello world");
        assert!(!m.body_truncated);
        assert_eq!(m.attachment_count, 0);
    }

    #[test]
    fn read_rejects_bad_ids() {
        let c = GmailConnector::with_runner(fixture_runner);
        assert!(c.read("").is_err());
        assert!(c.read("abc; rm -rf /").is_err());
    }

    #[test]
    fn disabled_connector_fails_closed() {
        let c = GmailConnector {
            runner: fixture_runner,
            account: DEFAULT_ACCOUNT.to_string(),
            enabled: false,
            disabled_reason: Some("not connected".to_string()),
        };
        let err = c.search("x", None).unwrap_err();
        assert!(err.contains("disabled"));
    }

    #[test]
    fn body_cap_marks_truncation() {
        fn big(_: &[String]) -> Result<String, String> {
            Ok(format!(
                r#"{{"message_id":"z","body_text":"{}","body_text_inline_complete":true}}"#,
                "w".repeat(BODY_CHAR_CAP + 10)
            ))
        }
        let c = GmailConnector::with_runner(big);
        let m = c.read("z").unwrap();
        assert!(m.body_truncated);
        assert!(m.body_text.contains("BODY TRUNCATED"));
    }

    #[test]
    fn addr_normalizes_polymorphic_shapes() {
        use serde_json::json;
        assert_eq!(addr_to_string(&json!("A <a@x.com>")), "A <a@x.com>");
        assert_eq!(
            addr_to_string(&json!({"name": "GitHub", "email": "noreply@github.com"})),
            "GitHub <noreply@github.com>"
        );
        assert_eq!(
            addr_to_string(&json!([{"name": "A", "email": "a@x.com"}, {"email": "b@x.com"}])),
            "A <a@x.com>, b@x.com"
        );
        assert_eq!(addr_to_string(&json!(null)), "");
    }

    #[test]
    fn read_handles_dict_addresses() {
        fn dict_addr(_: &[String]) -> Result<String, String> {
            Ok(r#"{"message_id":"d1","thread_id":"t","from":{"name":"GitHub","email":"noreply@github.com"},"to":[{"email":"maroonedpilot@gmail.com","name":"Zack"}],"subject":"S","date":"today","body_text":"b","body_text_inline_complete":true,"attachment_count":0}"#.to_string())
        }
        let c = GmailConnector::with_runner(dict_addr);
        let m = c.read("d1").unwrap();
        assert_eq!(m.from, "GitHub <noreply@github.com>");
        assert_eq!(m.to, "Zack <maroonedpilot@gmail.com>");
    }
}
