// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! FFT via Apple's Accelerate framework (vDSP) — macOS only.
//!
//! Same API surface and output layout as `cpu_fft`: forward unnormalised,
//! inverse divided by `n`, inputs zero-padded to the next power of two.
//! Uses the complex split-format `vDSP_fft_zip` (real input, zero imaginary)
//! so the full spectrum comes back without unpacking vDSP's packed real format.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

#[repr(C)]
struct DspSplitComplex {
    realp: *mut f32,
    imagp: *mut f32,
}

const FFT_RADIX2: i32 = 0;
const FFT_FORWARD: i32 = 1;
const FFT_INVERSE: i32 = -1;

#[link(name = "Accelerate", kind = "framework")]
unsafe extern "C" {
    fn vDSP_create_fftsetup(log2n: usize, radix: i32) -> *mut c_void;
    fn vDSP_fft_zip(setup: *mut c_void, c: *const DspSplitComplex, stride: isize, log2n: usize, direction: i32);
}

/// An `FFTSetup` (twiddle tables). Read-only once built; vDSP allows using one
/// from several threads at once. Kept for the life of the process.
#[derive(Clone, Copy)]
struct Setup(*mut c_void);
// SAFETY: see above — vDSP setups are immutable after creation.
unsafe impl Send for Setup {}

fn setup_for(log2n: usize) -> Option<Setup> {
    static SETUPS: OnceLock<Mutex<HashMap<usize, Setup>>> = OnceLock::new();
    let mut map = SETUPS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(s) = map.get(&log2n) {
        return Some(*s);
    }
    // SAFETY: plain constructor; null means failure.
    let p = unsafe { vDSP_create_fftsetup(log2n, FFT_RADIX2) };
    if p.is_null() {
        return None;
    }
    map.insert(log2n, Setup(p));
    Some(Setup(p))
}

fn transform(re: &mut [f32], im: &mut [f32], direction: i32) {
    let n = re.len();
    debug_assert!(n.is_power_of_two() && im.len() == n);
    let log2n = n.trailing_zeros() as usize;
    let setup = setup_for(log2n).expect("vDSP_create_fftsetup failed");
    let split = DspSplitComplex {
        realp: re.as_mut_ptr(),
        imagp: im.as_mut_ptr(),
    };
    // SAFETY: both buffers hold n = 2^log2n floats, stride 1, setup built for
    // log2n.
    unsafe { vDSP_fft_zip(setup.0, &split, 1, log2n, direction) };
}

/// Batched forward FFT; see the module docs for the layout.
pub fn fft_batch(signals: &[Vec<f32>]) -> Vec<(Vec<f32>, Vec<f32>)> {
    signals
        .iter()
        .map(|s| {
            let n = s.len().next_power_of_two();
            let mut re = vec![0.0f32; n];
            re[..s.len()].copy_from_slice(s);
            let mut im = vec![0.0f32; n];
            transform(&mut re, &mut im, FFT_FORWARD);
            (re, im)
        })
        .collect()
}

/// Batched inverse FFT: real part of the reconstructed signal, divided by `n`.
pub fn ifft_batch(spectra: &[(Vec<f32>, Vec<f32>)]) -> Vec<Vec<f32>> {
    // vDSP's radix-2 FFT needs 2^k points; spectra from `fft_batch` always are.
    if spectra.iter().any(|(re, _)| !re.len().is_power_of_two()) {
        return crate::cpu_fft::ifft_batch(spectra);
    }
    spectra
        .iter()
        .map(|(re, im)| {
            let n = re.len();
            let (mut r, mut i) = (re.clone(), im.clone());
            transform(&mut r, &mut i, FFT_INVERSE);
            let inv_n = 1.0 / n as f32;
            r.iter().map(|x| x * inv_n).collect()
        })
        .collect()
}
