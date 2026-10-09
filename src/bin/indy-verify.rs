//! indy-verify: verify a receipt stream's hash-chain integrity using the
//! library's own verifier (no reimplemented crypto logic here — the point is
//! that the runtime's receipts check out under the same rules the unit tests
//! enforce).
//!
//! Usage: indy-verify [receipts.json]
//!   Reads a JSON array of Receipt objects from the file (or stdin),
//!   groups by request_id, reconstructs each chain, verifies linkage.
//!   Prints a verdict JSON to stdout. Exit 0 = VERIFIED, 1 = FAILED.

use clap::Parser;
use indy_reads::governor::{reconstruct_chain, verify_chain_linkage};
use indy_reads::receipts::Receipt;
use std::collections::BTreeMap;
use std::io::Read;

/// Verify a receipt stream's hash-chain integrity using the library's own
/// verifier (no reimplemented crypto logic here — the point is that the
/// runtime's receipts check out under the same rules the unit tests enforce).
#[derive(Parser, Debug)]
#[command(name = "indy-verify", about = "Verify Indy_Reads receipt hash chains")]
struct Args {
    /// JSON file with an array of Receipt objects (stdin if omitted).
    input: Option<String>,
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn main() {
    let args = Args::parse();
    let raw = if let Some(path) = args.input.as_deref() {
        std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("cannot read {}: {}", path, e);
            std::process::exit(2);
        })
    } else {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).unwrap();
        s
    };

    let receipts: Vec<Receipt> = match serde_json::from_str(&raw) {
        Ok(r) => r,
        Err(e) => {
            let v = serde_json::json!({
                "verdict": "FAILED",
                "reason": format!("receipts JSON did not parse: {}", e),
                "receipts": 0, "chains": 0,
            });
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
            std::process::exit(1);
        }
    };

    let mut by_request: BTreeMap<String, Vec<Receipt>> = BTreeMap::new();
    for r in receipts.iter().cloned() {
        by_request.entry(r.request_id.clone()).or_default().push(r);
    }

    let mut broken: Vec<String> = Vec::new();
    let mut malformed: Vec<String> = Vec::new();
    let mut failed_ops = 0usize;
    let chains = by_request.len();

    // Per-request reconstruction (the C5 claim: request → decision →
    // provider → result → state → receipt, reconstructable).
    for (req, group) in &by_request {
        // newest-first, in the API's own commit order (no timestamp sort:
        // reconstruct_chain just filters + reverses, preserving order).
        let newest_first = group.clone();
        for r in &newest_first {
            if r.prev_hash != "genesis" && !is_hex64(&r.prev_hash) {
                malformed.push(format!("{}:{} bad prev_hash", req, r.receipt_id));
            }
            if r.receipt_id.is_empty() || r.operation.is_empty() {
                malformed.push(format!("{}:{} empty id/operation", req, r.receipt_id));
            }
            if !r.success {
                failed_ops += 1;
            }
        }
        let chain = reconstruct_chain(&newest_first, req);
        if chain.is_empty() {
            broken.push(format!("{}: empty chain after reconstruction", req));
            continue;
        }
        if let Err(e) = verify_chain_linkage(&chain) {
            broken.push(format!("{}: {}", req, e));
        }
    }

    // Global chronological chain (the governor commits to ONE global
    // last_hash; cross-request linkage must also hold). The API returns
    // newest-first in commit order, so the true order is the plain reverse
    // — NOT a timestamp sort (timestamps collide at ms resolution).
    let mut chrono = receipts.clone();
    chrono.reverse();
    if let Err(e) = verify_chain_linkage(&chrono) {
        broken.push(format!("global: {}", e));
    }

    let verdict = if broken.is_empty() && malformed.is_empty() {
        "VERIFIED"
    } else {
        "FAILED"
    };
    let v = serde_json::json!({
        "verdict": verdict,
        "receipts": receipts.len(),
        "chains": chains,
        "failed_operations": failed_ops,
        "broken_links": broken,
        "malformed": malformed,
    });
    println!("{}", serde_json::to_string_pretty(&v).unwrap());
    std::process::exit(if verdict == "VERIFIED" { 0 } else { 1 });
}
