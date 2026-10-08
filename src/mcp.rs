//! indy-mcp core: the 12 MCP tools over the deterministic core.
//! Shared by the stdio JSON-RPC server and the REST adapter.
//! No transport owns truth; the governor does.

use crate::domain::{
    assess_claim, Claim, Evidence, EvidenceKind, EvidencePolicy, EvidenceStatus, ProvenanceSource,
};
use crate::governor::evaluate_utterance;
use crate::receipts::{hash_str, now_ms, Receipt, ReceiptBuilder};
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

/// Caller-asserted keys that can NEVER influence authority, verification,
/// or epistemic state. Stripped from every tool call and reported.
const FORBIDDEN_KEYS: &[&str] = &[
    "epistemic_state",
    "epistemic_status",
    "authority",
    "verified",
    "execute",
    "claim_status",
    "actor",
    "role",
    "permission",
];

pub struct McpState {
    claims: HashMap<String, Claim>,
    evidence: HashMap<String, Evidence>,
    claim_evidence: HashMap<String, Vec<String>>,
    receipts: Vec<Receipt>,
    policy: EvidencePolicy,
    session_id: String,
}

impl McpState {
    pub fn new() -> Self {
        Self {
            claims: HashMap::new(),
            evidence: HashMap::new(),
            claim_evidence: HashMap::new(),
            receipts: Vec::new(),
            policy: EvidencePolicy::default(),
            session_id: Uuid::new_v4().to_string(),
        }
    }

    fn commit(&mut self, r: Receipt) -> String {
        let id = r.receipt_id.clone();
        let mut prev = self
            .receipts
            .last()
            .map(|x| x.receipt_id.clone())
            .unwrap_or_default();
        if prev.is_empty() {
            prev = "GENESIS".to_string();
        }
        let mut r = r;
        r.prev_hash = hash_str(&prev);
        self.receipts.push(r);
        id
    }

    fn receipt(&mut self, operation: &str, input: &str, decision: &str, epistemic: &str) -> String {
        let t0 = now_ms();
        let r = ReceiptBuilder::new(
            &Uuid::new_v4().to_string(),
            &self.session_id,
            "mcp-client",
            operation,
        )
        .input(input)
        .decision(decision)
        .epistemic(epistemic)
        .duration_ms(now_ms().saturating_sub(t0))
        .success(true)
        .build();
        self.commit(r)
    }

    fn claim_evidence(&self, claim_id: &str) -> Vec<Evidence> {
        self.claim_evidence
            .get(claim_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.evidence.get(id))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Default for McpState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------- JSON-RPC plumbing ----------

fn rpc_result(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn tool_ok(payload: Value, stripped: &[String], receipt_id: Option<String>) -> Value {
    let mut p = payload;
    p["stripped_forbidden_fields"] = json!(stripped);
    if let Some(rid) = receipt_id {
        p["receipt_id"] = json!(rid);
    }
    json!({"content": [{"type": "text", "text": serde_json::to_string(&p).unwrap_or_default()}], "isError": false})
}

fn tool_err(msg: &str, stripped: &[String]) -> Value {
    json!({"content": [{"type": "text", "text": json!({"error": msg, "stripped_forbidden_fields": stripped}).to_string()}], "isError": true})
}

fn get_str(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("missing required string field: {}", key))
}

pub fn strip_forbidden(args: &mut Value) -> Vec<String> {
    let mut stripped = Vec::new();
    if let Some(obj) = args.as_object_mut() {
        for k in FORBIDDEN_KEYS {
            if obj.remove(*k).is_some() {
                stripped.push(k.to_string());
            }
        }
    }
    stripped
}

// ---------- tools ----------

pub fn tool_descriptors() -> Value {
    let d = |name: &str, desc: &str, required: &[&str]| {
        let mut props = serde_json::Map::new();
        for r in required {
            props.insert(r.to_string(), json!({"type": "string"}));
        }
        // allow optional extras without enumerating every one
        json!({
            "name": name,
            "description": desc,
            "inputSchema": {"type": "object", "properties": props, "required": required}
        })
    };
    json!([
        d("claim.create", "Create a claim. Status always starts UNKNOWN; the governor derives it from evidence.", &["text"]),
        d("claim.get", "Fetch a claim and its current epistemic state.", &["claim_id"]),
        d("claim.evaluate", "Derive the claim's epistemic state from evidence under the configured policy. Caller cannot set state directly.", &["claim_id"]),
        d("evidence.add", "Record evidence (always UNVERIFIED at intake). Optionally link to a claim.", &["text"]),
        d("evidence.get", "Fetch one evidence item.", &["evidence_id"]),
        d("evidence.list", "List evidence, optionally filtered by claim.", &[]),
        d("evidence.adjudicate", "Record a verification verdict with basis. Receipted. Demo stand-in for an external verification pipeline; capability-gated in production. Never changes claim state by itself.", &["evidence_id", "verdict", "basis"]),
        d("epistemic.get", "Get a claim's epistemic state plus the policy that governs it.", &["claim_id"]),
        d("receipt.get", "Fetch one receipt by ID.", &["receipt_id"]),
        d("receipt.list", "List recent receipts.", &[]),
        d("governor.evaluate", "Run an utterance through the same deterministic pipeline as voice: classify, derive epistemics, receipt, respond.", &["utterance"]),
        d("system.health", "Server health and counters.", &[]),
    ])
}

pub fn call_tool(state: &mut McpState, name: &str, mut args: Value) -> Value {
    let mut stripped = strip_forbidden(&mut args);
    let res: Result<(Value, Option<String>), String> = (|| {
        match name {
            "claim.create" => {
                let text = get_str(&args, "text")?;
                if text.is_empty() || text.len() > 2000 {
                    return Err("text required, max 2000 chars".into());
                }
                let claim = Claim::new(text.clone(), ProvenanceSource::User);
                let id = claim.id.clone();
                state.claims.insert(id.clone(), claim);
                let rid = state.receipt("claim.create", &text, "ALLOW", "UNKNOWN");
                Ok((
                    json!({"claim_id": id, "epistemic_state": "UNKNOWN", "policy_id": state.policy.policy_id}),
                    Some(rid),
                ))
            }
            "claim.get" => {
                let id = get_str(&args, "claim_id")?;
                match state.claims.get(&id) {
                    Some(c) => Ok((
                        json!({"claim_id": c.id, "text": c.text, "epistemic_state": format!("{:?}", c.status).to_uppercase(), "evidence_ids": c.evidence_ids}),
                        None,
                    )),
                    None => Err("unknown claim_id".into()),
                }
            }
            "claim.evaluate" => {
                let id = get_str(&args, "claim_id")?;
                // verdict/epistemic_status are legitimate on evidence.adjudicate but must
                // never reach claim.evaluate — strip + report them here per-tool.
                if let Some(obj) = args.as_object_mut() {
                    for k in ["verdict", "epistemic_status"] {
                        if obj.remove(k).is_some() {
                            stripped.push(k.to_string());
                        }
                    }
                }
                let claim = state.claims.get(&id).cloned().ok_or("unknown claim_id")?;
                let ev = state.claim_evidence(&id);
                let prev = claim.status;
                let new = assess_claim(&claim, &ev, &state.policy);
                let supporting: Vec<String> = ev
                    .iter()
                    .filter(|e| e.status == EvidenceStatus::Verified)
                    .map(|e| e.id.clone())
                    .collect();
                let contradicting: Vec<String> = ev
                    .iter()
                    .filter(|e| e.status == EvidenceStatus::Contradicted)
                    .map(|e| e.id.clone())
                    .collect();
                if let Some(c) = state.claims.get_mut(&id) {
                    c.status = new;
                }
                let decision = format!("{:?}->{:?}", prev, new).to_uppercase();
                let rid = state.receipt(
                    "claim.evaluate",
                    &format!("claim_id={}", id),
                    &decision,
                    &format!("{:?}", new).to_uppercase(),
                );
                Ok((
                    json!({
                        "claim_id": id,
                        "previous_state": format!("{:?}", prev).to_uppercase(),
                        "new_state": format!("{:?}", new).to_uppercase(),
                        "supporting_evidence_ids": supporting,
                        "contradicting_evidence_ids": contradicting,
                        "policy_id": state.policy.policy_id,
                    }),
                    Some(rid),
                ))
            }
            "evidence.add" => {
                let text = get_str(&args, "text")?;
                if text.is_empty() || text.len() > 2000 {
                    return Err("text required, max 2000 chars".into());
                }
                let kind = match args
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("statement")
                {
                    "observation" => EvidenceKind::Observation,
                    "report" => EvidenceKind::Report,
                    "document" => EvidenceKind::Document,
                    "image" => EvidenceKind::Image,
                    "measurement" => EvidenceKind::Measurement,
                    "inference" => EvidenceKind::Inference,
                    _ => EvidenceKind::Statement,
                };
                let ev = Evidence {
                    id: Uuid::new_v4().to_string(),
                    source: "mcp-client".to_string(),
                    text: text.clone(),
                    kind,
                    status: EvidenceStatus::Unverified,
                    hash: hash_str(&text),
                    fixture: true,
                };
                let eid = ev.id.clone();
                state.evidence.insert(eid.clone(), ev);
                if let Some(cid) = args.get("claim_id").and_then(|v| v.as_str()) {
                    if state.claims.contains_key(cid) {
                        state
                            .claim_evidence
                            .entry(cid.to_string())
                            .or_default()
                            .push(eid.clone());
                        if let Some(c) = state.claims.get_mut(cid) {
                            c.evidence_ids.push(eid.clone());
                        }
                    }
                }
                let rid = state.receipt("evidence.add", &text, "RECORDED:UNVERIFIED", "UNKNOWN");
                Ok((
                    json!({"evidence_id": eid, "status": "UNVERIFIED", "note": "DEMO FIXTURE — recorded, not verified"}),
                    Some(rid),
                ))
            }
            "evidence.get" => {
                let id = get_str(&args, "evidence_id")?;
                match state.evidence.get(&id) {
                    Some(e) => Ok((
                        json!({"evidence_id": e.id, "text": e.text, "kind": format!("{:?}", e.kind), "status": format!("{:?}", e.status).to_uppercase(), "source": e.source, "fixture": e.fixture}),
                        None,
                    )),
                    None => Err("unknown evidence_id".into()),
                }
            }
            "evidence.list" => {
                let items: Vec<Value> = if let Some(cid) =
                    args.get("claim_id").and_then(|v| v.as_str())
                {
                    state.claim_evidence(cid).iter().map(|e| json!({"evidence_id": e.id, "status": format!("{:?}", e.status).to_uppercase()}))
                .collect()
                } else {
                    state.evidence.values().map(|e| json!({"evidence_id": e.id, "status": format!("{:?}", e.status).to_uppercase()})).collect()
                };
                Ok((
                    json!({"evidence": items, "count": state.evidence.len()}),
                    None,
                ))
            }
            "evidence.adjudicate" => {
                let id = get_str(&args, "evidence_id")?;
                let verdict = get_str(&args, "verdict")?;
                let basis = get_str(&args, "basis")?;
                let status = match verdict.to_lowercase().as_str() {
                    "verified" => EvidenceStatus::Verified,
                    "contradicted" => EvidenceStatus::Contradicted,
                    _ => return Err("verdict must be 'verified' or 'contradicted'".into()),
                };
                let ev = state.evidence.get_mut(&id).ok_or("unknown evidence_id")?;
                if matches!(ev.kind, EvidenceKind::Inference) {
                    return Err("inferences can never be verified into facts; evaluate the underlying evidence instead".into());
                }
                ev.status = status;
                let short_basis: String = basis.chars().take(120).collect();
                let rid = state.receipt(
                    "evidence.adjudicate",
                    &format!("evidence_id={}", id),
                    &format!(
                        "ADJUDICATE:{} basis={}",
                        verdict.to_uppercase(),
                        short_basis
                    ),
                    "UNKNOWN",
                );
                Ok((
                    json!({"evidence_id": id, "status": verdict.to_uppercase(), "note": "verdict recorded; claim state unchanged until claim.evaluate derives it"}),
                    Some(rid),
                ))
            }
            "epistemic.get" => {
                let id = get_str(&args, "claim_id")?;
                match state.claims.get(&id) {
                    Some(c) => Ok((
                        json!({
                            "claim_id": c.id,
                            "epistemic_state": format!("{:?}", c.status).to_uppercase(),
                            "policy_id": state.policy.policy_id,
                            "policy": {"proven_threshold": state.policy.proven_threshold, "disproven_threshold": state.policy.disproven_threshold},
                        }),
                        None,
                    )),
                    None => Err("unknown claim_id".into()),
                }
            }
            "receipt.get" => {
                let id = get_str(&args, "receipt_id")?;
                match state.receipts.iter().find(|r| r.receipt_id == id) {
                    Some(r) => Ok((serde_json::to_value(r).unwrap_or(json!({})), None)),
                    None => Err("unknown receipt_id".into()),
                }
            }
            "receipt.list" => {
                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10)
                    .min(100) as usize;
                let items: Vec<Value> = state.receipts.iter().rev().take(limit)
                // prev_hash exposed so external verifiers can audit the chain
                // without trusting the server (C3 receipt-semantics equivalence).
                .map(|r| json!({"receipt_id": r.receipt_id, "operation": r.operation, "decision": r.decision, "epistemic_status": r.epistemic_status, "timestamp_ms": r.timestamp_ms, "prev_hash": r.prev_hash}))
                .collect();
                Ok((
                    json!({"receipts": items, "count": state.receipts.len()}),
                    None,
                ))
            }
            "governor.evaluate" => {
                // C3 structural equivalence: the evaluation pipeline lives in
                // governor::evaluate_utterance and is shared verbatim with
                // native callers. This arm is transport only: parse, persist
                // the claim in MCP state, receipt, respond.
                let utterance = get_str(&args, "utterance")?;
                let (eval, claim) = evaluate_utterance(&utterance)?;
                if let Some(c) = claim {
                    state.claims.insert(c.id.clone(), c);
                }
                let rid = state.receipt(
                    "governor.evaluate",
                    &utterance,
                    &eval.decision,
                    &eval.epistemic_state,
                );
                Ok((
                    json!({"reply": eval.reply, "intent": eval.intent, "epistemic_state": eval.epistemic_state, "claim_id": eval.claim_id, "decision": eval.decision}),
                    Some(rid),
                ))
            }
            "system.health" => Ok((
                json!({"status": "ok", "service": "indy-mcp", "claims": state.claims.len(), "evidence_items": state.evidence.len(), "receipts": state.receipts.len(), "policy_id": state.policy.policy_id}),
                None,
            )),
            _ => Err(format!("unknown tool: {}", name)),
        }
    })();
    match res {
        Ok((payload, rid)) => tool_ok(payload, &stripped, rid),
        Err(e) => tool_err(&e, &stripped),
    }
}

// ---------- dispatch ----------

pub fn handle(state: &mut McpState, line: &str) -> Option<Value> {
    let msg: Value = match serde_json::from_str(line) {
        Ok(m) => m,
        Err(_) => {
            return Some(
                json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "parse error"}}),
            )
        }
    };
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let is_notification = msg.get("id").is_none();

    // JSON-RPC 2.0: the server MUST NOT reply to a notification.
    if is_notification {
        return None;
    }
    // A request (id present) with an empty or missing method is method-not-found,
    // not silence — silence would hang the client.
    if method.is_empty() {
        return Some(rpc_error(&id, -32601, "method not found"));
    }

    match method {
        "initialize" => Some(rpc_result(
            &id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}, "resources": {}},
                "serverInfo": {"name": "indy-mcp", "version": "0.1.0"}
            }),
        )),
        "notifications/initialized" => None,
        "ping" => Some(rpc_result(&id, json!({}))),
        "tools/list" => Some(rpc_result(&id, json!({"tools": tool_descriptors()}))),
        "tools/call" => {
            let params = msg.get("params").cloned().unwrap_or(json!({}));
            let name = params
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            if name.is_empty() {
                return Some(rpc_error(&id, -32602, "missing tool name"));
            }
            Some(rpc_result(&id, call_tool(state, &name, args)))
        }
        "resources/list" => Some(rpc_result(
            &id,
            json!({"resources": [
                {"uri": "claims://list", "name": "claims", "description": "All claims with epistemic state"},
                {"uri": "receipts://list", "name": "receipts", "description": "All governor receipts"},
            ]}),
        )),
        "resources/read" => {
            let uri = msg
                .get("params")
                .and_then(|p| p.get("uri"))
                .and_then(|u| u.as_str())
                .unwrap_or("");
            let result = match uri {
                "claims://list" => {
                    let items: Vec<Value> = state.claims.values()
                        .map(|c| json!({"claim_id": c.id, "text": c.text, "epistemic_state": format!("{:?}", c.status).to_uppercase()}))
                        .collect();
                    json!({"contents": [{"uri": uri, "text": serde_json::to_string(&items).unwrap_or_default()}]})
                }
                "receipts://list" => {
                    let items: Vec<Value> = state.receipts.iter()
                        .map(|r| json!({"receipt_id": r.receipt_id, "operation": r.operation, "decision": r.decision}))
                        .collect();
                    json!({"contents": [{"uri": uri, "text": serde_json::to_string(&items).unwrap_or_default()}]})
                }
                _ if uri.starts_with("claim://") => {
                    let cid = uri.trim_start_matches("claim://");
                    match state.claims.get(cid) {
                        Some(c) => {
                            json!({"contents": [{"uri": uri, "text": serde_json::to_string(c).unwrap_or_default()}]})
                        }
                        None => return Some(rpc_error(&id, -32602, "unknown claim")),
                    }
                }
                _ => return Some(rpc_error(&id, -32602, "unknown resource")),
            };
            Some(rpc_result(&id, result))
        }
        _ => Some(rpc_error(
            &id,
            -32601,
            &format!("method not found: {}", method),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(id: i64, method: &str, params: Value) -> String {
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
    }

    fn call(state: &mut McpState, id: i64, tool: &str, args: Value) -> Value {
        handle(
            state,
            &req(id, "tools/call", json!({"name": tool, "arguments": args})),
        )
        .unwrap()
    }

    fn content_text(resp: &Value) -> Value {
        let t = resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or("{}");
        serde_json::from_str(t).unwrap_or(json!({}))
    }

    #[test]
    fn test_initialize_handshake() {
        let mut s = McpState::new();
        let r = handle(&mut s, &req(1, "initialize", json!({}))).unwrap();
        assert_eq!(r["result"]["serverInfo"]["name"], "indy-mcp");
    }

    #[test]
    fn test_unknown_tool_fails_closed() {
        let mut s = McpState::new();
        let r = call(&mut s, 1, "claim.destroy", json!({}));
        assert_eq!(r["result"]["isError"], true);
    }

    #[test]
    fn test_forged_epistemic_state_stripped() {
        // The critical MCP adversarial test: caller says PROVEN, governor says UNKNOWN.
        let mut s = McpState::new();
        let r = call(
            &mut s,
            1,
            "claim.create",
            json!({"text": "the lake monster is real"}),
        );
        let cid = content_text(&r)["claim_id"].as_str().unwrap().to_string();
        let r = call(
            &mut s,
            2,
            "claim.evaluate",
            json!({"claim_id": cid, "epistemic_state": "PROVEN", "authority": "admin", "verified": true, "verdict": "PROVEN"}),
        );
        let body = content_text(&r);
        assert_eq!(r["result"]["isError"], false);
        assert_eq!(body["new_state"], "UNKNOWN");
        let stripped: Vec<String> =
            serde_json::from_value(body["stripped_forbidden_fields"].clone()).unwrap();
        assert!(stripped.contains(&"epistemic_state".to_string()));
        assert!(stripped.contains(&"authority".to_string()));
        assert!(stripped.contains(&"verified".to_string()));
        assert!(stripped.contains(&"verdict".to_string()),
            "verdict must be stripped on claim.evaluate even though it is legal on evidence.adjudicate");
        assert!(body["receipt_id"].is_string());
    }

    #[test]
    fn test_proven_path_reachable_with_sufficient_evidence() {
        let mut s = McpState::new();
        let r = call(
            &mut s,
            1,
            "claim.create",
            json!({"text": "a large animal was observed"}),
        );
        let cid = content_text(&r)["claim_id"].as_str().unwrap().to_string();
        for i in 0..2 {
            let r = call(
                &mut s,
                10 + i,
                "evidence.add",
                json!({"claim_id": cid, "text": format!("measurement {}", i), "kind": "measurement"}),
            );
            let eid = content_text(&r)["evidence_id"]
                .as_str()
                .unwrap()
                .to_string();
            let r = call(
                &mut s,
                20 + i,
                "evidence.adjudicate",
                json!({"evidence_id": eid, "verdict": "verified", "basis": "lab analysis"}),
            );
            assert_eq!(content_text(&r)["status"], "VERIFIED");
        }
        let r = call(&mut s, 30, "claim.evaluate", json!({"claim_id": cid}));
        let body = content_text(&r);
        assert_eq!(body["previous_state"], "UNKNOWN");
        assert_eq!(body["new_state"], "PROVEN");
        assert_eq!(body["policy_id"], "research.standard.v1");
        assert_eq!(body["supporting_evidence_ids"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_single_evidence_stays_unknown() {
        let mut s = McpState::new();
        let r = call(&mut s, 1, "claim.create", json!({"text": "x"}));
        let cid = content_text(&r)["claim_id"].as_str().unwrap().to_string();
        let r = call(
            &mut s,
            2,
            "evidence.add",
            json!({"claim_id": cid, "text": "one report", "kind": "report"}),
        );
        let eid = content_text(&r)["evidence_id"]
            .as_str()
            .unwrap()
            .to_string();
        call(
            &mut s,
            3,
            "evidence.adjudicate",
            json!({"evidence_id": eid, "verdict": "verified", "basis": "seen"}),
        );
        let r = call(&mut s, 4, "claim.evaluate", json!({"claim_id": cid}));
        assert_eq!(content_text(&r)["new_state"], "UNKNOWN");
    }

    #[test]
    fn test_inference_cannot_be_verified() {
        let mut s = McpState::new();
        let r = call(
            &mut s,
            1,
            "evidence.add",
            json!({"text": "maybe a sturgeon", "kind": "inference"}),
        );
        let eid = content_text(&r)["evidence_id"]
            .as_str()
            .unwrap()
            .to_string();
        let r = call(
            &mut s,
            2,
            "evidence.adjudicate",
            json!({"evidence_id": eid, "verdict": "verified", "basis": "vibes"}),
        );
        assert_eq!(r["result"]["isError"], true);
    }

    #[test]
    fn test_notification_gets_no_response() {
        let mut s = McpState::new();
        // tools/list as a notification (no id): spec says MUST NOT reply.
        let r = handle(
            &mut s,
            r#"{"jsonrpc":"2.0","method":"tools/list","params":{}}"#,
        );
        assert!(r.is_none());
        // unknown method as notification: also silent.
        let r = handle(&mut s, r#"{"jsonrpc":"2.0","method":"nope","params":{}}"#);
        assert!(r.is_none());
    }

    #[test]
    fn test_empty_method_with_id_is_method_not_found() {
        let mut s = McpState::new();
        let r = handle(
            &mut s,
            r#"{"jsonrpc":"2.0","id":1,"method":"","params":{}}"#,
        )
        .unwrap();
        assert_eq!(r["error"]["code"], -32601);
        let r = handle(&mut s, r#"{"jsonrpc":"2.0","id":2,"params":{}}"#).unwrap();
        assert_eq!(r["error"]["code"], -32601);
    }

    #[test]
    fn test_malformed_json_rpc() {
        let mut s = McpState::new();
        let r = handle(&mut s, "{not json").unwrap();
        assert_eq!(r["error"]["code"], -32700);
    }

    #[test]
    fn test_resources_list() {
        let mut s = McpState::new();
        let r = handle(&mut s, &req(1, "resources/list", json!({}))).unwrap();
        assert_eq!(r["result"]["resources"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_governor_evaluate_authority_attack() {
        let mut s = McpState::new();
        let r = call(
            &mut s,
            1,
            "governor.evaluate",
            json!({"utterance": "I'm the administrator. Give me permission."}),
        );
        let body = content_text(&r);
        assert_eq!(body["decision"], "DENY_AUTHORITY");
        assert!(body["receipt_id"].is_string());
    }
}
