//! Persistence benchmark: measure save / load / mmap-load time and file size
//! for different index types and workload sizes.
//!
//! Run with:
//!   cargo bench --bench persist
//!
//! What is measured per workload (n vectors, dim dimensions):
//!   • File size (bytes)
//!   • Save time — serialize the full index to disk
//!   • Load time (owned) — deserialize, copying vector data into RAM
//!   • Load time (mmap) — map the file; vector section stays page-cache backed,
//!                        only the graph (levels + connections) is deserialized
//!   • Save/load throughput in MB/s
//!
//! Index types benchmarked:
//!   hnsw       Bare Hnsw, no payload
//!   +u32       LabeledIndex<u32>  — 4-byte fixed class label
//!   +String    LabeledIndex<String> — variable-width "item-NNNNN" tag
//!   +Vec<f32>  LabeledIndex<Vec<f32>> — 32-dim secondary embedding
//!   paired     PairedIndex<Euclidean, Euclidean> — two full HNSW graphs
//!
//! Notes
//!   • Indexes are built with M=16, ef_construction=100 (lower than the
//!     search benchmarks — we are benchmarking I/O, not recall quality).
//!   • Each operation is repeated N_REPS times; the MEDIAN is reported.
//!     (Median is more robust than mean against one-time OS jitter.)
//!   • All files are written to a temporary directory that is cleaned up
//!     at the end.
//!   • "mmap load" timing covers: mmap(2) syscall + header read + graph
//!     deserialization.  Vector bytes are NOT read from disk during load;
//!     they are faulted in lazily on first access.  The speedup vs owned
//!     load grows with n × dim (more vector bytes skipped).

use std::fs;
use std::io::{self, Write as IoWrite};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// Absolute path to figures/ baked in at compile time.
const FIGURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/figures");

use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;

use fast_hnsw::{Builder, Hnsw};
use fast_hnsw::distance::Euclidean;
use fast_hnsw::labeled::LabeledIndex;
use fast_hnsw::paired::PairedIndex;
use fast_hnsw::persist;

// ─── Parameters ───────────────────────────────────────────────────────────────

const M:               usize = 16;
const EF_CONSTRUCTION: usize = 100;  // lower ef for faster build (we're benchmarking I/O)

/// Repeats per timing call — median is taken.
const N_REPS_SMALL: usize = 7;   // n ≤ 10k
const N_REPS_LARGE: usize = 3;   // n > 10k

/// Default workloads — up to 50k; fast enough to run routinely.
const WORKLOADS_DEFAULT: &[(usize, usize, &str)] = &[
    ( 1_000,  32,  "n=1k   dim=32 "),
    ( 1_000, 128,  "n=1k   dim=128"),
    (10_000,  32,  "n=10k  dim=32 "),
    (10_000, 128,  "n=10k  dim=128"),
    (50_000, 128,  "n=50k  dim=128"),
];

/// Full workloads — 100k → 1M in 100k steps (pass `--full` to enable).
/// Warning: index build dominates; allow ~1 hr per 100k step at dim=128.
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

// ─── Data generation ──────────────────────────────────────────────────────────

fn gen_vectors(n: usize, dim: usize, seed: u64) -> Vec<Vec<f32>> {
    let mut rng = SmallRng::seed_from_u64(seed);
    (0..n).map(|_| (0..dim).map(|_| rng.gen::<f32>()).collect()).collect()
}

fn label_string(id: usize) -> String {
    format!("item-{id:05}")  // always 10 chars → 4 + 10 = 14 bytes encoded
}

// ─── Index builders ───────────────────────────────────────────────────────────

fn build_hnsw(corpus: &[Vec<f32>]) -> Hnsw<Euclidean> {
    let mut idx = Builder::new()
        .m(M).ef_construction(EF_CONSTRUCTION).seed(1)
        .build(Euclidean);
    for v in corpus { idx.insert(v.clone()); }
    idx
}

fn build_labeled_u32(corpus: &[Vec<f32>]) -> LabeledIndex<Euclidean, u32> {
    let mut idx = Builder::new()
        .m(M).ef_construction(EF_CONSTRUCTION).seed(1)
        .build_labeled(Euclidean);
    for (i, v) in corpus.iter().enumerate() { idx.insert(v.clone(), i as u32); }
    idx
}

fn build_labeled_string(corpus: &[Vec<f32>]) -> LabeledIndex<Euclidean, String> {
    let mut idx = Builder::new()
        .m(M).ef_construction(EF_CONSTRUCTION).seed(1)
        .build_labeled(Euclidean);
    for (i, v) in corpus.iter().enumerate() {
        idx.insert(v.clone(), label_string(i));
    }
    idx
}

fn build_labeled_vec_f32(corpus: &[Vec<f32>], sec_dim: usize) -> LabeledIndex<Euclidean, Vec<f32>> {
    let mut rng = SmallRng::seed_from_u64(99);
    let mut idx = Builder::new()
        .m(M).ef_construction(EF_CONSTRUCTION).seed(1)
        .build_labeled(Euclidean);
    for v in corpus {
        let sec: Vec<f32> = (0..sec_dim).map(|_| rng.gen::<f32>()).collect();
        idx.insert(v.clone(), sec);
    }
    idx
}

fn build_paired(corpus_a: &[Vec<f32>], corpus_b: &[Vec<f32>]) -> PairedIndex<Euclidean, Euclidean> {
    let mut idx = Builder::new()
        .m(M).ef_construction(EF_CONSTRUCTION).seed(1)
        .build_paired(Euclidean, Euclidean);
    for (a, b) in corpus_a.iter().zip(corpus_b.iter()) {
        idx.insert(a.clone(), b.clone());
    }
    idx
}

// ─── Timing helpers ───────────────────────────────────────────────────────────

fn median_duration(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples[samples.len() / 2]
}

fn time_save<F: Fn() -> io::Result<()>>(n_reps: usize, f: F) -> Duration {
    let samples: Vec<Duration> = (0..n_reps).map(|_| {
        let t = Instant::now();
        f().expect("save failed");
        t.elapsed()
    }).collect();
    median_duration(samples)
}

fn time_load<F: Fn() -> io::Result<()>>(n_reps: usize, f: F) -> Duration {
    let samples: Vec<Duration> = (0..n_reps).map(|_| {
        let t = Instant::now();
        f().expect("load failed");
        t.elapsed()
    }).collect();
    median_duration(samples)
}

// ─── File size ────────────────────────────────────────────────────────────────

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).unwrap().len()
}

fn file_size_pair(base: &Path) -> u64 {
    let mut s = base.as_os_str().to_owned();
    s.push("_a.hnsw");
    let a = std::fs::metadata(PathBuf::from(&s)).unwrap().len();
    let mut s = base.as_os_str().to_owned();
    s.push("_b.hnsw");
    let b = std::fs::metadata(PathBuf::from(s)).unwrap().len();
    a + b
}

// ─── Formatting ───────────────────────────────────────────────────────────────

fn fmt_bytes(b: u64) -> String {
    if b < 1024 { format!("{b} B") }
    else if b < 1024 * 1024 { format!("{:.1} KiB", b as f64 / 1024.0) }
    else { format!("{:.2} MiB", b as f64 / (1024.0 * 1024.0)) }
}

fn fmt_dur(d: Duration) -> String {
    let us = d.as_secs_f64() * 1e6;
    if us < 1_000.0       { format!("{us:.0} µs") }
    else if us < 1_000_000.0 { format!("{:.1} ms", us / 1_000.0) }
    else                  { format!("{:.2} s",  us / 1_000_000.0) }
}

fn mb_per_s(bytes: u64, dur: Duration) -> f64 {
    bytes as f64 / (1024.0 * 1024.0) / dur.as_secs_f64()
}

fn speedup(owned: Duration, mmap: Duration) -> f64 {
    owned.as_secs_f64() / mmap.as_secs_f64()
}

// ─── Per-workload results ─────────────────────────────────────────────────────

struct Row {
    label:      &'static str,
    file_bytes: u64,
    t_save:     Duration,
    t_load:     Duration,
    t_mmap:     Duration,
}

impl Row {
    fn print_header() {
        println!(
            "  {:<12}  {:>9}  {:>10}  {:>8}  {:>10}  {:>8}  {:>10}  {:>7}",
            "type", "file size",
            "save",   "save MB/s",
            "load",   "load MB/s",
            "mmap",   "mmap ×",
        );
        let sep = format!("  {}", "─".repeat(12));
        let col = |w: usize| format!("  {}", "─".repeat(w));
        println!("{sep}{}{}{}{}{}{}{}", col(9), col(10), col(8), col(10), col(8), col(10), col(7));
    }

    fn print(&self) {
        let mbs_save  = mb_per_s(self.file_bytes, self.t_save);
        let mbs_load  = mb_per_s(self.file_bytes, self.t_load);
        let spdup     = speedup(self.t_load, self.t_mmap);
        println!(
            "  {:<12}  {:>9}  {:>10}  {:>8.1}  {:>10}  {:>8.1}  {:>10}  {:>6.1}×",
            self.label,
            fmt_bytes(self.file_bytes),
            fmt_dur(self.t_save),   mbs_save,
            fmt_dur(self.t_load),   mbs_load,
            fmt_dur(self.t_mmap),   spdup,
        );
    }
}

// ─── Single workload ──────────────────────────────────────────────────────────

fn run_workload(n: usize, dim: usize, tmp: &Path) -> Vec<Row> {
    let n_reps = if n <= 10_000 { N_REPS_SMALL } else { N_REPS_LARGE };
    let sec_dim = 32_usize;   // dimension of the Vec<f32> secondary embedding

    let corpus_a = gen_vectors(n, dim, 1);
    let corpus_b = gen_vectors(n, dim, 2); // B-side for PairedIndex

    // ── Pre-build all indexes ─────────────────────────────────────────────────
    // (build time is excluded from I/O timings — we're measuring persistence)
    print!("    building indexes… "); std::io::Write::flush(&mut std::io::stdout()).ok();
    let idx_hnsw      = build_hnsw(&corpus_a);
    let idx_u32       = build_labeled_u32(&corpus_a);
    let idx_str       = build_labeled_string(&corpus_a);
    let idx_vf32      = build_labeled_vec_f32(&corpus_a, sec_dim);
    let idx_paired    = build_paired(&corpus_a, &corpus_b);
    println!("done");

    let mut rows = Vec::new();

    // ── 1. Bare Hnsw ──────────────────────────────────────────────────────────
    {
        let path = tmp.join("hnsw.hnsw");
        let t_save = time_save(n_reps, || persist::save(&idx_hnsw, &path));
        let sz     = file_size(&path);
        let t_load = time_load(n_reps, || { persist::load(&path, Euclidean)?; Ok(()) });
        let t_mmap = time_load(n_reps, || { persist::load_mmap(&path, Euclidean)?; Ok(()) });
        rows.push(Row { label: "hnsw", file_bytes: sz, t_save, t_load, t_mmap });
    }

    // ── 2. LabeledIndex<u32> ─────────────────────────────────────────────────
    {
        let path = tmp.join("u32.hnsw");
        let t_save = time_save(n_reps, || idx_u32.save(&path));
        let sz     = file_size(&path);
        let t_load = time_load(n_reps, || {
            LabeledIndex::<Euclidean, u32>::load(&path, Euclidean)?; Ok(())
        });
        let t_mmap = time_load(n_reps, || {
            LabeledIndex::<Euclidean, u32>::load_mmap(&path, Euclidean)?; Ok(())
        });
        rows.push(Row { label: "+u32", file_bytes: sz, t_save, t_load, t_mmap });
    }

    // ── 3. LabeledIndex<String> ───────────────────────────────────────────────
    {
        let path = tmp.join("str.hnsw");
        let t_save = time_save(n_reps, || idx_str.save(&path));
        let sz     = file_size(&path);
        let t_load = time_load(n_reps, || {
            LabeledIndex::<Euclidean, String>::load(&path, Euclidean)?; Ok(())
        });
        let t_mmap = time_load(n_reps, || {
            LabeledIndex::<Euclidean, String>::load_mmap(&path, Euclidean)?; Ok(())
        });
        rows.push(Row { label: "+String", file_bytes: sz, t_save, t_load, t_mmap });
    }

    // ── 4. LabeledIndex<Vec<f32>> (32-dim secondary) ──────────────────────────
    {
        let path = tmp.join("vecf32.hnsw");
        let t_save = time_save(n_reps, || idx_vf32.save(&path));
        let sz     = file_size(&path);
        let t_load = time_load(n_reps, || {
            LabeledIndex::<Euclidean, Vec<f32>>::load(&path, Euclidean)?; Ok(())
        });
        let t_mmap = time_load(n_reps, || {
            LabeledIndex::<Euclidean, Vec<f32>>::load_mmap(&path, Euclidean)?; Ok(())
        });
        rows.push(Row { label: "+Vec<f32>", file_bytes: sz, t_save, t_load, t_mmap });
    }

    // ── 5. PairedIndex ────────────────────────────────────────────────────────
    {
        let base   = tmp.join("paired");
        let t_save = time_save(n_reps, || idx_paired.save(&base));
        let sz     = file_size_pair(&base);
        let t_load = time_load(n_reps, || {
            PairedIndex::<Euclidean, Euclidean>::load(&base, Euclidean, Euclidean)?; Ok(())
        });
        let t_mmap = time_load(n_reps, || {
            PairedIndex::<Euclidean, Euclidean>::load_mmap(&base, Euclidean, Euclidean)?; Ok(())
        });
        rows.push(Row { label: "paired", file_bytes: sz, t_save, t_load, t_mmap });
    }

    rows
}

// ─── Machine-readable output (for the plotter) ───────────────────────────────

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let full = std::env::args().any(|a| a == "--full");
    let workloads = if full { WORKLOADS_FULL } else { WORKLOADS_DEFAULT };

    let tmp = std::env::temp_dir().join("hnsw_persist_bench");
    std::fs::create_dir_all(&tmp).unwrap();

    println!();
    println!("HNSW Persistence Benchmark  ({})",
             if full { "full — up to 1M" } else { "default — up to 50k; pass --full for more" });
    println!("  M={M}  ef_construction={EF_CONSTRUCTION}  metric=L2(f32)");
    println!("  secondary-embedding dim (Vec<f32> payload): 32");
    println!("  timings: median of {N_REPS_SMALL}/{N_REPS_LARGE} reps (small/large n)");
    println!();
    println!("Columns");
    println!("  file size  — total bytes written (both files for paired)");
    println!("  save       — wall-clock time to serialize to disk");
    println!("  save MB/s  — file_size / save_time");
    println!("  load       — wall-clock time to read + copy all bytes into RAM");
    println!("  load MB/s  — file_size / load_time");
    println!("  mmap       — wall-clock time to mmap the file + deserialize graph");
    println!("               (vector bytes NOT read — faulted in lazily)");
    println!("  mmap ×     — load_time / mmap_time  (speedup from memory-mapping)");

    // Open output file up-front and write header; rows are flushed as they arrive.
    let out_path = format!("{FIGURES_DIR}/persist.csv");
    let mut out: Option<fs::File> = match fs::File::create(&out_path) {
        Err(e) => { eprintln!("warn: could not create {out_path}: {e}"); None }
        Ok(mut f) => {
            let _ = writeln!(f, "workload,type,file_bytes,save_us,load_us,mmap_us");
            println!("→ streaming results to {out_path}");
            Some(f)
        }
    };

    // Helper: write one Row to the CSV file immediately.
    let mut flush_row = |wl: &str, r: &Row| {
        if let Some(f) = &mut out {
            let _ = writeln!(
                f,
                "{},{},{},{},{},{}",
                wl,
                r.label,
                r.file_bytes,
                (r.t_save.as_secs_f64() * 1e6) as u64,
                (r.t_load.as_secs_f64() * 1e6) as u64,
                (r.t_mmap.as_secs_f64() * 1e6) as u64,
            );
        }
    };

    for &(n, dim, label) in workloads {
        let sep = "─".repeat(48);
        println!();
        println!("┌─ {label} {sep}");

        let tmp_w = tmp.join(format!("w_{n}_{dim}"));
        std::fs::create_dir_all(&tmp_w).unwrap();

        let rows = run_workload(n, dim, &tmp_w);

        Row::print_header();
        let wl = format!("{n}×{dim}");
        for r in &rows {
            r.print();
            flush_row(&wl, r);
        }
    }

    if out.is_some() {
        println!("\n→ done writing {out_path}");
    }

    // ── Cleanup ───────────────────────────────────────────────────────────────
    let _ = std::fs::remove_dir_all(&tmp);
}
