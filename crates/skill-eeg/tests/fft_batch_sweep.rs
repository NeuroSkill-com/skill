// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Measurement (not a gate; `--ignored`): per-backend forward / inverse FFT
//! time across batch sizes (channels × length), to find where a GPU backend
//! overtakes Accelerate. Feeds the size-based choice in `fft` (`auto`).
//!
//! `cargo test --release -p skill-eeg --features rlx-fft-metal,rlx-fft-gpu \
//!     --test fft_batch_sweep -- --ignored --nocapture`

use std::time::{Duration, Instant};

use skill_eeg::fft;

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

#[test]
#[ignore]
fn fft_batch_sweep() {
    let shapes = [
        (4, 256),
        (32, 256),
        (32, 1024),
        (64, 1024),
        (128, 1024),
        (256, 1024),
        (32, 4096),
        (128, 4096),
        (512, 1024),
        (256, 4096),
        (1024, 1024),
        (512, 4096),
    ];
    let backends = ["cpu", "accelerate", "metal", "gpu"];
    eprintln!(
        "{:>6} {:>6} {:>9} | {}",
        "ch",
        "len",
        "samples",
        backends.map(|b| format!("{b:>22}")).join("")
    );
    for (ch, len) in shapes {
        let sig: Vec<Vec<f32>> = (0..ch)
            .map(|c| (0..len).map(|i| ((i + c) as f32 * 0.3).sin()).collect())
            .collect();
        let mut row = String::new();
        for name in backends {
            let ok = fft::set_backend(name).is_ok();
            if !ok {
                row.push_str(&format!("{:>22}", "-"));
                continue;
            }
            let spec = fft::fft_batch(&sig); // warm-up (compile for this shape)
            let _ = fft::ifft_batch(&spec);
            let (mut f, mut i) = (Vec::new(), Vec::new());
            for _ in 0..9 {
                let t = Instant::now();
                let s = fft::fft_batch(&sig);
                f.push(t.elapsed());
                let t = Instant::now();
                std::hint::black_box(fft::ifft_batch(&s));
                i.push(t.elapsed());
            }
            row.push_str(&format!(
                "{:>22}",
                format!(
                    "{:.2}/{:.2}ms",
                    median(f).as_secs_f64() * 1e3,
                    median(i).as_secs_f64() * 1e3
                )
            ));
        }
        eprintln!("{ch:>6} {len:>6} {:>9} | {row}", ch * len);
    }
    eprintln!("(cells: forward/inverse median)");
}

/// What `auto` would decide on this machine (prints the per-size timings).
#[test]
#[ignore]
fn auto_calibration_plan() {
    fft::set_backend("cpu").unwrap();
    let cpu = fft::parse("auto").unwrap();
    let plan = fft::calibrate(cpu);
    eprintln!("auto plan vs `{}`: {plan:?}", cpu.name());
}
