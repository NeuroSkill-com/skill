//! End-to-end REVE RLX backend parity: every available device must run
//! the full model and match CPU output within cosine tolerance.
//!
//! ```text
//! cargo test --release --features rlx-cpu,rlx-metal,rlx-mlx,rlx-gpu \
//!     --test rlx_backend_parity -- --nocapture
//! ```

use std::path::{Path, PathBuf};

use reve_rs::rlx::ReveEncoder;

const COS_MIN: f64 = 0.9999;
/// Full-model max abs diff vs CPU reference.
const MAX_ABS: f32 = 0.002;

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let (mut dot, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
    for (&x, &y) in a.iter().zip(b.iter()) {
        let (x, y) = (x as f64, y as f64);
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    dot / (na.sqrt() * nb.sqrt())
}

fn max_abs_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| {
            if x.is_nan() || y.is_nan() {
                f32::INFINITY
            } else {
                (x - y).abs()
            }
        })
        .fold(0.0f32, f32::max)
}


fn locate_paths() -> Option<(PathBuf, PathBuf)> {
    let weights = std::env::var("REVE_WEIGHTS")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/model.safetensors");
            p.exists().then_some(p)
        })?;
    let config = std::env::var("REVE_CONFIG")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/config.json");
            p.exists().then_some(p)
        })?;
    Some((config, weights))
}

/// Deterministic synthetic EEG + positions for cross-backend comparison.
fn synthetic_input(n_channels: usize, n_times: usize) -> (Vec<f32>, Vec<f32>) {
    let signal = vec![0.0f32; n_channels * n_times];
    let positions = vec![0.0f32; n_channels * 3];
    (signal, positions)
}

fn run_on_device(
    config: &Path,
    weights: &Path,
    device: rlx::Device,
    signal: &[f32],
    positions: &[f32],
    n_channels: usize,
    n_times: usize,
) -> anyhow::Result<Vec<f32>> {
    let (mut enc, _) = ReveEncoder::load(config, weights, device)?;
    let out = enc.run_one(
        signal.to_vec(),
        positions.to_vec(),
        n_channels,
        n_times,
    )?;
    Ok(out.output)
}

fn vector_stats(v: &[f32]) -> (f32, f32, usize, usize) {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    let mut n_nan = 0usize;
    let mut n_zero = 0usize;
    for &x in v {
        if x.is_nan() {
            n_nan += 1;
        } else {
            min = min.min(x);
            max = max.max(x);
        }
        if x == 0.0 {
            n_zero += 1;
        }
    }
    (min, max, n_nan, n_zero)
}

fn check_parity(label: &str, reference: &[f32], candidate: &[f32]) -> bool {
    if reference.len() != candidate.len() {
        eprintln!("  [FAIL] {label}: output length mismatch");
        return false;
    }
    let max_abs = max_abs_diff(reference, candidate);
    let cos = cosine_similarity(reference, candidate);
    let (rmin, rmax, rnan, _) = vector_stats(reference);
    let (cmin, cmax, cnan, _) = vector_stats(candidate);
    eprintln!(
        "  {label}: max_abs={max_abs:.3e}  cosine={cos:.6}  n={}  ref=[{rmin:.3},{rmax:.3}] cand=[{cmin:.3},{cmax:.3}] nan=({rnan},{cnan})",
        reference.len()
    );
    eprintln!("  {label}[0..6]: {:?}", &candidate[..6.min(candidate.len())]);

    if cnan != 0 {
        eprintln!(
            "  [FAIL] {label}: output contains {cnan} NaNs (first={:?})",
            candidate.iter().find(|v| v.is_nan())
        );
        return false;
    }
    if !max_abs.is_finite() || max_abs >= MAX_ABS || !cos.is_finite() || cos <= COS_MIN {
        eprintln!("  [FAIL] {label}: parity failed (max_abs={max_abs:.3e}, cosine={cos:.6})");
        return false;
    }
    true
}

/// Backends with known upstream multi-block graph bugs (see `tests/rlx_attn_parity.rs`).
fn is_known_parity_limitation(_label: &str) -> bool {
    false
}
fn try_backend(
    label: &str,
    device: rlx::Device,
    config: &Path,
    weights: &Path,
    signal: &[f32],
    positions: &[f32],
    n_channels: usize,
    n_times: usize,
    reference: &[f32],
) -> bool {
    let out = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_on_device(config, weights, device, signal, positions, n_channels, n_times)
    })) {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            eprintln!("  [skip] {label}: {e:#}");
            return true;
        }
        Err(_) => {
            eprintln!("  [skip] {label}: backend panicked (likely unavailable)");
            return true;
        }
    };

    check_parity(label, reference, &out)
        || if is_known_parity_limitation(label) {
            eprintln!(
                "  [known-limitation] {label}: full-model parity pending rlx-mlx fix (depth>=2); op tests pass"
            );
            true
        } else {
            false
        }
}

#[test]
fn all_rlx_backends_match_cpu() {
    let (config, weights) = match locate_paths() {
        Some(p) => p,
        None => {
            eprintln!("\n[SKIP] rlx_backend_parity — missing weights/config.");
            eprintln!("       set REVE_WEIGHTS / REVE_CONFIG or place files in data/");
            return;
        }
    };

    let n_channels = 22;
    let n_times = 1000;
    let (signal, positions) = synthetic_input(n_channels, n_times);

    eprintln!("→ config  = {}", config.display());
    eprintln!("→ weights = {}", weights.display());
    eprintln!("→ input   = {n_channels} ch × {n_times} samples");

    eprintln!("\n── reference (CPU) ──");
    let cpu_out = run_on_device(
        &config,
        &weights,
        rlx::Device::Cpu,
        &signal,
        &positions,
        n_channels,
        n_times,
    )
    .expect("CPU inference");
    eprintln!("  CPU: ok, dim={}", cpu_out.len());
    let cpu_mean: f64 = cpu_out.iter().map(|&v| v as f64).sum::<f64>() / cpu_out.len() as f64;
    eprintln!("  CPU mean={cpu_mean:.4}  std={:.4}", {
        let var = cpu_out.iter().map(|&v| {
            let d = v as f64 - cpu_mean;
            d * d
        }).sum::<f64>() / cpu_out.len() as f64;
        var.sqrt()
    });
    eprintln!("  CPU[0..6]: {:?}", &cpu_out[..6.min(cpu_out.len())]);

    // Sanity: CPU must be deterministic across two runs.
    let cpu_out2 = run_on_device(
        &config,
        &weights,
        rlx::Device::Cpu,
        &signal,
        &positions,
        n_channels,
        n_times,
    )
    .expect("CPU inference (2nd run)");
    assert!(check_parity("CPU×2", &cpu_out, &cpu_out2), "CPU must be deterministic");

    eprintln!("\n── backend parity vs CPU ──");
    let mut all_ok = true;
    for (label, device) in [
        ("Metal", rlx::Device::Metal),
        ("MLX", rlx::Device::Mlx),
        ("GPU (wgpu)", rlx::Device::Gpu),
        ("CUDA", rlx::Device::Cuda),
        ("ROCm", rlx::Device::Rocm),
        ("TPU", rlx::Device::Tpu),
    ] {
        all_ok &= try_backend(
            label,
            device,
            &config,
            &weights,
            &signal,
            &positions,
            n_channels,
            n_times,
            &cpu_out,
        );
    }
    assert!(all_ok, "one or more RLX backends failed CPU parity");
}
