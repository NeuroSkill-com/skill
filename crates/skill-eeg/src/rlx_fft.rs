// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! GPU-accelerated FFT via RLX — Metal on macOS, CUDA → wgpu → CPU elsewhere.
//!
//! Provides the same `fft_batch` / `ifft_batch` / `psd` surface as `cpu_fft`
//! but dispatches to the rlx runtime using whichever backend was selected at
//! daemon startup via [`init_device`].
//!
//! Compiled graphs are cached by `(device, batch, n)` so the first call for a
//! given shape compiles once; subsequent calls reuse the cached executable.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use rlx::ir::fft::FftNorm;
use rlx::{CompiledGraph, DType, Device, Graph, Session, Shape};

// ── Device selection ─────────────────────────────────────────────────────────

static FFT_DEVICE: OnceLock<Device> = OnceLock::new();

/// Set the RLX device used for FFT dispatch.  Call once at daemon startup
/// (e.g. from `pipeline.rs`) before the first `fft_batch` call.
/// Subsequent calls are silent no-ops.
pub fn init_device(device: Device) {
    let _ = FFT_DEVICE.set(device);
}

fn current_device() -> Device {
    FFT_DEVICE.get().copied().unwrap_or(Device::Cpu)
}

fn device_tag(d: Device) -> u8 {
    match d {
        Device::Cpu => 0,
        Device::Metal => 1,
        Device::Mlx => 2,
        Device::Cuda => 3,
        Device::Gpu => 4,
        Device::Rocm => 5,
        _ => 255,
    }
}

// ── Kernel cache ─────────────────────────────────────────────────────────────

type CacheKey = (u8, usize, usize); // (device_tag, batch, n)
type CacheMap = Mutex<HashMap<CacheKey, Arc<Mutex<CompiledGraph>>>>;

static FWD_CACHE: OnceLock<CacheMap> = OnceLock::new();
static INV_CACHE: OnceLock<CacheMap> = OnceLock::new();

fn fwd_cache() -> &'static CacheMap {
    FWD_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn inv_cache() -> &'static CacheMap {
    INV_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_or_compile_fwd(device: Device, batch: usize, n: usize) -> Arc<Mutex<CompiledGraph>> {
    let key = (device_tag(device), batch, n);
    {
        let lock = fwd_cache().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(g) = lock.get(&key) {
            return Arc::clone(g);
        }
    }
    // Compile: real input [batch, n] → auto-pads to next pow2 → (re, im) [batch, n_pad].
    let mut g = Graph::new("eeg_fft_fwd");
    let x = g.input("x", Shape::new(&[batch, n], DType::F32));
    let (re, im) = g.fft_batch_real(x, FftNorm::Forward);
    g.set_outputs(vec![re, im]);
    let compiled = Arc::new(Mutex::new(Session::new(device).compile(g)));

    let mut lock = fwd_cache().lock().unwrap_or_else(|e| e.into_inner());
    Arc::clone(lock.entry(key).or_insert(compiled))
}

fn get_or_compile_inv(device: Device, batch: usize, n_pad: usize) -> Arc<Mutex<CompiledGraph>> {
    let key = (device_tag(device), batch, n_pad);
    {
        let lock = inv_cache().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(g) = lock.get(&key) {
            return Arc::clone(g);
        }
    }
    // Compile: (re, im) [batch, n_pad] → real time-domain [batch, n_pad].
    let mut g = Graph::new("eeg_fft_inv");
    let re = g.input("re", Shape::new(&[batch, n_pad], DType::F32));
    let im = g.input("im", Shape::new(&[batch, n_pad], DType::F32));
    let out = g.ifft_spectrum(re, im, FftNorm::Forward);
    g.set_outputs(vec![out]);
    let compiled = Arc::new(Mutex::new(Session::new(device).compile(g)));

    let mut lock = inv_cache().lock().unwrap_or_else(|e| e.into_inner());
    Arc::clone(lock.entry(key).or_insert(compiled))
}

// ── Public API (same surface as cpu_fft) ─────────────────────────────────────

/// Batched forward FFT.
///
/// All signals must have the same length.  Returns `(real, imag)` pairs, each
/// of length `n.next_power_of_two()`, matching the `cpu_fft::fft_batch` layout.
pub fn fft_batch(signals: &[Vec<f32>]) -> Vec<(Vec<f32>, Vec<f32>)> {
    fft_batch_on(current_device(), signals)
}

/// [`fft_batch`] on an explicit device (used by [`crate::fft`]).
pub fn fft_batch_on(device: Device, signals: &[Vec<f32>]) -> Vec<(Vec<f32>, Vec<f32>)> {
    if signals.is_empty() {
        return vec![];
    }
    let batch = signals.len();
    let n = signals[0].len();
    let kernel = get_or_compile_fwd(device, batch, n);
    let n_pad = n.next_power_of_two();

    let flat: Vec<f32> = signals.iter().flat_map(|s| s.iter().copied()).collect();
    let mut lock = kernel.lock().unwrap_or_else(|e| e.into_inner());
    let outputs = lock.run(&[("x", &flat)]);
    drop(lock);

    let re_flat = &outputs[0];
    let im_flat = &outputs[1];
    (0..batch)
        .map(|i| {
            (
                re_flat[i * n_pad..(i + 1) * n_pad].to_vec(),
                im_flat[i * n_pad..(i + 1) * n_pad].to_vec(),
            )
        })
        .collect()
}

/// Batched inverse FFT.
///
/// Each spectrum is a `(real, imag)` pair of length `n_pad` (power of two).
/// Returns the real time-domain signal of the same length.
pub fn ifft_batch(spectra: &[(Vec<f32>, Vec<f32>)]) -> Vec<Vec<f32>> {
    ifft_batch_on(current_device(), spectra)
}

/// [`ifft_batch`] on an explicit device (used by [`crate::fft`]).
pub fn ifft_batch_on(device: Device, spectra: &[(Vec<f32>, Vec<f32>)]) -> Vec<Vec<f32>> {
    if spectra.is_empty() {
        return vec![];
    }
    let batch = spectra.len();
    let n_pad = spectra[0].0.len();
    let kernel = get_or_compile_inv(device, batch, n_pad);

    let re_flat: Vec<f32> = spectra.iter().flat_map(|(re, _)| re.iter().copied()).collect();
    let im_flat: Vec<f32> = spectra.iter().flat_map(|(_, im)| im.iter().copied()).collect();
    let mut lock = kernel.lock().unwrap_or_else(|e| e.into_inner());
    let outputs = lock.run(&[("re", &re_flat), ("im", &im_flat)]);
    drop(lock);

    let out_flat = &outputs[0];
    (0..batch)
        .map(|i| out_flat[i * n_pad..(i + 1) * n_pad].to_vec())
        .collect()
}

/// One-sided Power Spectral Density: `(r² + i²) / n` for each bin.
///
/// This is a CPU-side reduction on already-downloaded FFT output; GPU
/// acceleration here would be dominated by transfer overhead.
pub fn psd(real: &[f32], imag: &[f32]) -> Vec<f32> {
    let n = real.len();
    real.iter()
        .zip(imag.iter())
        .map(|(&r, &i)| (r * r + i * i) / n as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every FFT backend must match `cpu_fft` exactly in layout and scale
    /// (band powers and the filter depend on it). rlx's CPU backend here; the
    /// selectable devices are checked the same way when chosen
    /// (`crate::fft::set_backend`).
    #[test]
    fn matches_cpu_fft() {
        for n in [250usize, 256, 1000, 1024] {
            let sig: Vec<Vec<f32>> = (0..4)
                .map(|c| {
                    (0..n)
                        .map(|i| ((i * (c + 3)) as f32 * 0.07).sin() * 20.0 + (i % 7) as f32)
                        .collect()
                })
                .collect();
            let a = fft_batch(&sig);
            let b = crate::cpu_fft::fft_batch(&sig);
            let scale = a[0].0.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
            for ((ar, ai), (br, bi)) in a.iter().zip(&b) {
                assert_eq!(ar.len(), br.len(), "n={n}: padded length");
                for k in 0..ar.len() {
                    assert!(
                        (ar[k] - br[k]).abs() <= 1e-3 * scale && (ai[k] - bi[k]).abs() <= 1e-3 * scale,
                        "n={n} bin {k}: rlx ({}, {}) vs cpu ({}, {})",
                        ar[k],
                        ai[k],
                        br[k],
                        bi[k]
                    );
                }
            }
            let ra = ifft_batch(&a);
            let rb = crate::cpu_fft::ifft_batch(&b);
            for (x, y) in ra.iter().zip(&rb) {
                for k in 0..x.len() {
                    assert!(
                        (x[k] - y[k]).abs() <= 1e-3,
                        "n={n} ifft sample {k}: {} vs {}",
                        x[k],
                        y[k]
                    );
                }
            }
        }
    }
}
