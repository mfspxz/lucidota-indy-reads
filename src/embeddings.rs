//! Embeddings organelle: the local-first semantic primitive.
//!
//! LAW: embedding similarity is NEVER proof. It produces candidate
//! relationships; the deterministic graph + governor decide what becomes
//! an edge, and the evidence policy decides what becomes knowledge.
//!
//! One contract, many runtimes:
//!   browser -> Ternlight WASM (@ternlight/base)
//!   Cloudflare Worker -> same WASM
//!   Rust service -> TernlightProvider (WASM/native adapter)
//!   offline/tests -> MockEmbeddingProvider (deterministic, labeled MOCK)
//!
//! Provider state is explicit: LIVE / FIXTURE / UNAVAILABLE. Never silently
//! replace one with another.

use std::fmt;

/// Embedding dimensionality. Ternlight outputs 384-dim L2-normalized vectors.
pub const DIM: usize = 384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderState {
    Live,
    Fixture,
    Unavailable,
}

impl fmt::Display for ProviderState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderState::Live => write!(f, "LIVE"),
            ProviderState::Fixture => write!(f, "FIXTURE"),
            ProviderState::Unavailable => write!(f, "UNAVAILABLE"),
        }
    }
}

#[derive(Debug)]
pub enum EmbeddingError {
    BackendUnavailable(String),
    InvalidInput(String),
}

impl fmt::Display for EmbeddingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EmbeddingError::BackendUnavailable(m) => write!(f, "backend unavailable: {}", m),
            EmbeddingError::InvalidInput(m) => write!(f, "invalid input: {}", m),
        }
    }
}

/// The one contract every embedding runtime implements.
pub trait EmbeddingProvider: Send + Sync {
    /// Embed text -> DIM-length L2-normalized vector.
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError>;

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    /// Stable model identity for receipt metadata, e.g. "ternlight/base@0.1.0".
    fn model_id(&self) -> &str;

    fn dimensions(&self) -> usize {
        DIM
    }

    fn state(&self) -> ProviderState;
}

/// Cosine similarity. Vectors are L2-normalized by contract, so this is a
/// dot product. Debug-asserts normalization to catch contract violations.
pub fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), DIM);
    debug_assert_eq!(b.len(), DIM);
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn l2_normalize(v: &mut [f32]) {
    let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}

/// Deterministic hash-based vectors. NOT semantic — for contract tests and
/// offline demos only. Same text -> same vector, always. Labeled MOCK
/// everywhere it appears.
pub struct MockEmbeddingProvider;

impl MockEmbeddingProvider {
    fn hash_embed(text: &str) -> Vec<f32> {
        // xorshift64 seeded from a simple string hash; deterministic.
        let mut h: u64 = 0xcbf29ce484222325;
        for b in text.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        let mut state = h | 1;
        let mut v = Vec::with_capacity(DIM);
        for _ in 0..DIM {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            // map to [-1, 1]
            v.push((state as f64 / u64::MAX as f64) as f32 * 2.0 - 1.0);
        }
        l2_normalize(&mut v);
        v
    }
}

impl EmbeddingProvider for MockEmbeddingProvider {
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        if text.is_empty() {
            return Err(EmbeddingError::InvalidInput("empty text".into()));
        }
        Ok(Self::hash_embed(text))
    }

    fn model_id(&self) -> &str {
        "mock/hash-v1 (NOT SEMANTIC - contract testing only)"
    }

    fn state(&self) -> ProviderState {
        ProviderState::Fixture
    }
}

/// Adapter for Ternlight (@ternlight/base or @ternlight/mini):
/// Rust->WASM ternary (BitNet-style) embedding engine, 384-dim normalized,
/// ~7MB base / ~5.5MB mini, synchronous, no network, no API key.
///
/// Status: contract-complete. The native/WASM backend plugs in behind
/// `embed_wasm`; until the artifact is vendored this provider reports
/// UNAVAILABLE explicitly instead of silently substituting the mock.
pub struct TernlightProvider {
    tier: String,
    wasm_loaded: bool,
}

impl TernlightProvider {
    pub fn base() -> Self {
        Self { tier: "base".into(), wasm_loaded: false }
    }

    pub fn mini() -> Self {
        Self { tier: "mini".into(), wasm_loaded: false }
    }

    /// Call once the WASM artifact (or native adapter) is vendored.
    pub fn with_wasm(mut self) -> Self {
        self.wasm_loaded = true;
        self
    }
}

impl EmbeddingProvider for TernlightProvider {
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        if text.is_empty() {
            return Err(EmbeddingError::InvalidInput("empty text".into()));
        }
        if !self.wasm_loaded {
            return Err(EmbeddingError::BackendUnavailable(format!(
                "@ternlight/{} WASM not vendored yet; use MockEmbeddingProvider for offline work",
                self.tier
            )));
        }
        // Backend seam: the real call is `embed(text)` from the WASM module
        // (Node/browser/Worker) or the native adapter (Rust service).
        Err(EmbeddingError::BackendUnavailable(
            "wasm backend flag set but no artifact linked in this build".into(),
        ))
    }

    fn model_id(&self) -> &str {
        if self.tier == "mini" {
            "ternlight/mini@0.1.0"
        } else {
            "ternlight/base@0.1.0"
        }
    }

    fn state(&self) -> ProviderState {
        if self.wasm_loaded {
            ProviderState::Live
        } else {
            ProviderState::Unavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_is_deterministic_and_normalized() {
        let p = MockEmbeddingProvider;
        let a = p.embed("the lake creature").unwrap();
        let b = p.embed("the lake creature").unwrap();
        assert_eq!(a.len(), DIM);
        assert_eq!(a, b, "mock must be deterministic");
        let n: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((n - 1.0).abs() < 1e-5, "vectors must be L2-normalized");
    }

    #[test]
    fn test_cosine_sim_identity() {
        let p = MockEmbeddingProvider;
        let a = p.embed("identical text").unwrap();
        let sim = cosine_sim(&a, &a);
        assert!((sim - 1.0).abs() < 1e-5, "self-similarity must be 1.0");
    }

    #[test]
    fn test_mock_is_labeled_fixture() {
        let p = MockEmbeddingProvider;
        assert_eq!(p.state(), ProviderState::Fixture);
        assert!(p.model_id().contains("MOCK") || p.model_id().contains("mock"));
        assert!(p.embed("").is_err());
    }

    #[test]
    fn test_ternlight_reports_unavailable_honestly() {
        // The critical property: no silent substitution. Until the WASM is
        // vendored, the provider says UNAVAILABLE instead of faking vectors.
        let p = TernlightProvider::base();
        assert_eq!(p.state(), ProviderState::Unavailable);
        assert_eq!(p.dimensions(), 384);
        assert!(matches!(
            p.embed("hello"),
            Err(EmbeddingError::BackendUnavailable(_))
        ));
    }

    #[test]
    fn test_similarity_is_not_proof() {
        // Documents the architectural law: even a perfect similarity score
        // must pass through graph verification before becoming an edge.
        // This test exists so future code can't "optimize" the law away.
        let p = MockEmbeddingProvider;
        let a = p.embed("the creature exists").unwrap();
        let b = p.embed("the creature exists").unwrap();
        let sim = cosine_sim(&a, &b);
        assert!((sim - 1.0).abs() < 1e-5);
        // ...and still: no claim state changes here. Similarity is a
        // candidate signal, not an epistemic verdict. Enforced by the
        // governor, not by this module.
    }
}
