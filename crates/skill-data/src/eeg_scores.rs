// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Pure-math EEG composite score formulas.
//!
//! These functions compute derived metrics from averaged band powers.
//! They are used both by the live pipeline (via `skill-devices`) and by
//! the history cache when recomputing missing metrics from stored data.

/// Compute meditation score (0–100) from averaged band powers.
///
/// Components:
/// - Alpha dominance (rel_alpha × 200, capped at 40)
/// - Beta penalty (rel_beta × 100, capped at 20, subtracted)
/// - Stillness (stillness × 0.2)
/// - HRV component (RMSSD / 100 × 20, capped at 20; defaults to 10 if unavailable)
pub fn meditation(rel_alpha: f64, rel_beta: f64, stillness: f64, rmssd: Option<f64>) -> f64 {
    let alpha_c = (rel_alpha * 200.0).min(40.0);
    let beta_p = (rel_beta * 100.0).min(20.0);
    let still_c = stillness * 0.2;
    let hrv_c = match rmssd {
        Some(v) if v > 0.0 => (v / 100.0 * 20.0).min(20.0),
        _ => 10.0,
    };
    let raw = alpha_c - beta_p + still_c + hrv_c;
    (raw.clamp(0.0, 100.0) * 10.0).round() / 10.0
}

/// Compute cognitive load score (0–100) from averaged band powers.
///
/// Uses a sigmoid of the theta/alpha ratio:
/// `100 / (1 + exp(−2.5 × (ratio − 1)))`.
///
/// When per-channel frontal/parietal data is unavailable, the averaged
/// `rel_theta` and `rel_alpha` are used as proxies.
pub fn cognitive_load(rel_theta: f64, rel_alpha: f64) -> f64 {
    let parietal_alpha = rel_alpha.max(0.01);
    let ratio = rel_theta / parietal_alpha;
    let raw = 100.0 / (1.0 + (-2.5 * (ratio - 1.0)).exp());
    (raw.clamp(0.0, 100.0) * 10.0).round() / 10.0
}

/// Compute drowsiness score (0–100) from TAR and alpha power.
///
/// Components:
/// - TAR component: TAR / 3 × 80, capped at 80
/// - Alpha spindle component: rel_alpha × 100, capped at 20
pub fn drowsiness(tar: f64, rel_alpha: f64) -> f64 {
    let tar_c = (tar / 3.0 * 80.0).min(80.0);
    let alpha_s = (rel_alpha * 100.0).min(20.0);
    ((tar_c + alpha_s).clamp(0.0, 100.0) * 10.0).round() / 10.0
}

/// Compute stress index approximation from HRV metrics.
///
/// Uses the Baevsky stress formula: `AMo / (2 × MxDMn × Mo)` where
/// AMo ≈ 1/RMSSD, MxDMn ≈ SDNN, Mo ≈ 60/HR. Returns 0 if inputs
/// are insufficient.
pub fn stress_index(hr: f64, rmssd: f64, sdnn: f64) -> f64 {
    if hr <= 0.0 || rmssd <= 0.0 || sdnn <= 0.0 {
        return 0.0;
    }
    let mo = 60.0 / hr; // modal RR interval in seconds
    let amo = 1.0 / (rmssd / 1000.0).max(0.001); // approximate amplitude of mode
    let mx_dmn = sdnn / 1000.0; // variation range in seconds
    let raw = amo / (2.0 * mx_dmn.max(0.001) * mo);
    (raw.clamp(0.0, 1000.0) * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meditation_basic() {
        let m = meditation(0.08, 0.12, 0.0, None);
        assert!(m > 0.0 && m <= 100.0, "meditation={m}");
    }

    #[test]
    fn meditation_high_alpha_high_score() {
        let high = meditation(0.20, 0.05, 50.0, Some(80.0));
        let low = meditation(0.02, 0.20, 0.0, None);
        assert!(high > low, "high alpha+stillness should beat low alpha+high beta");
    }

    #[test]
    fn cognitive_load_low_ratio() {
        let cl = cognitive_load(0.05, 0.10); // ratio 0.5 → below midpoint
        assert!(cl < 50.0, "low theta/alpha ratio → low cognitive load, got {cl}");
    }

    #[test]
    fn cognitive_load_high_ratio() {
        let cl = cognitive_load(0.30, 0.05); // ratio 6.0 → high
        assert!(cl > 90.0, "high theta/alpha ratio → high cognitive load, got {cl}");
    }

    #[test]
    fn drowsiness_low_tar() {
        let d = drowsiness(0.5, 0.05);
        assert!(d < 30.0, "low TAR → low drowsiness, got {d}");
    }

    #[test]
    fn drowsiness_high_tar() {
        let d = drowsiness(3.0, 0.08);
        assert!(d > 70.0, "high TAR → high drowsiness, got {d}");
    }

    #[test]
    fn stress_zero_inputs() {
        assert_eq!(stress_index(0.0, 50.0, 40.0), 0.0);
        assert_eq!(stress_index(72.0, 0.0, 40.0), 0.0);
    }

    #[test]
    fn stress_reasonable_range() {
        let s = stress_index(72.0, 50.0, 40.0);
        assert!(s > 0.0, "stress should be positive, got {s}");
    }
}
