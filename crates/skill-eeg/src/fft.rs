// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Selectable FFT backend for the live DSP (filter, band analysis).
//!
//! | name | runs on |
//! |------|---------|
//! | `auto` (default) | `accelerate` on macOS, `cpu` elsewhere — and a GPU for batches big enough that it measurably wins (see below) |
//! | `cpu` | rustfft |
//! | `accelerate` | Apple vDSP (macOS) |
//! | any rlx device: `metal`, `ane` / `coreml`, `cuda`, `rocm`, `xdna`, `gpu`, `vulkan`, `rlx-cpu`, … | rlx, if that backend is compiled in and present |
//!
//! Not MLX: checking it can crash the process (see `MLX_REFUSED`).
//!
//! The default stays on the CPU on purpose. Measured on an M-series Mac for the
//! live batch (32 ch × 1024, forward + inverse; `tests/fft_backends.rs`):
//! Accelerate ~70 µs, rustfft ~120 µs, rlx-cpu ~350 µs, ANE (CoreML) ~0.6–0.8 ms,
//! Metal 0.4–3 ms and MLX 0.7–7 ms depending on load, plus 4 ms–140 ms for the
//! first call. A GPU FFT also waits behind any other GPU work in the process —
//! a TTS warm-up stalled new sessions for seconds. The others are there to
//! measure and for hardware where they win.
//!
//! **`auto` picks per call by batch size** (channels × length). On selecting it,
//! a background thread times every usable GPU backend against the CPU one over
//! [`SWEEP`] ([`calibrate`]); batches of at least the crossover size — the
//! smallest size from which the GPU is [`MARGIN`]× faster at that size and every
//! larger one — then run on that GPU, everything else on the CPU. Until that
//! finishes, or when no GPU wins (the M-series case above: Accelerate was faster
//! at every size up to 512 × 4096), everything runs on the CPU. Calibration runs
//! once per process and off the EEG path, so it cannot stall a session.
//!
//! **A backend is only used if it gives the right answer.** [`set_backend`]
//! runs a parity probe against rustfft first and refuses a backend that is
//! missing, panics or disagrees, keeping the current one. If a chosen rlx
//! backend later panics (e.g. compiling a new shape), the call is redone on CPU
//! and CPU stays selected — an FFT backend never takes a session down.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::RwLock;

/// Where FFTs run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FftBackend {
    Cpu,
    #[cfg(target_os = "macos")]
    Accelerate,
    #[cfg(feature = "rlx-fft")]
    Rlx(rlx::Device),
}

impl FftBackend {
    /// Setting value for this backend (round-trips through [`parse`]).
    pub fn name(self) -> String {
        match self {
            FftBackend::Cpu => "cpu".into(),
            #[cfg(target_os = "macos")]
            FftBackend::Accelerate => "accelerate".into(),
            #[cfg(feature = "rlx-fft")]
            FftBackend::Rlx(rlx::Device::Cpu) => "rlx-cpu".into(),
            #[cfg(feature = "rlx-fft")]
            FftBackend::Rlx(d) => d.as_arg().into(),
        }
    }
}

static BACKEND: RwLock<FftBackend> = RwLock::new(FftBackend::Cpu);

/// The backend FFTs currently run on.
pub fn backend() -> FftBackend {
    *BACKEND.read().unwrap_or_else(|e| e.into_inner())
}

fn store(b: FftBackend) {
    *BACKEND.write().unwrap_or_else(|e| e.into_inner()) = b;
}

/// What `auto` learned here: batches of at least `min_samples` samples
/// (channels × length) run on `gpu`, smaller ones on [`backend`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoPlan {
    pub gpu: FftBackend,
    pub min_samples: usize,
}

static AUTO: AtomicBool = AtomicBool::new(false);
static PLAN: RwLock<Option<AutoPlan>> = RwLock::new(None);
/// 0 = not started, 1 = running, 2 = done.
static CALIBRATION: AtomicU8 = AtomicU8::new(0);

/// Whether `auto` (size-based choice) is selected.
pub fn is_auto() -> bool {
    AUTO.load(Ordering::Relaxed)
}

/// The GPU `auto` sends large batches to, if calibration found one that wins.
pub fn auto_plan() -> Option<AutoPlan> {
    *PLAN.read().unwrap_or_else(|e| e.into_inner())
}

/// Whether `auto`'s calibration is still running.
pub fn calibrating() -> bool {
    CALIBRATION.load(Ordering::Relaxed) == 1
}

fn store_plan(p: Option<AutoPlan>) {
    *PLAN.write().unwrap_or_else(|e| e.into_inner()) = p;
}

/// Backend for a batch of `samples` samples: the plan's GPU when `auto` is on
/// and the batch reaches its crossover, else `base`.
fn pick_for(samples: usize, auto: bool, plan: Option<AutoPlan>, base: FftBackend) -> FftBackend {
    match plan {
        Some(p) if auto && samples >= p.min_samples => p.gpu,
        _ => base,
    }
}

/// Parse a setting value. Does not check that the backend is usable.
pub fn parse(name: &str) -> Result<FftBackend, String> {
    let n = name.trim().to_ascii_lowercase();
    match n.as_str() {
        "" | "auto" | "default" => {
            #[cfg(target_os = "macos")]
            return Ok(FftBackend::Accelerate);
            #[cfg(not(target_os = "macos"))]
            return Ok(FftBackend::Cpu);
        }
        "cpu" | "rustfft" => return Ok(FftBackend::Cpu),
        "accelerate" | "vdsp" => {
            #[cfg(target_os = "macos")]
            return Ok(FftBackend::Accelerate);
            #[cfg(not(target_os = "macos"))]
            return Err("Accelerate (vDSP) is only available on macOS".into());
        }
        "blas" => {
            return Err("BLAS has no FFT routine; use `accelerate` (Apple's BLAS/vDSP library) or `rlx-cpu`".into());
        }
        _ => {}
    }
    #[cfg(feature = "rlx-fft")]
    {
        let dev = if n == "rlx-cpu" {
            "cpu"
        } else {
            n.strip_prefix("rlx:").unwrap_or(&n)
        };
        if let Ok(d) = dev.parse::<rlx::Device>() {
            if d == rlx::Device::Mlx {
                return Err(MLX_REFUSED.into());
            }
            return Ok(FftBackend::Rlx(d));
        }
    }
    Err(format!(
        "unknown FFT backend `{name}` (try one of: {})",
        known().join(", ")
    ))
}

/// MLX is never offered for EEG FFTs. Checking whether it can run means
/// running an MLX kernel in this process, and when the app bundle lacks MLX's
/// Metal library MLX traps in C++ — uncatchable, the daemon dies (seen in
/// 0.1.0-rc.8: opening Settings → EXG crashed it). It was also the slowest
/// backend measured (0.7–7 ms vs ~0.07 ms for Accelerate).
#[cfg(feature = "rlx-fft")]
const MLX_REFUSED: &str =
    "MLX is not offered for EEG FFTs: checking it can crash the app when MLX's Metal library is missing, and it is the slowest backend here";

/// rlx devices that may be offered, in display order. Never MLX (see
/// [`MLX_REFUSED`]), and never `rlx::available_devices()`, which probes MLX.
#[cfg(feature = "rlx-fft")]
const RLX_CANDIDATES: [rlx::Device; 8] = [
    rlx::Device::Cpu,
    rlx::Device::Metal,
    rlx::Device::Ane,
    rlx::Device::Cuda,
    rlx::Device::Rocm,
    rlx::Device::Xdna,
    rlx::Device::Gpu,
    rlx::Device::Vulkan,
];

/// Whether rlx can run `d` here. Safe to call: each check is per device, and
/// MLX — the one whose check runs a kernel — is excluded.
#[cfg(feature = "rlx-fft")]
fn rlx_usable(d: rlx::Device) -> bool {
    d != rlx::Device::Mlx && rlx::is_available(d)
}

/// Backends this build can run here right now (not yet parity-checked).
pub fn known() -> Vec<String> {
    #[allow(unused_mut)]
    let mut v = vec![FftBackend::Cpu.name()];
    #[cfg(target_os = "macos")]
    v.push(FftBackend::Accelerate.name());
    #[cfg(feature = "rlx-fft")]
    for d in RLX_CANDIDATES.into_iter().filter(|d| rlx_usable(*d)) {
        v.push(FftBackend::Rlx(d).name());
    }
    v
}

/// Select a backend by setting value. On success returns the backend now in
/// use; on failure the current backend is kept and the reason is returned.
pub fn set_backend(name: &str) -> Result<FftBackend, String> {
    let b = parse(name)?;
    if b != backend() {
        probe(b)?;
        store(b);
    }
    let auto = matches!(name.trim().to_ascii_lowercase().as_str(), "" | "auto" | "default");
    AUTO.store(auto, Ordering::Relaxed);
    if auto
        && CALIBRATION
            .compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        let spawned = std::thread::Builder::new().name("fft-calibrate".into()).spawn(|| {
            let plan = calibrate(backend());
            store_plan(plan);
            CALIBRATION.store(2, Ordering::Relaxed);
        });
        if spawned.is_err() {
            CALIBRATION.store(2, Ordering::Relaxed);
        }
    }
    Ok(b)
}

/// Batch sizes `auto` times (channels, length): the live batch up to the
/// largest caps × windows the DSP could see.
pub const SWEEP: [(usize, usize); 7] = [
    (32, 256),
    (32, 1024),
    (128, 1024),
    (256, 1024),
    (256, 4096),
    (1024, 1024),
    (512, 4096),
];

/// How much faster a GPU must be before `auto` uses it: idle-machine timings
/// flatter the GPU, which in the app also waits behind LLM / TTS work.
pub const MARGIN: f64 = 1.25;

/// Smallest size from which `gpu` beats `cpu` by [`MARGIN`] at that size and
/// every larger one. Times are per size, in `sizes` order (ascending); a
/// missing GPU time (it failed) disqualifies that size and all below it.
#[cfg_attr(not(feature = "rlx-fft"), allow(dead_code))] // only `calibrate` with rlx
fn crossover(sizes: &[usize], cpu: &[f64], gpu: &[Option<f64>]) -> Option<usize> {
    let mut at = None;
    for i in (0..sizes.len()).rev() {
        match gpu[i] {
            Some(g) if g * MARGIN <= cpu[i] => at = Some(sizes[i]),
            _ => break,
        }
    }
    at
}

/// Median seconds of a forward + inverse round trip on `b` for `ch` × `len`,
/// after one untimed call (which compiles GPU graphs for the shape).
#[cfg_attr(not(feature = "rlx-fft"), allow(dead_code))] // only `calibrate` with rlx
fn time_round_trip(b: FftBackend, ch: usize, len: usize) -> Option<f64> {
    let sig: Vec<Vec<f32>> = (0..ch)
        .map(|c| (0..len).map(|i| ((i + c) as f32 * 0.3).sin()).collect())
        .collect();
    std::panic::catch_unwind(|| {
        let _ = ifft_on(b, &fft_on(b, &sig));
        let mut t: Vec<f64> = (0..5)
            .map(|_| {
                let start = std::time::Instant::now();
                std::hint::black_box(ifft_on(b, &fft_on(b, &sig)));
                start.elapsed().as_secs_f64()
            })
            .collect();
        t.sort_by(f64::total_cmp);
        t[2]
    })
    .ok()
}

/// Time every usable, parity-checked GPU backend against `cpu` over [`SWEEP`]
/// and return the one with the smallest crossover, if any wins.
pub fn calibrate(cpu: FftBackend) -> Option<AutoPlan> {
    #[cfg(feature = "rlx-fft")]
    {
        let sizes: Vec<usize> = SWEEP.iter().map(|(c, l)| c * l).collect();
        let cpu_t: Vec<f64> = SWEEP
            .iter()
            .map(|&(c, l)| time_round_trip(cpu, c, l).unwrap_or(f64::INFINITY))
            .collect();
        let mut best: Option<AutoPlan> = None;
        for d in RLX_CANDIDATES
            .into_iter()
            .filter(|d| *d != rlx::Device::Cpu && rlx_usable(*d))
        {
            let gpu = FftBackend::Rlx(d);
            if let Err(e) = probe(gpu) {
                eprintln!("[fft] auto: skipping `{}`: {e}", gpu.name());
                continue;
            }
            let gpu_t: Vec<Option<f64>> = SWEEP.iter().map(|&(c, l)| time_round_trip(gpu, c, l)).collect();
            let at = crossover(&sizes, &cpu_t, &gpu_t);
            let ms = |t: Option<f64>| t.map_or("failed".to_string(), |t| format!("{:.2}", t * 1e3));
            eprintln!(
                "[fft] auto: `{}` vs `{}` ms per size {:?}: {} → {}",
                gpu.name(),
                cpu.name(),
                sizes,
                gpu_t
                    .iter()
                    .zip(&cpu_t)
                    .map(|(g, c)| format!("{}/{:.2}", ms(*g), c * 1e3))
                    .collect::<Vec<_>>()
                    .join(" "),
                at.map_or("never wins".to_string(), |n| format!("GPU from {n} samples")),
            );
            if let Some(n) = at {
                if best.is_none_or(|b| n < b.min_samples) {
                    best = Some(AutoPlan { gpu, min_samples: n });
                }
            }
        }
        best
    }
    #[cfg(not(feature = "rlx-fft"))]
    {
        let _ = cpu;
        None
    }
}

/// Run one FFT round trip on `b` and compare it with rustfft.
pub fn probe(b: FftBackend) -> Result<(), String> {
    if b == FftBackend::Cpu {
        return Ok(());
    }
    #[cfg(feature = "rlx-fft")]
    if let FftBackend::Rlx(d) = b {
        if !rlx_usable(d) {
            return Err(format!("rlx device `{}` is not compiled in or not present", d.as_arg()));
        }
    }
    let sig: Vec<Vec<f32>> = (0..4)
        .map(|c| {
            (0..250)
                .map(|i| ((i * (c + 2)) as f32 * 0.09).sin() * 30.0 + c as f32)
                .collect()
        })
        .collect();
    let want = crate::cpu_fft::fft_batch(&sig);
    let want_back = crate::cpu_fft::ifft_batch(&want);
    let got = std::panic::catch_unwind(|| {
        let f = fft_on(b, &sig);
        let back = ifft_on(b, &f);
        (f, back)
    })
    .map_err(|_| format!("`{}` panicked running an FFT", b.name()))?;
    let scale = want[0].0.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    let close = |x: f32, y: f32, tol: f32| (x - y).abs() <= tol;
    let spectrum_ok = got.0.len() == want.len()
        && got.0.iter().zip(&want).all(|((gr, gi), (wr, wi))| {
            gr.len() == wr.len()
                && gr.iter().zip(wr).all(|(&x, &y)| close(x, y, 1e-3 * scale))
                && gi.iter().zip(wi).all(|(&x, &y)| close(x, y, 1e-3 * scale))
        });
    let inverse_ok = got
        .1
        .iter()
        .zip(&want_back)
        .all(|(g, w)| g.len() == w.len() && g.iter().zip(w).all(|(&x, &y)| close(x, y, 1e-2)));
    if spectrum_ok && inverse_ok {
        Ok(())
    } else {
        Err(format!("`{}` gave a different FFT result from rustfft", b.name()))
    }
}

fn fft_on(b: FftBackend, signals: &[Vec<f32>]) -> Vec<(Vec<f32>, Vec<f32>)> {
    match b {
        FftBackend::Cpu => crate::cpu_fft::fft_batch(signals),
        #[cfg(target_os = "macos")]
        FftBackend::Accelerate => crate::accel_fft::fft_batch(signals),
        #[cfg(feature = "rlx-fft")]
        FftBackend::Rlx(d) => crate::rlx_fft::fft_batch_on(d, signals),
    }
}

fn ifft_on(b: FftBackend, spectra: &[(Vec<f32>, Vec<f32>)]) -> Vec<Vec<f32>> {
    match b {
        FftBackend::Cpu => crate::cpu_fft::ifft_batch(spectra),
        #[cfg(target_os = "macos")]
        FftBackend::Accelerate => crate::accel_fft::ifft_batch(spectra),
        #[cfg(feature = "rlx-fft")]
        FftBackend::Rlx(d) => crate::rlx_fft::ifft_batch_on(d, spectra),
    }
}

/// Run `f` on the backend for a batch of `samples`; if it panics, redo it on
/// CPU. A failing `auto` GPU is dropped from the plan (smaller batches keep
/// their backend); a failing selected backend is replaced by CPU.
fn guarded<T>(samples: usize, f: impl Fn(FftBackend) -> T) -> T {
    let base = backend();
    let b = pick_for(samples, is_auto(), auto_plan(), base);
    if b == FftBackend::Cpu {
        return f(b);
    }
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(b))) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("[fft] `{}` failed; falling back to CPU (rustfft)", b.name());
            if b == base {
                store(FftBackend::Cpu);
            } else {
                store_plan(None);
            }
            f(FftBackend::Cpu)
        }
    }
}

/// Batched forward FFT; layout as `cpu_fft::fft_batch`.
pub fn fft_batch(signals: &[Vec<f32>]) -> Vec<(Vec<f32>, Vec<f32>)> {
    let samples = signals.iter().map(Vec::len).sum();
    guarded(samples, |b| fft_on(b, signals))
}

/// Batched inverse FFT; layout as `cpu_fft::ifft_batch`.
pub fn ifft_batch(spectra: &[(Vec<f32>, Vec<f32>)]) -> Vec<Vec<f32>> {
    let samples = spectra.iter().map(|(r, _)| r.len()).sum();
    guarded(samples, |b| ifft_on(b, spectra))
}

pub use crate::cpu_fft::psd;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_names() {
        assert_eq!(parse("cpu").unwrap(), FftBackend::Cpu);
        #[cfg(target_os = "macos")]
        assert_eq!(parse("auto").unwrap(), FftBackend::Accelerate);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(parse("auto").unwrap(), FftBackend::Cpu);
        assert!(parse("blas").unwrap_err().contains("no FFT routine"));
        assert!(parse("warp-drive").unwrap_err().contains("unknown FFT backend"));
        #[cfg(target_os = "macos")]
        assert_eq!(parse("Accelerate").unwrap(), FftBackend::Accelerate);
        #[cfg(feature = "rlx-fft")]
        {
            assert_eq!(parse("metal").unwrap(), FftBackend::Rlx(rlx::Device::Metal));
            assert_eq!(parse("coreml").unwrap(), FftBackend::Rlx(rlx::Device::Ane));
            assert_eq!(parse("rlx-cpu").unwrap(), FftBackend::Rlx(rlx::Device::Cpu));
            assert_eq!(parse("rlx:xdna").unwrap(), FftBackend::Rlx(rlx::Device::Xdna));
            // Never MLX: its availability check can abort the process.
            assert!(parse("mlx").unwrap_err().contains("MLX is not offered"));
            assert!(!known().iter().any(|n| n == "mlx"));
        }
        for n in known() {
            assert_eq!(parse(&n).unwrap().name(), n, "name round-trips");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn accelerate_matches_rustfft() {
        probe(FftBackend::Accelerate).unwrap();
        // Sizes the filter and band analysis actually use.
        for n in [256usize, 512, 1000, 1024] {
            let sig: Vec<Vec<f32>> = (0..3)
                .map(|c| (0..n).map(|i| ((i + 5 * c) as f32 * 0.21).cos()).collect())
                .collect();
            let a = crate::accel_fft::fft_batch(&sig);
            let b = crate::cpu_fft::fft_batch(&sig);
            for ((ar, ai), (br, bi)) in a.iter().zip(&b) {
                for k in 0..ar.len() {
                    assert!(
                        (ar[k] - br[k]).abs() < 1e-2 && (ai[k] - bi[k]).abs() < 1e-2,
                        "n={n} bin {k}"
                    );
                }
            }
        }
    }

    #[test]
    fn crossover_needs_a_win_at_every_larger_size() {
        let sizes = [10, 100, 1000, 10000];
        let cpu = [1.0, 2.0, 4.0, 8.0];
        // Wins from 1000 up.
        assert_eq!(
            crossover(&sizes, &cpu, &[Some(5.0), Some(3.0), Some(2.0), Some(1.0)]),
            Some(1000)
        );
        // Wins at 100 but loses at 1000 → only the top size counts.
        assert_eq!(
            crossover(&sizes, &cpu, &[Some(5.0), Some(1.0), Some(5.0), Some(1.0)]),
            Some(10000)
        );
        // Faster but not by MARGIN at the top → never.
        assert_eq!(
            crossover(&sizes, &cpu, &[Some(0.1), Some(0.1), Some(0.1), Some(7.0)]),
            None
        );
        // Failed at the top → never.
        assert_eq!(crossover(&sizes, &cpu, &[Some(0.1), Some(0.1), Some(0.1), None]), None);
        // Wins everywhere.
        assert_eq!(crossover(&sizes, &cpu, &[Some(0.1); 4]), Some(10));
    }

    #[test]
    fn auto_routes_only_big_batches_to_the_gpu() {
        let cpu = FftBackend::Cpu;
        #[cfg(feature = "rlx-fft")]
        let gpu = FftBackend::Rlx(rlx::Device::Metal);
        #[cfg(not(feature = "rlx-fft"))]
        let gpu = FftBackend::Cpu;
        let plan = Some(AutoPlan { gpu, min_samples: 4096 });
        assert_eq!(pick_for(4095, true, plan, cpu), cpu);
        assert_eq!(pick_for(4096, true, plan, cpu), gpu);
        assert_eq!(
            pick_for(1 << 20, true, None, cpu),
            cpu,
            "no plan (calibrating / no GPU win)"
        );
        assert_eq!(
            pick_for(1 << 20, false, plan, cpu),
            cpu,
            "an explicit backend is never overridden"
        );
    }

    #[test]
    fn unusable_backend_is_refused_and_cpu_kept() {
        #[cfg(feature = "rlx-fft")]
        {
            // Not compiled into this test build (no `cuda` feature).
            let err = set_backend("cuda").unwrap_err();
            assert!(err.contains("not compiled in or not present"), "{err}");
        }
        assert_eq!(backend(), FftBackend::Cpu);
    }
}
