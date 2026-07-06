// SPDX-License-Identifier: GPL-3.0-only
//! Benchmark RLX (`rlx-embed`) text embeddings across devices and batch sizes.
//!
//! Example:
//! ```sh
//! cargo run --release -p skill-daemon-state --features text-embeddings-rlx-metal \
//!   --bin bench_text_embeddings -- \
//!   --model nomic-ai/nomic-embed-text-v1.5 --rlx-device metal --batch-sizes 1,8,32
//! ```

use anyhow::{anyhow, Result};
use skill_daemon_state::text_embedder::SharedTextEmbedder;
use std::time::Instant;

#[derive(Debug, Clone)]
struct Args {
    model: String,
    rlx_device: String,
    rlx_max_seq: usize,
    batch_sizes: Vec<usize>,
    warmup: usize,
    runs: usize,
}

struct BenchRow {
    backend: &'static str,
    batch: usize,
    load_ms: f64,
    mean_ms: f64,
    docs_s: f64,
    dim: usize,
}

fn main() -> Result<()> {
    let args = parse_args()?;
    println!("# Text embedding backend benchmark");
    println!("model: {}", args.model);
    println!(
        "batch_sizes: {:?} - runs: {} - warmup: {} - rlx_device: {} - rlx_max_seq: {}",
        args.batch_sizes, args.runs, args.warmup, args.rlx_device, args.rlx_max_seq
    );

    let corpus = sample_texts(*args.batch_sizes.iter().max().unwrap_or(&1));
    let rows = run_backend(&args, &corpus)?;

    print_rows(&rows);
    Ok(())
}

fn run_backend(args: &Args, corpus: &[String]) -> Result<Vec<BenchRow>> {
    let embedder = SharedTextEmbedder::new();
    embedder.set_model_code(&args.model);
    embedder.set_rlx_device(&args.rlx_device);
    embedder.set_rlx_max_seq(args.rlx_max_seq);

    let t_load = Instant::now();
    if !embedder.reload() {
        return Err(anyhow!("failed to load rlx backend for {}", args.model));
    }
    let load_ms = t_load.elapsed().as_secs_f64() * 1000.0;

    let mut rows = Vec::new();
    for &batch in &args.batch_sizes {
        let texts: Vec<&str> = corpus.iter().take(batch).map(String::as_str).collect();
        for _ in 0..args.warmup {
            let _ = embedder.embed_batch(texts.clone());
        }

        let mut times = Vec::with_capacity(args.runs);
        let mut dim = 0usize;
        for _ in 0..args.runs {
            let t = Instant::now();
            let vecs = embedder
                .embed_batch(texts.clone())
                .ok_or_else(|| anyhow!("rlx embedding failed at batch {batch}"))?;
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            dim = vecs.first().map_or(0, Vec::len);
            times.push(ms);
        }

        let mean_ms = times.iter().sum::<f64>() / times.len().max(1) as f64;
        rows.push(BenchRow {
            backend: "rlx",
            batch,
            load_ms,
            mean_ms,
            docs_s: batch as f64 / (mean_ms / 1000.0),
            dim,
        });
    }

    Ok(rows)
}

fn parse_args() -> Result<Args> {
    let mut model = "nomic-ai/nomic-embed-text-v1.5".to_string();
    let mut rlx_device = if cfg!(target_os = "macos") { "metal" } else { "cpu" }.to_string();
    let mut rlx_max_seq = 512usize;
    let mut batch_sizes = vec![1, 8, 32];
    let mut warmup = 2usize;
    let mut runs = 10usize;

    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0usize;
    while i < argv.len() {
        let key = &argv[i];
        let mut value = || -> Result<String> {
            i += 1;
            argv.get(i).cloned().ok_or_else(|| anyhow!("missing value for {key}"))
        };
        match key.as_str() {
            "--model" => model = value()?,
            "--rlx-device" => rlx_device = value()?,
            "--rlx-max-seq" => rlx_max_seq = value()?.parse()?,
            "--batch-sizes" => {
                batch_sizes = value()?
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::parse)
                    .collect::<Result<Vec<_>, _>>()?;
            }
            "--warmup" => warmup = value()?.parse()?,
            "--runs" => runs = value()?.parse()?,
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(anyhow!("unknown flag {other}")),
        }
        i += 1;
    }

    Ok(Args {
        model,
        rlx_device,
        rlx_max_seq,
        batch_sizes,
        warmup,
        runs,
    })
}

fn sample_texts(n: usize) -> Vec<String> {
    let seeds = [
        "Working on a Rust refactor with local model inference.",
        "Reading documentation about Apple Metal graph execution.",
        "Debugging a semantic search issue in the label index.",
        "Reviewing EEG focus metrics during a coding session.",
        "Comparing ONNX Runtime and RLX embedding throughput.",
        "Writing a concise summary for a pull request.",
        "Investigating terminal activity embeddings for recent commands.",
        "Planning a benchmark for Qwen prompt prefill and decode.",
    ];
    (0..n)
        .map(|i| format!("{} Sample #{i}.", seeds[i % seeds.len()]))
        .collect()
}

fn print_rows(rows: &[BenchRow]) {
    println!();
    println!("| backend | batch | load ms | mean ms | docs/s | dim |");
    println!("|---|---:|---:|---:|---:|---:|");
    for row in rows {
        println!(
            "| {} | {} | {:.1} | {:.2} | {:.1} | {} |",
            row.backend, row.batch, row.load_ms, row.mean_ms, row.docs_s, row.dim
        );
    }
}

fn print_usage() {
    eprintln!(
        "Usage: bench_text_embeddings [--model HF_REPO] \
         [--rlx-device metal] [--rlx-max-seq 512] [--batch-sizes 1,8,32] \
         [--warmup 2] [--runs 10]"
    );
}
