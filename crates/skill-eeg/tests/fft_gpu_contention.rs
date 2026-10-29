// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Measurement (not a gate; `--ignored`), kept as the evidence for running the
//! live DSP on CPU: EEG FFT latency on Metal while another thread
//! keeps the shared rlx command queue busy, as the TTS warm-up does.
#![cfg(all(target_os = "macos", feature = "rlx-fft-metal"))]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rlx::{DType, Device, Graph, Session, Shape};

fn fft_latencies(n: usize) -> Vec<Duration> {
    let sig: Vec<Vec<f32>> = (0..32)
        .map(|c| (0..1024).map(|i| ((i + c) as f32 * 0.3).sin()).collect())
        .collect();
    (0..n)
        .map(|_| {
            let t = Instant::now();
            let _ = skill_eeg::rlx_fft::fft_batch(&sig);
            std::thread::sleep(Duration::from_millis(20));
            t.elapsed().saturating_sub(Duration::from_millis(20))
        })
        .collect()
}

#[test]
#[ignore]
fn fft_latency_under_gpu_contention() {
    skill_eeg::rlx_fft::init_device(Device::Metal);
    let idle = fft_latencies(20);

    let stop = Arc::new(AtomicBool::new(false));
    let s2 = stop.clone();
    let hog = std::thread::spawn(move || {
        let n = 3072;
        let mut g = Graph::new("gpu_hog");
        let a = g.input("a", Shape::new(&[n, n], DType::F32));
        let b = g.input("b", Shape::new(&[n, n], DType::F32));
        let mut x = g.matmul(a, b, Shape::new(&[n, n], DType::F32));
        for _ in 0..7 {
            x = g.matmul(x, b, Shape::new(&[n, n], DType::F32));
        }
        g.set_outputs(vec![x]);
        let mut c = Session::new(Device::Metal).compile(g);
        let data = vec![0.001f32; n * n];
        let mut runs = 0;
        let t = Instant::now();
        while !s2.load(Ordering::Relaxed) {
            let _ = c.run(&[("a", &data), ("b", &data)]);
            runs += 1;
        }
        (runs, t.elapsed())
    });
    std::thread::sleep(Duration::from_millis(500));
    let busy = fft_latencies(20);
    stop.store(true, Ordering::Relaxed);
    let (runs, el) = hog.join().unwrap();

    let med = |v: &[Duration]| {
        let mut v = v.to_vec();
        v.sort();
        v[v.len() / 2]
    };
    let max = |v: &[Duration]| *v.iter().max().unwrap();
    let sig: Vec<Vec<f32>> = (0..32)
        .map(|c| (0..1024).map(|i| ((i + c) as f32 * 0.3).sin()).collect())
        .collect();
    let cpu: Vec<Duration> = (0..50)
        .map(|_| {
            let t = Instant::now();
            let f = skill_eeg::cpu_fft::fft_batch(&sig);
            std::hint::black_box(skill_eeg::cpu_fft::ifft_batch(&f));
            t.elapsed()
        })
        .collect();
    eprintln!("CPU fft+ifft (rustfft): median {:?} max {:?}", med(&cpu), max(&cpu));
    eprintln!("hog: {runs} runs in {el:?} ({:?}/run)", el / runs.max(1));
    eprintln!("FFT idle: median {:?} max {:?}", med(&idle), max(&idle));
    eprintln!("FFT busy: median {:?} max {:?}", med(&busy), max(&busy));
}
