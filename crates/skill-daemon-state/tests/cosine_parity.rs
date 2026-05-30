//! Cosine-distance parity: rlx text embedder vs fastembed on identical
//! input strings. Both backends must be enabled (`text-embeddings-fastembed`
//! + `text-embeddings-rlx`) for the test to run — otherwise the file
//! compiles to a no-op so the workspace `cargo test` doesn't break.
//!
//! Why cosine and not bit-exact: fastembed uses ONNX (ort/rten) with
//! INT8 weights for many models, while rlx-embed runs F32 inference
//! over the same model architecture. Outputs are numerically close
//! but not bit-identical; cosine similarity ≥ 0.99 on every test
//! string is the operational parity bar.
//!
//! Run:
//! ```sh
//! cargo test -p skill-daemon-state --release --test cosine_parity \
//!   --features text-embeddings-fastembed,text-embeddings-rlx-metal -- --nocapture
//! ```

#![cfg(all(feature = "text-embeddings-fastembed", feature = "text-embeddings-rlx"))]

use skill_daemon_state::text_embedder::{SharedTextEmbedder, TextEmbeddingBackend};

const CASES: &[&str] = &[
    "hello world",
    "the quick brown fox jumps over the lazy dog",
    "transformer attention scales as O(n^2) in sequence length",
    "Apple Silicon, with its unified memory architecture, makes Metal compute kernels especially efficient.",
    "Multilingual embedding: 嗨 こんにちは 안녕 مرحبا",
];

/// Cosine similarity between two equal-length vectors.
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "embedding dim mismatch: {} vs {}", a.len(), b.len());
    let mut dot = 0f32;
    let mut na = 0f32;
    let mut nb = 0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let n = (na.sqrt() * nb.sqrt()).max(1e-12);
    dot / n
}

#[test]
fn rlx_vs_fastembed_cosine_at_least_099() {
    // Use BGE-small-en-v1.5 — supported by both backends.
    let model = "BAAI/bge-small-en-v1.5";

    let fe = SharedTextEmbedder::new();
    fe.set_model_code(model);
    fe.set_backend(TextEmbeddingBackend::FastEmbed);
    assert!(fe.reload(), "FastEmbed reload failed (network or model issue?)");

    let rl = SharedTextEmbedder::new();
    rl.set_model_code(model);
    rl.set_backend(TextEmbeddingBackend::Rlx);
    assert!(rl.reload(), "RLX reload failed (network or model issue?)");

    let mut min_cos = 1.0f32;
    for s in CASES {
        let a = fe.embed(s).expect("fastembed produced no embedding");
        let b = rl.embed(s).expect("rlx produced no embedding");
        let c = cosine(&a, &b);
        eprintln!("cosine({s:?}) = {c:.6}");
        min_cos = min_cos.min(c);
        assert!(c >= 0.99, "cosine {c:.4} below 0.99 threshold for input {s:?}");
    }
    eprintln!("min cosine across cases: {min_cos:.6}");
}
