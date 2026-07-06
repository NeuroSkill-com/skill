// SPDX-License-Identifier: GPL-3.0-only
//! RLX text-embedding semantic self-consistency check.
//!
//! Validates that `rlx-embed` produces meaningful embeddings: semantically
//! related sentences sit closer (higher cosine) than unrelated ones, and the
//! output has the expected width and is finite. This replaces the old
//! rlx-vs-fastembed parity test (fastembed has been removed — RLX is the
//! only text-embedding backend).
//!
//! Run:
//! ```sh
//! cargo test -p skill-daemon-state --release --test rlx_semantic \
//!   --features text-embeddings-rlx-metal -- --nocapture
//! ```

#![cfg(feature = "text-embeddings-rlx")]

use skill_daemon_state::text_embedder::SharedTextEmbedder;

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
    dot / (na.sqrt() * nb.sqrt()).max(1e-12)
}

#[test]
fn rlx_embeddings_are_semantically_consistent() {
    let model = "nomic-ai/nomic-embed-text-v1.5";
    let te = SharedTextEmbedder::new();
    te.set_model_code(model);
    assert!(te.reload(), "RLX reload failed (network or model issue?)");

    let anchor = "The cat sat on the warm windowsill in the afternoon sun.";
    let similar = "A kitten rested on the sunny window ledge.";
    let unrelated = "Quarterly corporate tax filings are due at the fiscal year end.";

    let a = te.embed(anchor).expect("rlx produced no embedding");
    let s = te.embed(similar).expect("rlx produced no embedding");
    let u = te.embed(unrelated).expect("rlx produced no embedding");

    // Expected width (nomic-embed-text-v1.5) and finiteness.
    assert_eq!(a.len(), 768, "unexpected embedding width: {}", a.len());
    assert!(a.iter().all(|x| x.is_finite()), "embedding contains non-finite values");

    let sim_close = cosine(&a, &s);
    let sim_far = cosine(&a, &u);
    eprintln!("cosine(anchor, similar)   = {sim_close:.4}");
    eprintln!("cosine(anchor, unrelated) = {sim_far:.4}");

    // The related pair must be clearly closer than the unrelated pair.
    assert!(
        sim_close > sim_far + 0.05,
        "related ({sim_close:.4}) should be clearly closer than unrelated ({sim_far:.4})"
    );
}

/// Validate the document re-embed (`search_document:`) at 1-minute (12) and
/// 5-minute (60) chunk scale, and report throughput — this is the text loop the
/// migration drives for labels / terminal / OCR.
#[test]
fn rlx_text_chunks_1min_and_5min() {
    let te = SharedTextEmbedder::new();
    te.set_model_code("nomic-ai/nomic-embed-text-v1.5");
    if !te.reload() {
        eprintln!("skipping: rlx reload failed (network/model issue?)");
        return;
    }
    assert!(te.needs_task_prefix(), "nomic model should use task prefixes");

    for &(label, n) in &[("1-min", 12usize), ("5-min", 60usize)] {
        let texts: Vec<String> = (0..n)
            .map(|i| format!("screenshot {i}: terminal output and OCR text sample number {i}"))
            .collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let t = std::time::Instant::now();
        let vecs = te.embed_documents(refs).expect("embed_documents returned None");
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(vecs.len(), n, "expected {n} embeddings");
        for v in &vecs {
            assert_eq!(v.len(), 768, "unexpected embedding width");
            assert!(v.iter().all(|x| x.is_finite()), "non-finite embedding");
        }
        eprintln!(
            "[{label} chunk] {n} docs in {ms:.0} ms ({:.1} docs/s)",
            n as f64 / (ms / 1000.0)
        );
    }
}
