//! Simple timing benchmarks (run with `cargo bench --bench bench`).
//!
//! Results are printed to stdout **and** written as JSONL to
//! `figures/bench.jsonl` so the plotting scripts can consume them without
//! scraping terminal output.
//!
//! ## JSONL schema
//!
//! Each line is one self-contained JSON object.  Two record types:
//!
//! ```json
//! {"type":"insert","n":10000,"dim":128,"total_s":2.67,"per_insert_us":266.95}
//! {"type":"search","n":10000,"dim":128,"k":10,"ef":50,"qps":9319.0,"per_query_us":107.31}
//! ```

use std::fs;
use std::io::Write as IoWrite;
use std::time::Instant;

use fast_hnsw::{Builder, Hnsw};
use fast_hnsw::distance::Euclidean;
use rand::{Rng, SeedableRng};

// Absolute path to figures/ baked in at compile time.
const FIGURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/figures");

// ─── Data generation ──────────────────────────────────────────────────────────

fn random_vectors(n: usize, dim: usize, seed: u64) -> Vec<Vec<f32>> {
    let mut rng = rand::rngs::SmallRng::seed_from_u64(seed);
    (0..n)
        .map(|_| (0..dim).map(|_| rng.gen::<f32>()).collect())
        .collect()
}

// ─── Record types ─────────────────────────────────────────────────────────────

enum Record {
    Insert {
        n:             usize,
        dim:           usize,
        total_s:       f64,
        per_insert_us: f64,
    },
    Search {
        n:             usize,
        dim:           usize,
        k:             usize,
        ef:            usize,
        qps:           f64,
        per_query_us:  f64,
    },
}

impl Record {
    fn to_json(&self) -> String {
        match self {
            Record::Insert { n, dim, total_s, per_insert_us } => format!(
                r#"{{"type":"insert","n":{n},"dim":{dim},"total_s":{total_s:.4},"per_insert_us":{per_insert_us:.2}}}"#
            ),
            Record::Search { n, dim, k, ef, qps, per_query_us } => format!(
                r#"{{"type":"search","n":{n},"dim":{dim},"k":{k},"ef":{ef},"qps":{qps:.1},"per_query_us":{per_query_us:.2}}}"#
            ),
        }
    }
}

// ─── Benchmarks ───────────────────────────────────────────────────────────────

fn bench_insert(n: usize, dim: usize) -> Record {
    let vecs = random_vectors(n, dim, 1);
    let mut index: Hnsw<Euclidean> = Builder::new()
        .m(16)
        .ef_construction(200)
        .seed(42)
        .build(Euclidean);

    let t = Instant::now();
    for v in vecs {
        index.insert(v);
    }
    let elapsed = t.elapsed();
    let total_s       = elapsed.as_secs_f64();
    let per_insert_us = total_s * 1e6 / n as f64;

    println!(
        "insert  n={n:>6}  dim={dim:>4}  total={:.2?}  per_insert={:.2?}",
        elapsed,
        elapsed / n as u32,
    );

    Record::Insert { n, dim, total_s, per_insert_us }
}

fn bench_search(n: usize, dim: usize, k: usize, ef: usize) -> Record {
    let vecs    = random_vectors(n, dim, 2);
    let queries = random_vectors(1000, dim, 3);

    let mut index: Hnsw<Euclidean> = Builder::new()
        .m(16)
        .ef_construction(200)
        .seed(42)
        .build(Euclidean);
    for v in vecs {
        index.insert(v);
    }

    let t = Instant::now();
    let mut sink = 0usize;
    for q in &queries {
        sink += index.search(q, k, ef).len();
    }
    let elapsed      = t.elapsed();
    let nq           = queries.len();
    let qps          = nq as f64 / elapsed.as_secs_f64();
    let per_query_us = elapsed.as_secs_f64() * 1e6 / nq as f64;

    println!(
        "search  n={n:>6}  dim={dim:>4}  k={k}  ef={ef:>4}  qps={qps:.0}  per_query={:.2?}  (sink={sink})",
        elapsed / nq as u32,
    );

    Record::Search { n, dim, k, ef, qps, per_query_us }
}

// ─── Workload tables (ordered by n asc, dim asc, ef asc) ─────────────────────

/// (n, dim)
const INSERT_DEFAULT: &[(usize, usize)] = &[
    ( 10_000, 128),
    ( 10_000, 512),
    ( 50_000, 128),
];

const INSERT_FULL: &[(usize, usize)] = &[
    (  10_000, 128),
    (  10_000, 512),
    (  50_000, 128),
    ( 100_000, 128),
    ( 200_000, 128),
    ( 300_000, 128),
    ( 400_000, 128),
    ( 500_000, 128),
    ( 600_000, 128),
    ( 700_000, 128),
    ( 800_000, 128),
    ( 900_000, 128),
    (1_000_000, 128),
];

/// (n, dim, k, ef)
const SEARCH_DEFAULT: &[(usize, usize, usize, usize)] = &[
    (10_000, 128, 10,  50),
    (10_000, 128, 10, 200),
    (10_000, 512, 10,  50),
    (50_000, 128, 10,  50),
];

const SEARCH_FULL: &[(usize, usize, usize, usize)] = &[
    (   10_000, 128, 10,  50),
    (   10_000, 128, 10, 200),
    (   10_000, 512, 10,  50),
    (   50_000, 128, 10,  50),
    (  100_000, 128, 10,  50),
    (  200_000, 128, 10,  50),
    (  300_000, 128, 10,  50),
    (  400_000, 128, 10,  50),
    (  500_000, 128, 10,  50),
    (  600_000, 128, 10,  50),
    (  700_000, 128, 10,  50),
    (  800_000, 128, 10,  50),
    (  900_000, 128, 10,  50),
    (1_000_000, 128, 10,  50),
];

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let full = std::env::args().any(|a| a == "--full");

    let insert_cfgs = if full { INSERT_FULL  } else { INSERT_DEFAULT  };
    let search_cfgs = if full { SEARCH_FULL  } else { SEARCH_DEFAULT  };

    println!("=== HNSW Benchmarks ({}) ===",
             if full { "full — up to 1M" } else { "default — up to 50k; pass --full for more" });
    println!();

    // Open output file up-front so every result is flushed to disk as it arrives.
    let out_path = format!("{FIGURES_DIR}/bench.jsonl");
    let mut out: Option<fs::File> = match fs::File::create(&out_path) {
        Ok(f)  => { println!("→ streaming results to {out_path}\n"); Some(f) }
        Err(e) => { eprintln!("warn: could not create {out_path}: {e}"); None }
    };

    let mut write = |rec: Record| {
        if let Some(f) = &mut out {
            let _ = writeln!(f, "{}", rec.to_json());
        }
    };

    println!("-- Insert --");
    for &(n, dim) in insert_cfgs {
        write(bench_insert(n, dim));
    }

    println!("\n-- Search --");
    for &(n, dim, k, ef) in search_cfgs {
        write(bench_search(n, dim, k, ef));
    }

    if out.is_some() {
        println!("\n→ done writing {out_path}");
    }
}
