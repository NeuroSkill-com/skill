//! Head-to-head benchmark: our pure-Rust HNSW vs. `hnsw_rs` and `hnsw` (v0.11).
//!
//! Run with:
//!   cargo bench --bench compare
//!
//! Results are printed to stdout **and** written as JSONL to
//! `figures/compare.jsonl` so the plotting scripts can consume them.
//!
//! ## Libraries compared
//!
//! | crate       | version | notes |
//! |-------------|---------|-------|
//! | ours        | (local) | this repo |
//! | hnsw_rs     | 0.3.3   | Jean-Pierre Both; uses Rayon+RwLock internally |
//! | hnsw_ext    | 0.11.0  | rust-cv / Geordon Worley; const-generic M; external Searcher |
//!
//! ## JSONL schema (one object per line, one line per workload × library)
//!
//! ```json
//! {"n":1000,"dim":32,"lib":"ours",
//!  "ins_per_s":18702.1,"per_ins_us":53.5,
//!  "ef50_qps":41209.0,"ef50_per_q_us":24.3,"ef50_recall_pct":99.8,
//!  "ef200_qps":15521.0,"ef200_per_q_us":64.4,"ef200_recall_pct":100.0,
//!  "ef500_qps":8405.0,"ef500_per_q_us":119.0,"ef500_recall_pct":100.0}
//! ```

use std::collections::HashSet;
use std::fs;
use std::io::Write as IoWrite;
use std::time::Instant;

// ── our implementation ──────────────────────────────────────────────────────
use fast_hnsw::{Builder, Hnsw};
use fast_hnsw::distance::Euclidean;

// ── hnsw_rs ─────────────────────────────────────────────────────────────────
use hnsw_rs::hnsw::Hnsw as HnswRs;
use hnsw_rs::anndists::dist::distances::DistL2;

// ── hnsw v0.11 (rust-cv) ────────────────────────────────────────────────────
use hnsw_ext::{Hnsw as HnswV0, Params as ParamsV0, Searcher as SearcherV0};
use rand_pcg::Pcg64;
use space::Metric as SpaceMetric;
use space::Neighbor;

// ── utilities ────────────────────────────────────────────────────────────────
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;

// Absolute path to figures/ baked in at compile time.
const FIGURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/figures");

// ─── Parameters ──────────────────────────────────────────────────────────────

const M:              usize = 16;
const M0:             usize = M * 2;   // = 32; must be a literal for hnsw_ext const generic
const EF_CONSTRUCTION: usize = 200;
const K:              usize = 10;
const N_QUERIES:      usize = 500;

/// ef values tested during search.
const EFS: [usize; 3] = [50, 200, 500];

/// Default workloads — up to 50k; fast enough to run routinely.
const WORKLOADS_DEFAULT: &[(usize, usize, &str)] = &[
    ( 1_000,  32,  "n=1k   dim=32 "),
    ( 1_000, 128,  "n=1k   dim=128"),
    (10_000,  32,  "n=10k  dim=32 "),
    (10_000, 128,  "n=10k  dim=128"),
    (50_000, 128,  "n=50k  dim=128"),
];

/// Full workloads — 100k → 1M in 100k steps (pass `--full` to enable).
/// Warning: takes ~hours with three libraries; run offline.
const WORKLOADS_FULL: &[(usize, usize, &str)] = &[
    (   1_000,  32,  "n=1k   dim=32 "),
    (   1_000, 128,  "n=1k   dim=128"),
    (  10_000,  32,  "n=10k  dim=32 "),
    (  10_000, 128,  "n=10k  dim=128"),
    (  50_000, 128,  "n=50k  dim=128"),
    ( 100_000, 128,  "n=100k dim=128"),
    ( 200_000, 128,  "n=200k dim=128"),
    ( 300_000, 128,  "n=300k dim=128"),
    ( 400_000, 128,  "n=400k dim=128"),
    ( 500_000, 128,  "n=500k dim=128"),
    ( 600_000, 128,  "n=600k dim=128"),
    ( 700_000, 128,  "n=700k dim=128"),
    ( 800_000, 128,  "n=800k dim=128"),
    ( 900_000, 128,  "n=900k dim=128"),
    (1_000_000, 128, "n=1M   dim=128"),
];

// ─── hnsw v0.11 metric ───────────────────────────────────────────────────────

/// Euclidean (L2) metric for the `hnsw` v0.11 crate.
///
/// The `space::Metric` trait requires an unsigned integer unit.  We use `u32`
/// and encode the f32 distance via `f32::to_bits()`.  IEEE 754 guarantees that
/// for non-negative finite values, bit-pattern order == numeric order, so this
/// preserves the nearest-neighbour ranking exactly.
struct EuclideanV0;

impl SpaceMetric<Vec<f32>> for EuclideanV0 {
    type Unit = u32;
    #[inline]
    fn distance(&self, a: &Vec<f32>, b: &Vec<f32>) -> u32 {
        a.iter()
            .zip(b.iter())
            .map(|(&x, &y)| (x - y) * (x - y))
            .sum::<f32>()
            .sqrt()
            .to_bits()
    }
}

// ─── Data generation ─────────────────────────────────────────────────────────

fn gen_vectors(n: usize, dim: usize, seed: u64) -> Vec<Vec<f32>> {
    let mut rng = SmallRng::seed_from_u64(seed);
    (0..n).map(|_| (0..dim).map(|_| rng.gen::<f32>()).collect()).collect()
}

// ─── Exact k-NN (brute-force ground truth) ───────────────────────────────────

fn exact_knn(corpus: &[Vec<f32>], query: &[f32], k: usize) -> Vec<usize> {
    let mut dists: Vec<(f32, usize)> = corpus
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let d: f32 = v.iter().zip(query).map(|(a, b)| (a - b) * (a - b)).sum::<f32>().sqrt();
            (d, i)
        })
        .collect();
    dists.sort_by(|a, b| a.0.total_cmp(&b.0));
    dists.iter().take(k).map(|(_, i)| *i).collect()
}

// ─── Result types ─────────────────────────────────────────────────────────────

struct EfResult {
    ef:       usize,
    qps:      f64,
    per_q_us: f64,
    recall:   f64,   // 0..1
}

struct LibResult {
    lib:        &'static str,
    n:          usize,
    dim:        usize,
    ins_per_s:  f64,
    per_ins_us: f64,
    efs:        Vec<EfResult>,
}

impl LibResult {
    fn to_json(&self) -> String {
        let get = |ef: usize| self.efs.iter().find(|r| r.ef == ef).unwrap();
        let e50  = get(50);
        let e200 = get(200);
        let e500 = get(500);
        format!(
            r#"{{"n":{n},"dim":{dim},"lib":"{lib}","ins_per_s":{ins:.1},"per_ins_us":{piu:.2},"ef50_qps":{q50:.1},"ef50_per_q_us":{u50:.2},"ef50_recall_pct":{r50:.1},"ef200_qps":{q200:.1},"ef200_per_q_us":{u200:.2},"ef200_recall_pct":{r200:.1},"ef500_qps":{q500:.1},"ef500_per_q_us":{u500:.2},"ef500_recall_pct":{r500:.1}}}"#,
            n   = self.n,
            dim = self.dim,
            lib = self.lib,
            ins = self.ins_per_s,
            piu = self.per_ins_us,
            q50  = e50.qps,  u50  = e50.per_q_us,  r50  = e50.recall  * 100.0,
            q200 = e200.qps, u200 = e200.per_q_us, r200 = e200.recall * 100.0,
            q500 = e500.qps, u500 = e500.per_q_us, r500 = e500.recall * 100.0,
        )
    }

    fn print_header() {
        print!("  {:<12}  {:>10}  {:>9}", "impl", "ins/s", "µs/ins");
        for ef in EFS {
            print!("  {:>8}  {:>8}  {:>9}", format!("ef={ef} qps"), "µs/q", "recall@10");
        }
        println!();
        print!("  {}", "─".repeat(12));
        print!("  {}", "─".repeat(10));
        print!("  {}", "─".repeat(9));
        for _ in EFS {
            print!("  {}  {}  {}", "─".repeat(8), "─".repeat(8), "─".repeat(9));
        }
        println!();
    }

    fn print(&self) {
        print!(
            "  {:<12}  {:>10.0}  {:>9.1}",
            self.lib, self.ins_per_s, self.per_ins_us,
        );
        for ef in &self.efs {
            print!(
                "  {:>8.0}  {:>7.1}µ  {:>8.1}%",
                ef.qps, ef.per_q_us, ef.recall * 100.0,
            );
        }
        println!();
    }
}

// ─── Bench our implementation ────────────────────────────────────────────────

fn bench_ours(
    n:            usize,
    dim:          usize,
    corpus:       &[Vec<f32>],
    queries:      &[Vec<f32>],
    ground_truth: &[Vec<usize>],
) -> LibResult {
    let nq = queries.len();

    let t = Instant::now();
    let mut index: Hnsw<Euclidean> = Builder::new()
        .m(M)
        .ef_construction(EF_CONSTRUCTION)
        .seed(42)
        .build(Euclidean);
    for v in corpus { index.insert(v.clone()); }
    let ins_dur = t.elapsed();

    let efs = EFS.iter().map(|&ef| {
        let t = Instant::now();
        let mut hits = 0usize;
        for (q, gt) in queries.iter().zip(ground_truth.iter()) {
            let gt_set: HashSet<usize> = gt.iter().copied().collect();
            hits += index.search(q, K, ef).iter().filter(|r| gt_set.contains(&r.id)).count();
        }
        let dur = t.elapsed();
        EfResult {
            ef,
            qps:      nq as f64 / dur.as_secs_f64(),
            per_q_us: dur.as_secs_f64() * 1e6 / nq as f64,
            recall:   hits as f64 / (nq * K) as f64,
        }
    }).collect();

    LibResult {
        lib: "ours", n, dim,
        ins_per_s:  n as f64 / ins_dur.as_secs_f64(),
        per_ins_us: ins_dur.as_secs_f64() * 1e6 / n as f64,
        efs,
    }
}

// ─── Bench hnsw_rs ───────────────────────────────────────────────────────────

fn bench_hnsw_rs(
    n:            usize,
    dim:          usize,
    corpus:       &[Vec<f32>],
    queries:      &[Vec<f32>],
    ground_truth: &[Vec<usize>],
) -> LibResult {
    let nq = queries.len();

    let t = Instant::now();
    let index = HnswRs::new(M, n, 16, EF_CONSTRUCTION, DistL2);
    for (i, v) in corpus.iter().enumerate() { index.insert((v.as_slice(), i)); }
    let ins_dur = t.elapsed();

    let efs = EFS.iter().map(|&ef| {
        let t = Instant::now();
        let mut hits = 0usize;
        for (q, gt) in queries.iter().zip(ground_truth.iter()) {
            let gt_set: HashSet<usize> = gt.iter().copied().collect();
            hits += index.search(q.as_slice(), K, ef)
                .iter()
                .filter(|r| gt_set.contains(&r.d_id))
                .count();
        }
        let dur = t.elapsed();
        EfResult {
            ef,
            qps:      nq as f64 / dur.as_secs_f64(),
            per_q_us: dur.as_secs_f64() * 1e6 / nq as f64,
            recall:   hits as f64 / (nq * K) as f64,
        }
    }).collect();

    LibResult {
        lib: "hnsw_rs", n, dim,
        ins_per_s:  n as f64 / ins_dur.as_secs_f64(),
        per_ins_us: ins_dur.as_secs_f64() * 1e6 / n as f64,
        efs,
    }
}

// ─── Bench hnsw v0.11 (rust-cv) ─────────────────────────────────────────────

fn bench_hnsw_v0(
    n:            usize,
    dim:          usize,
    corpus:       &[Vec<f32>],
    queries:      &[Vec<f32>],
    ground_truth: &[Vec<usize>],
) -> LibResult {
    let nq = queries.len();

    // The hnsw v0.11 API requires a `Searcher` for both insert and search.
    // The same searcher instance is reused across calls — its allocations grow
    // monotonically and are never freed, which is the intended usage pattern.
    //
    // M and M0 must be compile-time constants (const generics); we use 16/32
    // to match the other benchmarks.
    let mut searcher: SearcherV0<u32> = SearcherV0::new();

    let t = Instant::now();
    let mut index: HnswV0<EuclideanV0, Vec<f32>, Pcg64, 16, 32> =
        HnswV0::new_params(EuclideanV0, ParamsV0::new().ef_construction(EF_CONSTRUCTION));
    for v in corpus {
        index.insert(v.clone(), &mut searcher);
    }
    let ins_dur = t.elapsed();

    // Pre-allocate the result buffer once; `nearest` fills it in-place.
    let mut dest = vec![Neighbor { index: !0_usize, distance: !0_u32 }; K];

    let efs = EFS.iter().map(|&ef| {
        let t = Instant::now();
        let mut hits = 0usize;
        for (q, gt) in queries.iter().zip(ground_truth.iter()) {
            let res = index.nearest(q, ef, &mut searcher, &mut dest);
            let gt_set: HashSet<usize> = gt.iter().copied().collect();
            hits += res.iter().filter(|nb| gt_set.contains(&nb.index)).count();
        }
        let dur = t.elapsed();
        EfResult {
            ef,
            qps:      nq as f64 / dur.as_secs_f64(),
            per_q_us: dur.as_secs_f64() * 1e6 / nq as f64,
            recall:   hits as f64 / (nq * K) as f64,
        }
    }).collect();

    LibResult {
        lib: "hnsw_v0", n, dim,
        ins_per_s:  n as f64 / ins_dur.as_secs_f64(),
        per_ins_us: ins_dur.as_secs_f64() * 1e6 / n as f64,
        efs,
    }
}

// ─── Speedup / ratio row ─────────────────────────────────────────────────────

fn print_ratio(label: &str, ours: &LibResult, other: &LibResult) {
    let ins_ratio   = other.per_ins_us / ours.per_ins_us;
    let ins_faster  = ours.per_ins_us <= other.per_ins_us;
    print!("  {:<12}  {:>+9}  {:>9}", label, fmt_ratio(ins_ratio, ins_faster), "");
    for (o, t) in ours.efs.iter().zip(other.efs.iter()) {
        let r     = t.per_q_us / o.per_q_us;
        let faster = o.per_q_us <= t.per_q_us;
        let recall_delta = o.recall - t.recall;
        print!(
            "  {:>8}  {:>8}  {:>+8.1}pp",
            fmt_ratio(r, faster), "", recall_delta * 100.0,
        );
    }
    println!();
}

fn fmt_ratio(r: f64, ours_is_faster: bool) -> String {
    let factor = if r >= 1.0 { r } else { 1.0 / r };
    let arrow = if ours_is_faster { "▲" } else { "▼" };
    format!("{arrow}{:.2}×", factor)
}

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    if std::env::var("RUST_LOG").is_err() {
        unsafe { std::env::set_var("RUST_LOG", "error") };
    }

    let full = std::env::args().any(|a| a == "--full");
    let workloads = if full { WORKLOADS_FULL } else { WORKLOADS_DEFAULT };

    println!();
    println!("HNSW comparison benchmark  (3 libraries, {})",
             if full { "full — up to 1M" } else { "default — up to 50k; pass --full for more" });
    println!(
        "Settings: M={M}  M0={M0}  ef_construction={EF_CONSTRUCTION}  K={K}  \
         n_queries={N_QUERIES}  metric=L2(f32)"
    );
    println!("Libraries:");
    println!("  ours     — this repository (pure Rust, M=16)");
    println!("  hnsw_rs  — v0.3.3 by Jean-Pierre Both (Rayon/RwLock, inserts serialized)");
    println!("  hnsw_v0  — v0.11.0 by Geordon Worley (const-generic M, external Searcher)");
    println!("Speedup rows: ▲ = ours faster, ▼ = ours slower.  pp = percentage-point recall delta.");

    // Open output file up-front so every result is flushed to disk as it arrives.
    let out_path = format!("{FIGURES_DIR}/compare.jsonl");
    let mut out: Option<fs::File> = match fs::File::create(&out_path) {
        Ok(f)  => { println!("→ streaming results to {out_path}"); Some(f) }
        Err(e) => { eprintln!("warn: could not create {out_path}: {e}"); None }
    };

    for &(n, dim, label) in workloads {
        println!();
        let sep = "─".repeat(55);
        println!("┌─ {label} {sep}");

        let corpus  = gen_vectors(n,         dim, 1);
        let queries = gen_vectors(N_QUERIES, dim, 2);
        let ground_truth: Vec<Vec<usize>> = queries.iter()
            .map(|q| exact_knn(&corpus, q, K))
            .collect();

        let ours    = bench_ours   (n, dim, &corpus, &queries, &ground_truth);
        let hnsw_rs = bench_hnsw_rs(n, dim, &corpus, &queries, &ground_truth);
        let hnsw_v0 = bench_hnsw_v0(n, dim, &corpus, &queries, &ground_truth);

        LibResult::print_header();
        ours   .print();
        hnsw_rs.print();
        hnsw_v0.print();
        print_ratio("vs hnsw_rs", &ours, &hnsw_rs);
        print_ratio("vs hnsw_v0", &ours, &hnsw_v0);

        // Write the three rows for this workload immediately.
        if let Some(f) = &mut out {
            let _ = writeln!(f, "{}", ours   .to_json());
            let _ = writeln!(f, "{}", hnsw_rs.to_json());
            let _ = writeln!(f, "{}", hnsw_v0.to_json());
        }
    }

    println!();
    println!("Legend");
    println!("  ins/s      – vectors inserted per second (higher = better)");
    println!("  µs/ins     – microseconds per insert (lower = better)");
    println!("  ef=N qps   – search queries per second at beam width N (higher = better)");
    println!("  µs/q       – microseconds per query (lower = better)");
    println!("  recall@10  – fraction of exact top-10 returned (higher = better)");
    println!("  pp         – percentage-point difference in recall (ours − other)");
    println!();
    println!("Notes");
    println!("  • hnsw_rs  uses RwLock + Rayon; inserts serialize on a global write-lock.");
    println!("  • hnsw_v0  owns its items (Vec<f32> cloned on insert); Searcher is reused.");
    println!("  • hnsw_v0  M/M0 are const generics; ef_construction is the only runtime param.");
    println!("  • All benchmarks are single-threaded (sequential insert and search loops).");
    println!("  • Low recall at large n / small ef is expected — ef must grow with index size.");

    if out.is_some() {
        println!("→ done writing {out_path}");
    }
}
