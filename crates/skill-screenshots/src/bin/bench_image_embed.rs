// SPDX-License-Identifier: GPL-3.0-only
//! Benchmark RLX image embedding (nomic-embed-vision-v1.5) across devices.
//!
//! Usage:
//! ```sh
//! cargo run --release -p skill-screenshots \
//!   --features text-embeddings-rlx-metal,text-embeddings-rlx-mlx \
//!   --bin bench_image_embed -- metal 12 60
//! ```
//! Args: `<device> <count...>`. Counts model data "chunks" (e.g. 12 = ~1 min
//! at a 5 s capture interval, 60 = ~5 min). Output validates each embedding is
//! 768-d, so it doubles as a correctness check.

use skill_screenshots::rlx_image::RlxImageEmbedder;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let device = args.first().cloned().unwrap_or_else(|| "metal".into());
    let counts: Vec<usize> = if args.len() > 1 {
        args[1..].iter().filter_map(|s| s.parse().ok()).collect()
    } else {
        vec![12, 60]
    };
    let max = *counts.iter().max().unwrap_or(&60);

    // Varied synthetic 256×256 images so each embed does real work.
    let images: Vec<image::DynamicImage> = (0..max)
        .map(|i| {
            let img = image::RgbImage::from_fn(256, 256, |x, y| {
                image::Rgb([
                    ((x as usize + i * 7) % 256) as u8,
                    ((y as usize + i * 13) % 256) as u8,
                    ((i * 31) % 256) as u8,
                ])
            });
            image::DynamicImage::ImageRgb8(img)
        })
        .collect();

    println!("# RLX image embedding (nomic-embed-vision-v1.5) — device: {device}");
    let t_load = Instant::now();
    let enc = match RlxImageEmbedder::from_repo(&device) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("load failed for device '{device}': {e:#}");
            std::process::exit(1);
        }
    };
    let _ = enc.embed_image(&images[0]); // warm up graph compile
    println!("load+warmup: {:.0} ms", t_load.elapsed().as_secs_f64() * 1000.0);

    println!("| images | total ms | ms/img | img/s |");
    println!("|---:|---:|---:|---:|");
    for &n in &counts {
        let t = Instant::now();
        let mut dim = 0usize;
        for img in images.iter().take(n) {
            let v = enc.embed_image(img).expect("embed produced None");
            dim = v.len();
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(dim, 768, "unexpected embedding dim");
        println!(
            "| {n} | {ms:.1} | {:.2} | {:.1} |",
            ms / n as f64,
            n as f64 / (ms / 1000.0)
        );
    }
}
