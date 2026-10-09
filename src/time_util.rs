//! Wall-clock helpers (jiff).
//!
//! Additive only. The receipt hash chain keeps using `receipts::now_ms()`
//! (u64 millis) — nothing about chain format, hashing, or verification
//! changes here. This module is for human-facing timestamps (logs,
//! health output), where RFC 3339 is the honest format.

/// Current UTC time as RFC 3339 (`2026-10-08T18:30:00Z`).
pub fn now_rfc3339() -> String {
    jiff::Timestamp::now().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_parses_back() {
        let s = now_rfc3339();
        let ts: jiff::Timestamp = s.parse().expect("our own output must parse");
        assert!(ts.to_string() == s);
    }
}
