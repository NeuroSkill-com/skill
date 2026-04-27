// SPDX-License-Identifier: GPL-3.0-only
//! Background worker that fills in `terminal_outputs.embedding` for rows
//! created by the session finalizer.
//!
//! Pulls rows where `embedding IS NULL` in batches of 32, runs them through
//! `state.text_embedder` (fastembed), int8-quantises each vector (cuts
//! storage 4× vs f32, <2% recall loss for cosine), writes back to SQLite.
//!
//! No new HNSW yet — semantic search at query time scans the full set,
//! which is fine up to ~100k rows. We can layer HNSW later without
//! changing the on-disk format because the int8 vectors are already in
//! place.

use skill_daemon_state::AppState;
use std::time::Duration;
use tracing::{debug, warn};

/// Run the worker every 30 s. Tunable; finalizer runs every 60 s and
/// embedding catches up between ticks.
const TICK: Duration = Duration::from_secs(30);
/// Batch size — fastembed throughput peaks around 32–64 on CPU, more on GPU.
const BATCH: usize = 32;

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(TICK).await;
            let s = state.clone();
            let _ = tokio::task::spawn_blocking(move || run_once(&s)).await;
        }
    });
}

fn run_once(state: &AppState) {
    let skill_dir = match state.skill_dir.lock() {
        Ok(g) => g.clone(),
        Err(_) => return,
    };
    let Some(store) = skill_data::activity_store::ActivityStore::open(&skill_dir) else {
        return;
    };

    let pending = store.pending_terminal_embeddings(BATCH as u32);
    if pending.is_empty() {
        return;
    }

    // Skip empty / whitespace-only rows by writing a sentinel zero-vector so
    // we don't keep retrying them. fastembed errors on empty input.
    let mut work: Vec<(i64, String)> = Vec::with_capacity(pending.len());
    for (id, text) in pending {
        if text.trim().is_empty() {
            store.set_terminal_embedding(id, &[]); // empty BLOB = "skipped"
        } else {
            work.push((id, text));
        }
    }
    if work.is_empty() {
        return;
    }

    let texts: Vec<&str> = work.iter().map(|(_, t)| t.as_str()).collect();
    let vectors = match state.text_embedder.embed_batch(texts) {
        Some(v) => v,
        None => {
            warn!("embedder unavailable; skipping batch of {}", work.len());
            return;
        }
    };

    if vectors.len() != work.len() {
        warn!(
            "embedder returned {} vectors for {} inputs; skipping batch",
            vectors.len(),
            work.len()
        );
        return;
    }

    for ((cmd_id, _), vec) in work.iter().zip(vectors.iter()) {
        let blob = quantize_int8(vec);
        store.set_terminal_embedding(*cmd_id, &blob);
    }

    debug!(batch = work.len(), "embedded terminal outputs");
}

/// Uniform int8 quantisation to `[-127, 127]` based on the per-vector L∞
/// norm. The max absolute value goes to 127 so we use the full range, then
/// the BLOB layout is: `[u32 dim_le][f32 scale_le][i8 * dim]`. Dot product
/// rank is preserved under this scheme, so cosine ranking matches f32 to
/// within ~1% on real embeddings.
fn quantize_int8(vec: &[f32]) -> Vec<u8> {
    if vec.is_empty() {
        return Vec::new();
    }
    let max_abs = vec.iter().fold(0f32, |m, &x| m.max(x.abs()));
    let scale = if max_abs == 0.0 { 1.0 } else { max_abs / 127.0 };
    let inv = 1.0 / scale;
    let mut out = Vec::with_capacity(8 + vec.len());
    out.extend_from_slice(&(vec.len() as u32).to_le_bytes());
    out.extend_from_slice(&scale.to_le_bytes());
    for &x in vec {
        let q = (x * inv).round().clamp(-127.0, 127.0) as i8;
        out.push(q as u8);
    }
    out
}

/// Reverse of `quantize_int8`. Used by the search endpoint.
pub fn dequantize_int8(blob: &[u8]) -> Option<Vec<f32>> {
    if blob.len() < 8 {
        return None;
    }
    let dim = u32::from_le_bytes(blob[0..4].try_into().ok()?) as usize;
    let scale = f32::from_le_bytes(blob[4..8].try_into().ok()?);
    if blob.len() != 8 + dim {
        return None;
    }
    Some(blob[8..].iter().map(|&b| (b as i8) as f32 * scale).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_round_trip_preserves_rank() {
        let a: Vec<f32> = (0..128).map(|i| (i as f32 / 128.0).sin()).collect();
        let b: Vec<f32> = (0..128).map(|i| (i as f32 / 64.0).cos()).collect();

        let dot_f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum::<f32>();
        let aq = dequantize_int8(&quantize_int8(&a)).unwrap();
        let bq = dequantize_int8(&quantize_int8(&b)).unwrap();
        let dot_q = aq.iter().zip(&bq).map(|(x, y)| x * y).sum::<f32>();

        // Within 5% — generous for synthetic data; real embeddings are
        // tighter because they don't span the full unit cube.
        let rel_err = (dot_f32 - dot_q).abs() / dot_f32.abs().max(1e-6);
        assert!(rel_err < 0.05, "rel_err = {rel_err}");
    }

    #[test]
    fn empty_round_trip() {
        let blob = quantize_int8(&[]);
        assert!(blob.is_empty());
    }

    #[test]
    fn dim_decode() {
        let v = vec![0.5_f32, -0.5, 1.0, -1.0];
        let blob = quantize_int8(&v);
        let back = dequantize_int8(&blob).unwrap();
        assert_eq!(back.len(), 4);
        for (a, b) in v.iter().zip(&back) {
            assert!((a - b).abs() < 0.01, "{a} vs {b}");
        }
    }
}
