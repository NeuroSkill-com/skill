// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Selectable FFT backend for the live DSP (filter, band analysis).
//!
//! | name | runs on |
//! |------|---------|
//! | `auto` (default) | `accelerate` on macOS, `cpu` elsewhere |
//! | `cpu` | rustfft |
//! | `accelerate` | Apple vDSP (macOS) |
//! | any rlx device: `metal`, `mlx`, `ane` / `coreml`, `cuda`, `rocm`, `xdna`, `gpu`, `vulkan`, `rlx-cpu`, … | rlx, if that backend is compiled in and present |
//!
//! The default stays on the CPU on purpose. Measured on an M-series Mac for the
//! live batch (32 ch × 1024, forward + inverse; `tests/fft_backends.rs`):
//! Accelerate ~70 µs, rustfft ~120 µs, rlx-cpu ~350 µs, ANE (CoreML) ~0.6–0.8 ms,
//! Metal 0.4–3 ms and MLX 0.7–7 ms depending on load, plus 4 ms–140 ms for the
//! first call. A GPU FFT also waits behind any other GPU work in the process —
//! a TTS warm-up stalled new sessions for seconds. The others are there to
//! measure and for hardware where they win.
//!
//! **A backend is only used if it gives the right answer.** [`set_backend`]
//! runs a parity probe against rustfft first and refuses a backend that is
//! missing, panics or disagrees, keeping the current one. If a chosen rlx
//! backend later panics (e.g. compiling a new shape), the call is redone on CPU
//! and CPU stays selected — an FFT backend never takes a session down.

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
            return Ok(FftBackend::Rlx(d));
        }
    }
    Err(format!(
        "unknown FFT backend `{name}` (try one of: {})",
        known().join(", ")
    ))
}

/// Backends this build can run here right now (not yet parity-checked).
pub fn known() -> Vec<String> {
    #[allow(unused_mut)]
    let mut v = vec![FftBackend::Cpu.name()];
    #[cfg(target_os = "macos")]
    v.push(FftBackend::Accelerate.name());
    #[cfg(feature = "rlx-fft")]
    for d in rlx::available_devices() {
        v.push(FftBackend::Rlx(d).name());
    }
    v
}

/// Select a backend by setting value. On success returns the backend now in
/// use; on failure the current backend is kept and the reason is returned.
pub fn set_backend(name: &str) -> Result<FftBackend, String> {
    let b = parse(name)?;
    if b == backend() {
        return Ok(b);
    }
    probe(b)?;
    store(b);
    Ok(b)
}

/// Run one FFT round trip on `b` and compare it with rustfft.
pub fn probe(b: FftBackend) -> Result<(), String> {
    if b == FftBackend::Cpu {
        return Ok(());
    }
    #[cfg(feature = "rlx-fft")]
    if let FftBackend::Rlx(d) = b {
        if !rlx::available_devices().contains(&d) {
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

/// Run `f` on the selected backend; if it panics, redo it on CPU and stay on
/// CPU from then on.
fn guarded<T>(f: impl Fn(FftBackend) -> T) -> T {
    let b = backend();
    if b == FftBackend::Cpu {
        return f(b);
    }
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(b))) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("[fft] `{}` failed; falling back to CPU (rustfft)", b.name());
            store(FftBackend::Cpu);
            f(FftBackend::Cpu)
        }
    }
}

/// Batched forward FFT on the selected backend; layout as `cpu_fft::fft_batch`.
pub fn fft_batch(signals: &[Vec<f32>]) -> Vec<(Vec<f32>, Vec<f32>)> {
    guarded(|b| fft_on(b, signals))
}

/// Batched inverse FFT on the selected backend; layout as `cpu_fft::ifft_batch`.
pub fn ifft_batch(spectra: &[(Vec<f32>, Vec<f32>)]) -> Vec<Vec<f32>> {
    guarded(|b| ifft_on(b, spectra))
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
