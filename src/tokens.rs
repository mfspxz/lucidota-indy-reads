//! Token budgeting for the 5% LLM slot (tiktoken-rs).
//!
//! The governor's LLM proposal slot is a cost and context-window surface.
//! Every byte sent to a provider is counted here first, in cl100k (OpenAI
//! family) tokens. Cross-vendor counts are estimates, never truth —
//! tiktoken is exact only for OpenAI-family tokenizers.

use std::sync::OnceLock;
use tiktoken_rs::{cl100k_base, CoreBPE};

static BPE: OnceLock<CoreBPE> = OnceLock::new();

fn bpe() -> &'static CoreBPE {
    BPE.get_or_init(|| {
        cl100k_base().expect("tiktoken cl100k BPE ranks are embedded; this cannot fail")
    })
}

/// Exact cl100k token count for `text`.
pub fn count_tokens(text: &str) -> usize {
    bpe().encode_ordinary(text).len()
}

/// True when `text` fits within `max_tokens` cl100k tokens.
pub fn fits_budget(text: &str, max_tokens: usize) -> bool {
    count_tokens(text) <= max_tokens
}

/// Truncate `text` to at most `max_tokens` cl100k tokens, decoding back to
/// a valid string. Truncation is from the tail — callers decide what is
/// expendable (usually the oldest transcript context, never the system
/// prompt's contract section).
pub fn truncate_to_budget(text: &str, max_tokens: usize) -> String {
    let bpe = bpe();
    let mut ranks = bpe.encode_ordinary(text);
    if ranks.len() > max_tokens {
        ranks.truncate(max_tokens);
    }
    bpe.decode(&ranks)
        .expect("decode of our own encode output cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("", 0)]
    #[case("hello", 1)]
    #[case("hello world", 2)]
    fn token_counts_are_sane(#[case] text: &str, #[case] expected: usize) {
        assert_eq!(count_tokens(text), expected);
    }

    #[rstest]
    #[case("hello world, this is a longer sentence for budgeting", 100, true)]
    #[case("hello world, this is a longer sentence for budgeting", 3, false)]
    fn fits_budget_cases(#[case] text: &str, #[case] budget: usize, #[case] fits: bool) {
        assert_eq!(fits_budget(text, budget), fits);
    }

    #[test]
    fn truncate_never_exceeds_budget() {
        let text = "the quick brown fox jumps over the lazy dog ".repeat(50);
        let out = truncate_to_budget(&text, 10);
        assert!(count_tokens(&out) <= 10);
        assert!(!out.is_empty());
    }

    #[test]
    fn truncate_is_prefix_stable() {
        // Truncating to a budget larger than the text is a no-op.
        let text = "short";
        assert_eq!(truncate_to_budget(text, 1000), text);
    }

    #[test]
    fn token_count_snapshot() {
        // Pinned so a tiktoken-rs upgrade that changes counts is a
        // deliberate review, not a silent drift.
        insta::assert_snapshot!(count_tokens("Indy_Reads governor turn").to_string(), @"6");
    }
}
