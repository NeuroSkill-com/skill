// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Advanced EEG metric computation helpers (SEF, Hjorth, entropy, DFA, etc.).
//!
//! Pure functions called by [`super::eeg_bands::BandAnalyzer`] during snapshot
//! computation.

// ── Helper functions for new metrics ──────────────────────────────────────────

/// Spectral Edge Frequency: frequency below which `pct` (0–1) of total power lies.
pub(crate) fn spectral_edge_freq(psd: &[f32], bin_hz: f32, pct: f32) -> f32 {
    let total: f32 = psd.iter().sum();
    if total < 1e-20 { return 0.0; }
    let threshold = total * pct;
    let mut cum = 0.0f32;
    for (k, &p) in psd.iter().enumerate() {
        cum += p;
        if cum >= threshold { return k as f32 * bin_hz; }
    }
    (psd.len() - 1) as f32 * bin_hz
}

/// Spectral Centroid: weighted mean frequency.
pub(crate) fn spectral_centroid_fn(psd: &[f32], bin_hz: f32) -> f32 {
    let mut num = 0.0f32;
    let mut den = 0.0f32;
    for (k, &p) in psd.iter().enumerate() {
        let f = k as f32 * bin_hz;
        num += f * p;
        den += p;
    }
    if den > 1e-20 { num / den } else { 0.0 }
}

/// Hjorth parameters: (activity, mobility, complexity).
pub(crate) fn hjorth_params(x: &[f32]) -> (f32, f32, f32) {
    let n = x.len();
    if n < 3 { return (0.0, 0.0, 0.0); }
    let mean = x.iter().sum::<f32>() / n as f32;
    let var0 = x.iter().map(|&v| (v - mean).powi(2)).sum::<f32>() / n as f32;
    if var0 < 1e-20 { return (0.0, 0.0, 0.0); }
    // First derivative
    let mut dx = Vec::with_capacity(n - 1);
    for i in 1..n { dx.push(x[i] - x[i - 1]); }
    let dm = dx.iter().sum::<f32>() / dx.len() as f32;
    let var1 = dx.iter().map(|&v| (v - dm).powi(2)).sum::<f32>() / dx.len() as f32;
    let mobility = (var1 / var0).sqrt();
    // Second derivative
    let mut ddx = Vec::with_capacity(dx.len() - 1);
    for i in 1..dx.len() { ddx.push(dx[i] - dx[i - 1]); }
    let ddm = ddx.iter().sum::<f32>() / ddx.len().max(1) as f32;
    let var2 = ddx.iter().map(|&v| (v - ddm).powi(2)).sum::<f32>() / ddx.len().max(1) as f32;
    let mob_dx = if var1 > 1e-20 { (var2 / var1).sqrt() } else { 0.0 };
    let complexity = if mobility > 1e-10 { mob_dx / mobility } else { 0.0 };
    (var0, mobility, complexity)
}

/// Permutation Entropy (order m=3, delay τ=1), normalised to [0,1].
pub(crate) fn permutation_entropy(x: &[f32]) -> f32 {
    const M: usize = 3;
    let n = x.len();
    if n < M { return 0.0; }
    // 3! = 6 possible patterns
    let mut counts = [0u32; 6];
    for i in 0..=(n - M) {
        let (a, b, c) = (x[i], x[i + 1], x[i + 2]);
        let pat = if a < b {
            if b < c { 0 } else if a < c { 1 } else { 2 }
        } else if a < c { 3 } else if b < c { 4 } else { 5 };
        counts[pat] += 1;
    }
    let total = counts.iter().sum::<u32>() as f32;
    if total < 1.0 { return 0.0; }
    let mut h = 0.0f32;
    for &c in &counts {
        if c > 0 {
            let p = c as f32 / total;
            h -= p * p.ln();
        }
    }
    h / (6.0f32).ln() // normalise by ln(m!)
}

/// Higuchi Fractal Dimension (k_max=8).
pub(crate) fn higuchi_fd(x: &[f32]) -> f32 {
    let n = x.len();
    let k_max = 8.min(n / 4);
    if k_max < 2 { return 0.0; }
    let mut log_k = Vec::with_capacity(k_max);
    let mut log_l = Vec::with_capacity(k_max);
    for k in 1..=k_max {
        let mut lk = 0.0f64;
        let mut count = 0u32;
        for m in 0..k {
            let mut l_m = 0.0f64;
            let floor_n = (n - 1 - m) / k;
            if floor_n < 1 { continue; }
            for i in 1..=floor_n {
                l_m += (x[m + i * k] as f64 - x[m + (i - 1) * k] as f64).abs();
            }
            l_m *= (n as f64 - 1.0) / (floor_n as f64 * k as f64 * k as f64);
            lk += l_m;
            count += 1;
        }
        if count > 0 {
            lk /= count as f64;
            if lk > 1e-20 {
                log_k.push((1.0 / k as f64).ln());
                log_l.push(lk.ln());
            }
        }
    }
    if log_k.len() < 2 { return 0.0; }
    // Linear regression slope
    lin_reg_slope(&log_k, &log_l) as f32
}

/// DFA scaling exponent.
pub(crate) fn dfa_exponent(x: &[f32]) -> f32 {
    let n = x.len();
    if n < 16 { return 0.0; }
    let mean = x.iter().sum::<f32>() / n as f32;
    // Cumulative sum of deviations
    let mut y = vec![0.0f64; n];
    y[0] = (x[0] - mean) as f64;
    for i in 1..n { y[i] = y[i - 1] + (x[i] - mean) as f64; }
    // Scales: powers of 2 from 4 to n/2
    let mut scales = Vec::new();
    let mut s = 4usize;
    while s <= n / 2 {
        scales.push(s);
        s *= 2;
    }
    if scales.len() < 2 { return 0.0; }
    let mut log_s = Vec::with_capacity(scales.len());
    let mut log_f = Vec::with_capacity(scales.len());
    for &seg_len in &scales {
        let n_seg = n / seg_len;
        if n_seg < 1 { continue; }
        let mut total_var = 0.0f64;
        let mut seg_count = 0u32;
        for seg in 0..n_seg {
            let start = seg * seg_len;
            // Linear detrend within segment
            let mut sx = 0.0f64; let mut sy = 0.0f64;
            let mut sxy = 0.0f64; let mut sx2 = 0.0f64;
            for j in 0..seg_len {
                let xj = j as f64;
                let yj = y[start + j];
                sx += xj; sy += yj; sxy += xj * yj; sx2 += xj * xj;
            }
            let nn = seg_len as f64;
            let denom = nn * sx2 - sx * sx;
            if denom.abs() < 1e-20 { continue; }
            let slope = (nn * sxy - sx * sy) / denom;
            let intercept = (sy - slope * sx) / nn;
            let mut var = 0.0f64;
            for j in 0..seg_len {
                let trend = intercept + slope * j as f64;
                let residual = y[start + j] - trend;
                var += residual * residual;
            }
            total_var += var / nn;
            seg_count += 1;
        }
        if seg_count > 0 {
            let f_n = (total_var / seg_count as f64).sqrt();
            if f_n > 1e-20 {
                log_s.push((seg_len as f64).ln());
                log_f.push(f_n.ln());
            }
        }
    }
    if log_s.len() < 2 { return 0.0; }
    lin_reg_slope(&log_s, &log_f) as f32
}

/// Sample Entropy (m=2, r=0.2*std).
pub(crate) fn sample_entropy_fn(x: &[f32]) -> f32 {
    let n = x.len();
    let m = 2usize;
    if n < m + 2 { return 0.0; }
    let mean = x.iter().sum::<f32>() / n as f32;
    let std = (x.iter().map(|&v| (v - mean).powi(2)).sum::<f32>() / n as f32).sqrt();
    let r = 0.2 * std;
    if r < 1e-10 { return 0.0; }
    // Count template matches
    let mut b_count = 0u64; // matches of length m
    let mut a_count = 0u64; // matches of length m+1
    for i in 0..(n - m) {
        for j in (i + 1)..(n - m) {
            // Check length m match
            let mut match_m = true;
            for k in 0..m {
                if (x[i + k] - x[j + k]).abs() > r { match_m = false; break; }
            }
            if match_m {
                b_count += 1;
                // Check length m+1
                if (x[i + m] - x[j + m]).abs() <= r {
                    a_count += 1;
                }
            }
        }
    }
    if b_count == 0 { return 0.0; }
    if a_count == 0 { return (b_count as f32).ln(); } // convention: large value
    -((a_count as f32) / (b_count as f32)).ln()
}

/// Phase-Amplitude Coupling (θ–γ) via sub-window power correlation.
/// Splits the signal into overlapping sub-windows, computes theta and gamma
/// band power in each using Goertzel, then returns the Pearson correlation.
pub(crate) fn pac_theta_gamma_fn(x: &[f32], sr: f32) -> f32 {
    let n = x.len();
    let sub_len = 128.min(n);
    let hop = sub_len / 2;
    if n < sub_len { return 0.0; }
    let n_subs = (n - sub_len) / hop + 1;
    if n_subs < 3 { return 0.0; }
    let mut theta_pwr = Vec::with_capacity(n_subs);
    let mut gamma_pwr = Vec::with_capacity(n_subs);
    // Target frequencies for Goertzel
    let theta_freqs: &[f32] = &[4.0, 5.0, 6.0, 7.0, 8.0];
    let gamma_freqs: &[f32] = &[30.0, 35.0, 40.0, 45.0, 50.0];
    for s in 0..n_subs {
        let start = s * hop;
        let sub = &x[start..start + sub_len];
        let tp: f32 = theta_freqs.iter().map(|&f| goertzel_power(sub, sr, f)).sum();
        let gp: f32 = gamma_freqs.iter().map(|&f| goertzel_power(sub, sr, f)).sum();
        theta_pwr.push(tp);
        gamma_pwr.push(gp);
    }
    pearson(&theta_pwr, &gamma_pwr).abs()
}

/// Goertzel algorithm: power at a single frequency.
fn goertzel_power(x: &[f32], sr: f32, freq: f32) -> f32 {
    let n = x.len();
    let k = (freq * n as f32 / sr).round();
    let w = 2.0 * std::f32::consts::PI * k / n as f32;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &sample in x {
        let s0 = sample + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    s1 * s1 + s2 * s2 - coeff * s1 * s2
}

/// Pearson correlation coefficient.
fn pearson(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len() as f32;
    if n < 2.0 { return 0.0; }
    let ma = a.iter().sum::<f32>() / n;
    let mb = b.iter().sum::<f32>() / n;
    let mut cov = 0.0f32;
    let mut va = 0.0f32;
    let mut vb = 0.0f32;
    for i in 0..a.len() {
        let da = a[i] - ma;
        let db = b[i] - mb;
        cov += da * db;
        va += da * da;
        vb += db * db;
    }
    let denom = (va * vb).sqrt();
    if denom > 1e-12 { cov / denom } else { 0.0 }
}

/// Laterality Index: generalised L/R asymmetry.
/// Uses total broadband power: (right − left) / (right + left).
/// TP9 (left), AF7 (left), AF8 (right), TP10 (right).
fn laterality_index_fn(ch: &[BandPowers]) -> f32 {
    if ch.len() < 4 { return 0.0; }
    let left  = (ch[0].delta + ch[0].theta + ch[0].alpha + ch[0].beta + ch[0].gamma)
              + (ch[1].delta + ch[1].theta + ch[1].alpha + ch[1].beta + ch[1].gamma);
    let right = (ch[2].delta + ch[2].theta + ch[2].alpha + ch[2].beta + ch[2].gamma)
              + (ch[3].delta + ch[3].theta + ch[3].alpha + ch[3].beta + ch[3].gamma);
    let total = left + right;
    if total > 1e-12 { (right - left) / total } else { 0.0 }
}

/// Simple linear regression slope.
fn lin_reg_slope(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    if n < 2.0 { return 0.0; }
    let sx: f64 = x.iter().sum();
    let sy: f64 = y.iter().sum();
    let sxy: f64 = x.iter().zip(y).map(|(a, b)| a * b).sum();
    let sx2: f64 = x.iter().map(|a| a * a).sum();
    let denom = n * sx2 - sx * sx;
    if denom.abs() < 1e-20 { 0.0 } else { (n * sxy - sx * sy) / denom }
}

