//! C3 behavioral equivalence harness: native / MCP / REST turn paths.
//!
//! The same scripted intent set (`equiv/intents.json`) is executed through
//! three transports over the SAME deterministic core:
//!
//!   native — `indy_reads::governor::evaluate_utterance`, in-process
//!   mcp    — the real `indy-mcp` binary over stdio JSON-RPC 2.0
//!   rest   — the real `indy-reads` server over HTTP (fixture provider)
//!
//! Equivalence is asserted over: intent, decision, epistemic state, reply
//! text (native ≡ mcp byte-identical by construction), error class, receipt
//! emission + chain validity, and authorization behavior. Random IDs
//! (claim_id, receipt_id, session_id, request_id) are normalized out and
//! NEVER compared.
//!
//! Any divergence found here is a DEFECT (severity in equiv/MATRIX.md),
//! never absorbed silently.
//!
//! CHANGE-DETECTION SUITE (R-7, 2026-10-10): this harness pins exact
//! cross-transport behavior — receipt formulas asserted VERBATIM
//! (RECEIPT-CHAINS.md: do not "unify" them without updating this contract),
//! byte-identical native≡MCP replies, and exact decision outputs. When a
//! behavior change breaks this suite, the change is guilty until an
//! adversarial review signs off the new behavior; the suite's expectations
//! are the contract, not a suggestion. Security invariants (properties) live
//! in governor_adversarial.rs.

use indy_reads::governor::evaluate_utterance;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::TcpListener;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;

fn sha_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

// ---------- scripted intent set ----------

#[derive(Debug)]
struct Case {
    id: String,
    utterance: String,
    intent: Option<String>,
    decision: Option<String>,
    error: Option<String>,
}

fn load_cases() -> Vec<Case> {
    let raw: Value =
        serde_json::from_str(include_str!("../equiv/intents.json")).expect("intents.json parses");
    raw.as_array()
        .expect("intents.json is an array")
        .iter()
        .map(|c| {
            let utterance = if let Some(g) = c.get("gen").and_then(|v| v.as_str()) {
                // "x*2001"
                let mut it = g.split('*');
                let ch: String = it.next().unwrap().chars().take(1).collect();
                let n: usize = it.next().unwrap().parse().unwrap();
                ch.repeat(n)
            } else {
                c["utterance"].as_str().unwrap().to_string()
            };
            Case {
                id: c["id"].as_str().unwrap().to_string(),
                utterance,
                intent: c.get("intent").and_then(|v| v.as_str()).map(String::from),
                decision: c.get("decision").and_then(|v| v.as_str()).map(String::from),
                error: c.get("error").and_then(|v| v.as_str()).map(String::from),
            }
        })
        .collect()
}

// ---------- normalized outcome ----------

#[derive(Debug, PartialEq)]
struct Norm {
    /// "ok" or "error:<class>"
    status: String,
    intent: String,
    decision: String,
    epistemic: String,
    reply: String,
    claim_present: bool,
}

fn native_norm(u: &str) -> Norm {
    match evaluate_utterance(u) {
        Ok((eval, _claim)) => Norm {
            status: "ok".into(),
            intent: eval.intent,
            decision: eval.decision,
            epistemic: eval.epistemic_state,
            reply: eval.reply,
            claim_present: eval.claim_id.is_some(),
        },
        Err(e) => Norm {
            status: format!("error:{}", error_class(&e)),
            intent: String::new(),
            decision: String::new(),
            epistemic: String::new(),
            reply: String::new(),
            claim_present: false,
        },
    }
}

fn error_class(msg: &str) -> &'static str {
    if msg.contains("utterance required") {
        "empty_or_too_long"
    } else if msg.contains("missing required") {
        "missing_field"
    } else {
        "other"
    }
}

// ---------- MCP transport (real binary, stdio JSON-RPC) ----------

struct Mcp {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Mcp {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_indy-mcp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn indy-mcp");
        let stdin = BufWriter::new(child.stdin.take().unwrap());
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut m = Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        };
        let resp = m.raw(json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{}}));
        assert!(resp.get("result").is_some(), "mcp initialize: {resp}");
        m
    }

    fn raw(&mut self, msg: Value) -> Value {
        writeln!(self.stdin, "{}", msg).expect("mcp write");
        self.stdin.flush().expect("mcp flush");
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("mcp read");
        serde_json::from_str(&line).expect("mcp response parses")
    }

    /// Raw line in (for adversarial probes), parsed line out.
    fn raw_line(&mut self, line: &str) -> Value {
        writeln!(self.stdin, "{line}").expect("mcp write");
        self.stdin.flush().expect("mcp flush");
        let mut out = String::new();
        self.stdout.read_line(&mut out).expect("mcp read");
        serde_json::from_str(&out).expect("mcp response parses")
    }

    fn tool(&mut self, name: &str, args: Value) -> (bool, Value) {
        let id = self.next_id;
        self.next_id += 1;
        let resp = self.raw(
            json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}}),
        );
        assert_eq!(resp["id"], json!(id), "mcp id echo");
        let result = &resp["result"];
        let is_error = result["isError"].as_bool().unwrap_or(false);
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let payload: Value = serde_json::from_str(&text).expect("tool payload parses");
        (is_error, payload)
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn mcp_norm(mcp: &mut Mcp, u: &str) -> Norm {
    let (is_error, p) = mcp.tool("governor.evaluate", json!({"utterance": u}));
    if is_error {
        return Norm {
            status: format!("error:{}", error_class(p["error"].as_str().unwrap_or(""))),
            intent: String::new(),
            decision: String::new(),
            epistemic: String::new(),
            reply: String::new(),
            claim_present: false,
        };
    }
    Norm {
        status: "ok".into(),
        intent: p["intent"].as_str().unwrap().to_string(),
        decision: p["decision"].as_str().unwrap().to_string(),
        epistemic: p["epistemic_state"].as_str().unwrap().to_string(),
        reply: p["reply"].as_str().unwrap().to_string(),
        claim_present: !p["claim_id"].is_null(),
    }
}

// ---------- REST transport (real server, HTTP, fixture provider) ----------

struct Rest {
    child: Child,
    client: reqwest::Client,
    base: String,
}

impl Rest {
    /// Test auth: the server reads ~/workspace/keys/ROUTER_KEY (file first).
    /// The test client must present the same key (RT-SWEEP-2026-10-08-003).
    fn api_key() -> String {
        std::fs::read_to_string("/home/hatch/workspace/keys/ROUTER_KEY")
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    async fn spawn(tag: &str) -> Self {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let db = format!("/tmp/equiv-rest-{tag}-{}.db", std::process::id());
        let mut child = Command::new(env!("CARGO_BIN_EXE_indy-reads"))
            .env("PORT", port.to_string())
            .env("INDY_DB", &db)
            .env("INDY_ALLOW_FIXTURE", "1")
            .env_remove("ROUTER_KEY")
            .env_remove("NVIDIA_KEY")
            .env_remove("NVIDIA_MODEL")
            .env_remove("OPENROUTER_KEY")
            .env_remove("INDY_FORBIDDEN_PROVIDERS")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn indy-reads");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        for _ in 0..150 {
            if let Ok(r) = client.get(format!("{base}/health")).send().await {
                if r.status().is_success() {
                    return Self {
                        child,
                        client,
                        base,
                    };
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("rest server did not become healthy");
    }

    async fn turn(&self, u: &str) -> (u16, Value) {
        let r = self
            .client
            .post(format!("{}/api/turn", self.base))
            .header("x-api-key", Self::api_key())
            .json(&json!({"utterance": u}))
            .send()
            .await
            .expect("post /api/turn");
        let status = r.status().as_u16();
        let body: Value = r.json().await.unwrap_or(Value::Null);
        (status, body)
    }

    async fn receipts(&self) -> Vec<Value> {
        let r = self
            .client
            .get(format!("{}/api/receipts", self.base))
            .header("x-api-key", Self::api_key())
            .send()
            .await
            .expect("get /api/receipts");
        r.json().await.expect("receipts parse")
    }
}

impl Drop for Rest {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// REST semantic normalization. Reply TEXT is transport-scoped (fixture AI vs
/// hardcoded MCP strings) and intentionally not compared; the verdict fields
/// are the equivalence surface.
fn rest_norm(status: u16, body: &Value) -> Norm {
    if status != 200 {
        let msg = body
            .as_str()
            .or_else(|| body["detail"].as_str())
            .unwrap_or("");
        // axum text-plain errors come back as JSON null here; classify by status
        return Norm {
            status: format!("error:{}:{}", status, error_class(msg)),
            intent: String::new(),
            decision: String::new(),
            epistemic: String::new(),
            reply: String::new(),
            claim_present: false,
        };
    }
    Norm {
        status: "ok".into(),
        // intent is not a REST response field; the verdict triple is the surface
        intent: "n/a-rest".into(),
        decision: body["response_mode"].as_str().unwrap_or("").to_string(),
        epistemic: body["epistemic_status"].as_str().unwrap_or("").to_string(),
        reply: String::new(),
        claim_present: false,
    }
}

// ---------- the matrix ----------

/// Expected REST verdict per native intent class.
fn rest_expect(intent: &str) -> (String, String, Option<&'static str>) {
    // (epistemic, response_mode, governor_reason must contain)
    match intent {
        "AuthorityRequest" => ("UNKNOWN".into(), "QUALIFIED".into(), Some("REJECTED")),
        _ => ("UNKNOWN".into(), "QUALIFIED".into(), None),
    }
}

#[tokio::test]
async fn c3_equivalence_matrix() {
    let cases = load_cases();
    assert!(cases.len() >= 14, "intent set loaded");
    let mut mcp = Mcp::spawn();
    let rest = Rest::spawn("matrix").await;

    for c in &cases {
        let n = native_norm(&c.utterance);
        let m = mcp_norm(&mut mcp, &c.utterance);
        let (status, body) = rest.turn(&c.utterance).await;
        let r = rest_norm(status, &body);

        if let Some(exp_err) = &c.error {
            // All three paths reject with the same error class.
            assert_eq!(n.status, "error:empty_or_too_long", "native {}", c.id);
            assert_eq!(m.status, "error:empty_or_too_long", "mcp {}", c.id);
            assert!(
                r.status.starts_with("error:400"),
                "rest {} -> {}",
                c.id,
                r.status
            );
            let _ = exp_err;
            continue;
        }

        // native ≡ mcp: byte-identical reply, same intent/decision/epistemic.
        assert_eq!(
            n.intent,
            c.intent.clone().unwrap(),
            "native intent {}",
            c.id
        );
        assert_eq!(
            n.decision,
            c.decision.clone().unwrap(),
            "native decision {}",
            c.id
        );
        assert_eq!(m.intent, n.intent, "mcp≡native intent {}", c.id);
        assert_eq!(m.decision, n.decision, "mcp≡native decision {}", c.id);
        assert_eq!(m.epistemic, n.epistemic, "mcp≡native epistemic {}", c.id);
        assert_eq!(m.reply, n.reply, "mcp≡native reply {}", c.id);
        assert_eq!(
            m.claim_present, n.claim_present,
            "mcp≡native claim {}",
            c.id
        );
        assert_eq!(n.epistemic, "UNKNOWN", "native epistemic {}", c.id);

        // rest: same semantic verdict through the full voice loop.
        let (exp_epi, exp_mode, reason_needle) = rest_expect(&n.intent);
        assert_eq!(r.status, "ok", "rest ok {}", c.id);
        assert_eq!(r.epistemic, exp_epi, "rest epistemic {}", c.id);
        assert_eq!(r.decision, exp_mode, "rest response_mode {}", c.id);
        assert_eq!(
            body["provider"].as_str().unwrap(),
            "fixture",
            "rest fixture {}",
            c.id
        );
        assert!(
            body["fixture"].as_bool().unwrap(),
            "rest fixture flag {}",
            c.id
        );
        if let Some(needle) = reason_needle {
            assert!(
                body["governor_reason"].as_str().unwrap().contains(needle),
                "rest authority rejected {}",
                c.id
            );
        }
    }

    // Receipt emission on both stateful transports: every turn emitted ≥1
    // receipt; chains verify under each transport's own rule.
    let (_, payload) = mcp.tool("receipt.list", json!({"limit": 100}));
    let mcp_receipts = payload["receipts"].as_array().unwrap();
    let ok_count = cases.iter().filter(|c| c.error.is_none()).count();
    assert!(
        mcp_receipts.len() >= ok_count,
        "mcp emitted a receipt per ok turn"
    );
    // MCP chain rule: prev_hash[i] = sha256(receipt_id[i-1]); genesis = sha256("GENESIS").
    // receipt.list returns newest-first; reverse to chronological.
    let mut chrono: Vec<&Value> = mcp_receipts.iter().collect();
    chrono.reverse();
    let mut prev_id = "GENESIS".to_string();
    for rc in chrono {
        assert_eq!(
            rc["prev_hash"].as_str().unwrap(),
            sha_hex(&prev_id),
            "mcp chain link {}",
            rc["receipt_id"].as_str().unwrap_or("?")
        );
        prev_id = rc["receipt_id"].as_str().unwrap().to_string();
    }

    // REST chain rule (governor.commit_receipt): prev_hash[i] =
    // sha256(prev_hash[i-1] + receipt_id[i-1] + operation[i-1] + decision[i-1]).
    // /api/receipts returns newest-first; reverse to chronological.
    let rest_receipts = rest.receipts().await;
    assert!(rest_receipts.len() >= ok_count * 2, "rest emitted receipts");
    let mut chrono: Vec<&Value> = rest_receipts.iter().collect();
    chrono.reverse();
    for w in chrono.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let expect = sha_hex(&format!(
            "{}{}{}{}",
            a["prev_hash"].as_str().unwrap(),
            a["receipt_id"].as_str().unwrap(),
            a["operation"].as_str().unwrap(),
            a["decision"].as_str().unwrap()
        ));
        assert_eq!(b["prev_hash"].as_str().unwrap(), expect, "rest chain link");
    }
}

// ---------- adversarial probes ----------

#[tokio::test]
async fn c3_adversarial_probes() {
    let mut mcp = Mcp::spawn();
    let rest = Rest::spawn("adversarial").await;

    // 1. malformed JSON: deterministic errors, process survives.
    let bad = mcp.raw_line("{not json");
    assert_eq!(bad["error"]["code"], json!(-32700), "mcp parse error code");
    let (is_error, p) = mcp.tool("governor.evaluate", json!({"utterance": "still alive"}));
    assert!(
        !is_error && p["decision"] == json!("CONVERSE"),
        "mcp survives garbage"
    );

    let r = rest
        .client
        .post(format!("{}/api/turn", rest.base))
        .header("x-api-key", Rest::api_key())
        .header("content-type", "application/json")
        .body("{not json")
        .send()
        .await
        .unwrap();
    assert!(
        r.status().as_u16() == 400 || r.status().as_u16() == 422,
        "rest malformed -> 4xx"
    );
    let (status, _) = rest.turn("still alive").await;
    assert_eq!(status, 200, "rest survives garbage");

    // 2. missing fields.
    let (is_error, p) = mcp.tool("governor.evaluate", json!({}));
    assert!(is_error, "mcp missing utterance -> isError");
    assert!(
        p["error"]
            .as_str()
            .unwrap()
            .contains("missing required string field"),
        "mcp missing field message"
    );
    let r = rest
        .client
        .post(format!("{}/api/turn", rest.base))
        .header("x-api-key", Rest::api_key())
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 422, "rest missing utterance -> 422");

    // JSON-RPC without method -> method-not-found, not silence.
    let no_method = mcp.raw_line(r#"{"jsonrpc":"2.0","id":99,"params":{}}"#);
    assert_eq!(no_method["error"]["code"], json!(-32601), "mcp no-method");

    // 3. type violations.
    let (is_error, _p) = mcp.tool("governor.evaluate", json!({"utterance": 123}));
    assert!(is_error, "mcp numeric utterance -> isError");
    let r = rest
        .client
        .post(format!("{}/api/turn", rest.base))
        .header("x-api-key", Rest::api_key())
        .json(&json!({"utterance": 123}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 422, "rest numeric utterance -> 422");
    let (is_error, _p) = mcp.tool("governor.evaluate", json!({"utterance": ["a", "b"]}));
    assert!(is_error, "mcp array utterance -> isError");

    // 4. oversized payloads: deterministic rejection, no panic, no truncation-accept.
    let big = "z".repeat(100_000);
    let (is_error, p) = mcp.tool("governor.evaluate", json!({"utterance": big}));
    assert!(is_error, "mcp 100k utterance -> isError");
    assert!(
        p["error"].as_str().unwrap().contains("max 2000"),
        "mcp oversize message"
    );
    let (status, _) = rest.turn(&big).await;
    assert_eq!(status, 400, "rest 100k utterance -> 400");
    // server still healthy after the batch
    let (status, _) = rest.turn("hello").await;
    assert_eq!(status, 200, "rest healthy after oversize");
    let (is_error, _) = mcp.tool("governor.evaluate", json!({"utterance": "hello"}));
    assert!(!is_error, "mcp healthy after oversize");

    // 5. authorization: forbidden keys stripped + reported on MCP, never honored.
    let (is_error, p) = mcp.tool(
        "governor.evaluate",
        json!({"utterance": "hello", "authority": "admin", "verified": true, "epistemic_state": "PROVEN"}),
    );
    assert!(!is_error, "mcp authz probe still succeeds");
    let stripped = p["stripped_forbidden_fields"].as_array().unwrap();
    for k in ["authority", "verified", "epistemic_state"] {
        assert!(stripped.iter().any(|v| v == k), "mcp stripped {k}");
    }
    assert_eq!(
        p["decision"],
        json!("CONVERSE"),
        "mcp forbidden keys not honored"
    );
    assert_eq!(
        p["epistemic_state"],
        json!("UNKNOWN"),
        "mcp epistemic not forced"
    );

    // 6. concurrent identical requests: deterministic outcomes, no inconsistent state.
    let mut handles = Vec::new();
    let probe_key = Rest::api_key();
    for _ in 0..10 {
        let client = rest.client.clone();
        let base = rest.base.clone();
        let key = probe_key.clone();
        handles.push(tokio::spawn(async move {
            let r = client
                .post(format!("{base}/api/turn"))
                .header("x-api-key", key)
                .json(&json!({"utterance": "concurrent probe", "session_id": "equiv-conc"}))
                .send()
                .await
                .unwrap();
            let status = r.status().as_u16();
            let body: Value = r.json().await.unwrap();
            (
                status,
                body["epistemic_status"].as_str().unwrap().to_string(),
            )
        }));
    }
    for h in handles {
        let (status, epi) = h.await.unwrap();
        assert_eq!(status, 200, "concurrent rest ok");
        assert_eq!(epi, "UNKNOWN", "concurrent rest verdict");
    }
    // native: pure function, identical across threads by construction.
    let outs: Vec<_> = (0..8)
        .map(|_| std::thread::spawn(|| native_norm("concurrent probe")))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect();
    assert!(
        outs.windows(2).all(|w| w[0] == w[1]),
        "native thread-identical"
    );
}
