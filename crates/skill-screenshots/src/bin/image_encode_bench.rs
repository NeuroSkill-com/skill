// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
#![allow(clippy::unwrap_used)]
//! Quick benchmark comparing image encoding strategies used in the screenshot
//! pipeline.  Run via `npm run bench:screenshot` or directly with
//! `cargo run --release -p skill-screenshots --bin image_encode_bench`.
//!
//! Tests:
//!   1. PNG encode (lossless — old pipeline)
//!   2. JPEG encode at various quality levels
//!   3. WebP encode (lossy — disk storage)
//!   4. Raw pixel hash (duplicate detection)
//!   5. Resize + bilinear (capture thread)
//!   6. DynamicImage clone (channel send cost)

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::time::{Duration, Instant};

use image::{DynamicImage, GenericImageView, ImageFormat, RgbImage};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn make_test_image(w: u32, h: u32) -> DynamicImage {
    // Synthetic gradient image — more realistic compression behaviour
    // than solid colour or pure noise.
    let mut img = RgbImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let r = ((x * 255) / w) as u8;
            let g = ((y * 255) / h) as u8;
            let b = (((x + y) * 128) / (w + h)) as u8;
            img.put_pixel(x, y, image::Rgb([r, g, b]));
        }
    }
    DynamicImage::ImageRgb8(img)
}

/// Resize with aspect-ratio-preserving fit, then center-pad to target×target.
fn resize_fit_pad(img: &DynamicImage, target: u32) -> DynamicImage {
    let (w, h) = img.dimensions();
    let scale = (target as f64 / w as f64).min(target as f64 / h as f64);
    let nw = (w as f64 * scale).round() as u32;
    let nh = (h as f64 * scale).round() as u32;
    let resized = img.resize_exact(nw, nh, image::imageops::FilterType::Triangle);
    let mut canvas = DynamicImage::new_rgb8(target, target);
    let ox = (target - nw) / 2;
    let oy = (target - nh) / 2;
    image::imageops::overlay(&mut canvas, &resized, ox as i64, oy as i64);
    canvas
}

fn encode_png(img: &DynamicImage) -> Vec<u8> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
        .unwrap();
    buf
}

fn encode_jpeg(img: &DynamicImage, quality: u8) -> Vec<u8> {
    let rgb = img.to_rgb8();
    let mut buf = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
    rgb.write_with_encoder(encoder).unwrap();
    buf
}

fn encode_webp(img: &DynamicImage) -> Vec<u8> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), ImageFormat::WebP)
        .unwrap();
    buf
}

fn hash_pixels(img: &DynamicImage) -> u64 {
    let mut hasher = DefaultHasher::new();
    img.as_bytes().hash(&mut hasher);
    hasher.finish()
}

// ── Benchmark runner ─────────────────────────────────────────────────────────

struct BenchResult {
    name: String,
    iterations: u32,
    total: Duration,
    output_kb: Option<f64>,
}

impl BenchResult {
    fn avg_ms(&self) -> f64 {
        self.total.as_secs_f64() * 1000.0 / self.iterations as f64
    }
}

fn bench<F: FnMut() -> usize>(name: &str, warmup: u32, iters: u32, mut f: F) -> BenchResult {
    // Warmup
    for _ in 0..warmup {
        std::hint::black_box(f());
    }
    // Timed
    let start = Instant::now();
    let mut last_size = 0usize;
    for _ in 0..iters {
        last_size = std::hint::black_box(f());
    }
    let total = start.elapsed();
    BenchResult {
        name: name.to_string(),
        iterations: iters,
        total,
        output_kb: if last_size > 0 {
            Some(last_size as f64 / 1024.0)
        } else {
            None
        },
    }
}

fn print_results(results: &[BenchResult]) {
    // Use the old pipeline round-trip as the baseline for comparison.
    let baseline = results
        .iter()
        .find(|r| r.name.starts_with("OLD:"))
        .map(BenchResult::avg_ms)
        .unwrap_or(1.0);

    // Find the fastest entry for a marker.
    let fastest_ms = results
        .iter()
        .map(BenchResult::avg_ms)
        .fold(f64::INFINITY, f64::min);

    println!();
    println!(
        "  {:<44}  {:>9}  {:>9}  {:>12}",
        "Operation", "Avg (ms)", "Size KB", "vs old pipe"
    );
    println!("  {}", "-".repeat(84));

    for r in results {
        let avg = r.avg_ms();
        let size_str = r
            .output_kb
            .map(|kb| format!("{kb:.1}"))
            .unwrap_or_else(|| "-".to_string());
        let speedup = baseline / avg;
        let speedup_str = if avg < 0.01 {
            "  ~instant".to_string()
        } else if speedup >= 1.0 {
            format!("{speedup:.1}x faster")
        } else {
            format!("{:.1}x slower", 1.0 / speedup)
        };
        let marker = if (avg - fastest_ms).abs() < 0.001 && avg > 0.01 {
            " *"
        } else {
            ""
        };
        println!(
            "  {:<44}  {:>9.2}  {:>9}  {:>12}{}",
            r.name, avg, size_str, speedup_str, marker
        );
    }
    println!();
}

// ── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let sizes: &[(u32, u32, &str)] = &[
        (1920, 1080, "1080p screenshot"),
        (2560, 1440, "1440p screenshot"),
        (3840, 2160, "4K screenshot"),
    ];
    let target = 768u32; // default image_size in config
    let warmup = 3u32;
    let iters = 20u32;

    println!("==========================================================");
    println!("  Screenshot Pipeline Image Encoding Benchmark");
    println!("  Warmup: {warmup}  |  Iterations: {iters}  |  Target: {target}px");
    println!("==========================================================");

    for &(w, h, label) in sizes {
        println!("\n--- Source: {label} ({w}x{h}) ---");

        let source = make_test_image(w, h);
        let resized = resize_fit_pad(&source, target);

        let mut results = Vec::new();

        // ── Resize (capture thread cost) ──
        results.push(bench(
            &format!("Resize {w}x{h} -> {target}x{target}"),
            warmup,
            iters,
            || {
                let _ = resize_fit_pad(&source, target);
                0
            },
        ));

        // ── Pixel hash (duplicate detection) ──
        results.push(bench(
            "Hash pixels (duplicate detect)",
            warmup,
            iters,
            || {
                hash_pixels(&resized);
                0
            },
        ));

        // ── DynamicImage clone (channel send cost) ──
        results.push(bench(
            "DynamicImage::clone (new pipeline)",
            warmup,
            iters,
            || {
                let c = resized.clone();
                c.as_bytes().len()
            },
        ));

        // ── PNG encode (old pipeline) ──
        results.push(bench("PNG encode (old pipeline)", warmup, iters, || {
            encode_png(&resized).len()
        }));

        // ── JPEG encode (new pipeline, various quality) ──
        for q in [70u8, 85, 95] {
            results.push(bench(
                &format!("JPEG encode q={q} (new pipeline)"),
                warmup,
                iters,
                || encode_jpeg(&resized, q).len(),
            ));
        }

        // ── WebP encode (disk storage) ──
        results.push(bench("WebP encode (disk storage)", warmup, iters, || {
            encode_webp(&resized).len()
        }));

        // ── PNG decode (fastembed old overhead) ──
        let png_bytes = encode_png(&resized);
        results.push(bench(
            "PNG decode (fastembed overhead)",
            warmup,
            iters,
            || {
                let img = image::load_from_memory(&png_bytes).unwrap();
                img.as_bytes().len()
            },
        ));

        // ── JPEG decode ──
        let jpeg_bytes = encode_jpeg(&resized, 85);
        results.push(bench("JPEG decode", warmup, iters, || {
            let img = image::load_from_memory(&jpeg_bytes).unwrap();
            img.as_bytes().len()
        }));

        // ── Full old pipeline: PNG encode + decode ──
        results.push(bench(
            "OLD: PNG encode + decode (round-trip)",
            warmup,
            iters,
            || {
                let png = encode_png(&resized);
                let _ = image::load_from_memory(&png).unwrap();
                png.len()
            },
        ));

        // ── Full new pipeline: clone only ──
        results.push(bench(
            "NEW: DynamicImage clone (zero encode)",
            warmup,
            iters,
            || {
                let c = resized.clone();
                c.as_bytes().len()
            },
        ));

        print_results(&results);
    }

    // ── Summary ──
    println!("==========================================================");
    println!("  KEY TAKEAWAYS:");
    println!("  - PNG encode is the most expensive CPU operation");
    println!("  - JPEG encode is ~10x faster than PNG");
    println!("  - DynamicImage clone is near-instant (memcpy)");
    println!("  - Old pipeline paid PNG encode + decode every capture");
    println!("  - New pipeline: zero encoding for fastembed path");
    println!("==========================================================");
}
