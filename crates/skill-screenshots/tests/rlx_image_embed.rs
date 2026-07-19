// SPDX-License-Identifier: GPL-3.0-only
//! RLX image-embedding validation — the nomic-vision re-embed path used by the
//! screenshot backfill / migration. Validates single embeddings plus 1-minute
//! and 5-minute data "chunks" (12 / 60 screenshots at a ~5 s capture interval)
//! end to end, with timing.
//!
//! Run:
//! ```sh
//! cargo test -p skill-screenshots --test rlx_image_embed \
//!   --features text-embeddings-rlx-metal -- --nocapture
//! ```

#![cfg(feature = "text-embeddings-rlx")]

use skill_screenshots::rlx_image::RlxImageEmbedder;
use std::time::Instant;

fn device() -> &'static str {
    if cfg!(target_os = "macos") {
        "metal"
    } else {
        "cpu"
    }
}

/// Deterministic, varied 256×256 image #`i`.
fn synth_image(i: usize) -> image::DynamicImage {
    let img = image::RgbImage::from_fn(256, 256, |x, y| {
        image::Rgb([
            ((x as usize + i * 7) % 256) as u8,
            ((y as usize + i * 13) % 256) as u8,
            ((i * 31) % 256) as u8,
        ])
    });
    image::DynamicImage::ImageRgb8(img)
}

fn assert_unit_768(v: &[f32]) {
    assert_eq!(v.len(), 768, "unexpected embedding width: {}", v.len());
    assert!(v.iter().all(|x| x.is_finite()), "embedding has non-finite values");
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!(
        (norm - 1.0).abs() < 0.05,
        "expected L2-normalized output, got {norm:.4}"
    );
}

#[test]
fn rlx_image_embedder_produces_normalized_768d() {
    let enc = match RlxImageEmbedder::from_repo(device()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("skipping: rlx image embedder load failed (network/model?): {e:#}");
            return;
        }
    };
    let v = enc.embed_image(&synth_image(0)).expect("rlx image embed returned None");
    assert_unit_768(&v);
}

/// Validate the re-embed at 1-minute (12) and 5-minute (60) chunk scale, and
/// report throughput — this is the loop the migration/backfill drives.
#[test]
fn rlx_image_chunks_1min_and_5min() {
    let enc = match RlxImageEmbedder::from_repo(device()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("skipping: rlx image embedder load failed (network/model?): {e:#}");
            return;
        }
    };
    let _ = enc.embed_image(&synth_image(0)); // warm up graph compile

    for &(label, n) in &[("1-min", 12usize), ("5-min", 60usize)] {
        let t = Instant::now();
        for i in 0..n {
            let v = enc.embed_image(&synth_image(i)).expect("embed produced None");
            assert_unit_768(&v);
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "[{label} chunk] {n} images on {} in {ms:.0} ms ({:.1} img/s)",
            device(),
            n as f64 / (ms / 1000.0)
        );
    }
}
