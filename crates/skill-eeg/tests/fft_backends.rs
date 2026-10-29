// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Measurement (not a gate; `--ignored`): which FFT backends this build can
//! use here, and how fast each is on the live DSP's batch (32 ch × 1024,
//! forward + inverse). A backend only counts if `fft::set_backend` accepts it,
//! i.e. it passed the parity probe against rustfft.
//!
//! `cargo test --release -p skill-eeg --features rlx-fft-metal,rlx-fft-mlx,rlx-fft-ane \
//!     --test fft_backends -- --ignored --nocapture`

use std::time::{Duration, Instant};

use skill_eeg::fft;

#[test]
#[ignore]
fn fft_backend_table() {
    let sig: Vec<Vec<f32>> = (0..32)
        .map(|c| (0..1024).map(|i| ((i + c) as f32 * 0.3).sin()).collect())
        .collect();
    let names = [
        "cpu",
        "accelerate",
        "rlx-cpu",
        "metal",
        "mlx",
        "ane",
        "gpu",
        "vulkan",
        "cuda",
        "rocm",
        "xdna",
        "blas",
    ];
    eprintln!("{:<12} {:>12} {:>12}  note", "backend", "median", "first call");
    for name in names {
        match fft::set_backend(name) {
            Err(e) => eprintln!("{name:<12} {:>12} {:>12}  {e}", "-", "-"),
            Ok(b) => {
                let t = Instant::now();
                let _ = fft::ifft_batch(&fft::fft_batch(&sig));
                let first = t.elapsed();
                let mut lat: Vec<Duration> = (0..50)
                    .map(|_| {
                        let t = Instant::now();
                        std::hint::black_box(fft::ifft_batch(&fft::fft_batch(&sig)));
                        t.elapsed()
                    })
                    .collect();
                lat.sort();
                let still = if fft::backend() == b {
                    ""
                } else {
                    "fell back to CPU during the run"
                };
                eprintln!("{name:<12} {:>12?} {:>12?}  {still}", lat[25], first);
            }
        }
    }
    fft::set_backend("cpu").unwrap();
}
