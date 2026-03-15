// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Real-time head orientation from IMU (accelerometer + gyroscope).
//!
//! Uses a complementary filter to fuse accelerometer (low-frequency, gravity
//! reference) with gyroscope (high-frequency, drift-prone) for stable
//! pitch/roll estimation.  Yaw is gyro-only (no magnetometer on Muse).
//!
//! Also computes a **stillness score** (0–100) from angular velocity magnitude
//! and detects discrete **nod** and **shake** gestures.
//!
//! # References
//! - Colton, S. (2007). The balance filter. MIT.
//! - Mahony, R. et al. (2008). Nonlinear complementary filters on SO(3).
//!   IEEE Trans. Automatic Control, 53(5), 1203–1218.

/// IMU sample rate (Hz) — Muse fires at ~52 Hz, 3 samples per notification.
const IMU_SR: f64 = 52.0;
/// Complementary filter coefficient (0–1).  Higher = more trust in gyro.
const ALPHA: f64 = 0.96;
/// Stillness: EMA smoothing time constant (seconds).
const STILL_TAU_S: f64 = 1.0;
/// Stillness: angular velocity threshold (°/s) below which score is ~100.
const STILL_QUIET_DPS: f64 = 3.0;
/// Stillness: angular velocity above which score is ~0.
const STILL_ACTIVE_DPS: f64 = 50.0;
/// Nod detection: minimum pitch delta (degrees) within the nod window.
const NOD_THRESHOLD_DEG: f64 = 12.0;
/// Shake detection: minimum yaw delta (degrees) within the shake window.
const SHAKE_THRESHOLD_DEG: f64 = 15.0;
/// Gesture detection window (seconds).
const GESTURE_WINDOW_S: f64 = 0.6;
/// Gesture refractory period (seconds).
const GESTURE_REFRACTORY_S: f64 = 1.0;

/// Head orientation and movement metrics.
#[derive(Clone, Debug, Default)]
pub struct HeadPoseMetrics {
    /// Pitch angle in degrees.  Positive = looking up, negative = looking down.
    pub pitch: f64,
    /// Roll angle in degrees.  Positive = tilting right ear down.
    pub roll: f64,
    /// Stillness score 0–100.  100 = perfectly still, 0 = vigorous movement.
    pub stillness: f64,
    /// Total nod count since connection.
    pub nod_count: u64,
    /// Total shake count since connection.
    pub shake_count: u64,
}

pub struct HeadPoseTracker {
    /// Fused pitch (degrees).
    pitch: f64,
    /// Fused roll (degrees).
    roll: f64,
    /// Accumulated yaw from gyro integration (degrees, drifts over time).
    yaw: f64,
    /// Smoothed angular velocity magnitude for stillness.
    ang_vel_ema: f64,
    /// Initialised flag (first sample seeds from accel).
    initialised: bool,
    /// Gesture detection: recent pitch history.
    pitch_history: std::collections::VecDeque<f64>,
    /// Gesture detection: recent yaw history.
    yaw_history: std::collections::VecDeque<f64>,
    /// Nod count.
    nod_count: u64,
    /// Shake count.
    shake_count: u64,
    /// Refractory counters (samples remaining).
    nod_refractory: usize,
    shake_refractory: usize,
}

impl HeadPoseTracker {
    pub fn new() -> Self {
        let hist_len = (GESTURE_WINDOW_S * IMU_SR) as usize + 1;
        Self {
            pitch: 0.0,
            roll: 0.0,
            yaw: 0.0,
            ang_vel_ema: 0.0,
            initialised: false,
            pitch_history: std::collections::VecDeque::with_capacity(hist_len),
            yaw_history: std::collections::VecDeque::with_capacity(hist_len),
            nod_count: 0,
            shake_count: 0,
            nod_refractory: 0,
            shake_refractory: 0,
        }
    }

    /// Feed one accelerometer + gyroscope sample.
    /// `accel`: [x, y, z] in g.
    /// `gyro`:  [x, y, z] in °/s.
    pub fn update(&mut self, accel: [f32; 3], gyro: [f32; 3]) {
        let dt = 1.0 / IMU_SR;
        let ax = accel[0] as f64;
        let ay = accel[1] as f64;
        let az = accel[2] as f64;
        let gx = gyro[0] as f64;
        let gy = gyro[1] as f64;
        let gz = gyro[2] as f64;

        // Pitch and roll from accelerometer (gravity reference).
        let accel_pitch = ax.atan2((ay * ay + az * az).sqrt()).to_degrees();
        let accel_roll  = ay.atan2((ax * ax + az * az).sqrt()).to_degrees();

        if !self.initialised {
            self.pitch = accel_pitch;
            self.roll  = accel_roll;
            self.initialised = true;
            return;
        }

        // Gyro integration.
        let gyro_pitch = self.pitch + gx * dt;
        let gyro_roll  = self.roll  + gy * dt;
        self.yaw      += gz * dt;

        // Complementary filter: fuse gyro (short-term) with accel (long-term).
        self.pitch = ALPHA * gyro_pitch + (1.0 - ALPHA) * accel_pitch;
        self.roll  = ALPHA * gyro_roll  + (1.0 - ALPHA) * accel_roll;

        // Angular velocity magnitude for stillness.
        let ang_vel = (gx * gx + gy * gy + gz * gz).sqrt();
        let ema_alpha = 1.0 - (-dt / STILL_TAU_S).exp();
        self.ang_vel_ema += ema_alpha * (ang_vel - self.ang_vel_ema);

        // ── Gesture detection ────────────────────────────────────────────────
        let hist_max = (GESTURE_WINDOW_S * IMU_SR) as usize;

        self.pitch_history.push_back(self.pitch);
        if self.pitch_history.len() > hist_max { self.pitch_history.pop_front(); }

        self.yaw_history.push_back(self.yaw);
        if self.yaw_history.len() > hist_max { self.yaw_history.pop_front(); }

        // Decrement refractories.
        if self.nod_refractory > 0 { self.nod_refractory -= 1; }
        if self.shake_refractory > 0 { self.shake_refractory -= 1; }

        let refractory_samples = (GESTURE_REFRACTORY_S * IMU_SR) as usize;

        // Nod: pitch oscillation (look down then up, or up then down).
        if self.nod_refractory == 0 && self.pitch_history.len() >= 3 {
            let mn = self.pitch_history.iter().cloned().fold(f64::INFINITY, f64::min);
            let mx = self.pitch_history.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            if mx - mn > NOD_THRESHOLD_DEG {
                self.nod_count += 1;
                self.nod_refractory = refractory_samples;
                self.pitch_history.clear();
            }
        }

        // Shake: yaw oscillation (look left then right, or right then left).
        if self.shake_refractory == 0 && self.yaw_history.len() >= 3 {
            let mn = self.yaw_history.iter().cloned().fold(f64::INFINITY, f64::min);
            let mx = self.yaw_history.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            if mx - mn > SHAKE_THRESHOLD_DEG {
                self.shake_count += 1;
                self.shake_refractory = refractory_samples;
                self.yaw_history.clear();
            }
        }
    }

    /// Current metrics snapshot.
    pub fn metrics(&self) -> HeadPoseMetrics {
        // Stillness: map angular velocity EMA to 0–100 score.
        let stillness = if self.ang_vel_ema <= STILL_QUIET_DPS {
            100.0
        } else if self.ang_vel_ema >= STILL_ACTIVE_DPS {
            0.0
        } else {
            100.0 * (1.0 - (self.ang_vel_ema - STILL_QUIET_DPS)
                / (STILL_ACTIVE_DPS - STILL_QUIET_DPS))
        };

        HeadPoseMetrics {
            pitch: (self.pitch * 10.0).round() / 10.0,
            roll:  (self.roll * 10.0).round() / 10.0,
            stillness: stillness.round(),
            nod_count: self.nod_count,
            shake_count: self.shake_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_orientation_at_rest() {
        let mut t = HeadPoseTracker::new();
        // Muse at rest on head: accel ≈ [0, 0, 1] g (upright).
        for _ in 0..100 {
            t.update([0.0, 0.0, 1.0], [0.0, 0.0, 0.0]);
        }
        let m = t.metrics();
        assert!(m.pitch.abs() < 5.0, "pitch should be near 0 at rest, got {}", m.pitch);
        assert!(m.roll.abs() < 5.0, "roll should be near 0 at rest, got {}", m.roll);
        assert!(m.stillness > 90.0, "should be very still, got {}", m.stillness);
    }

    #[test]
    fn movement_reduces_stillness() {
        let mut t = HeadPoseTracker::new();
        for _ in 0..50 {
            t.update([0.0, 0.0, 1.0], [0.0, 0.0, 0.0]);
        }
        // Vigorous rotation.
        for _ in 0..50 {
            t.update([0.2, 0.1, 0.9], [100.0, 80.0, 60.0]);
        }
        let m = t.metrics();
        assert!(m.stillness < 50.0, "should not be still during motion, got {}", m.stillness);
    }

    #[test]
    fn tilt_forward_gives_positive_pitch() {
        let mut t = HeadPoseTracker::new();
        // accel = [1, 0, 0] → looking up (tilted forward) → pitch ≈ 90°.
        // First sample seeds from accel (no metrics yet), subsequent samples converge.
        for _ in 0..200 {
            t.update([1.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        }
        let m = t.metrics();
        assert!(m.pitch > 50.0, "tilted forward: expected pitch > 50°, got {}", m.pitch);
    }

    #[test]
    fn tilt_right_gives_positive_roll() {
        let mut t = HeadPoseTracker::new();
        // accel = [0, 1, 0] → tilting right → roll ≈ 90°.
        for _ in 0..200 {
            t.update([0.0, 1.0, 0.0], [0.0, 0.0, 0.0]);
        }
        let m = t.metrics();
        assert!(m.roll > 50.0, "tilted right: expected roll > 50°, got {}", m.roll);
    }

    #[test]
    fn zero_gyro_at_rest_has_zero_yaw() {
        let mut t = HeadPoseTracker::new();
        for _ in 0..50 {
            t.update([0.0, 0.0, 1.0], [0.0, 0.0, 0.0]);
        }
        // With no gyro input the integrated yaw stays at 0.
        assert_eq!(t.yaw, 0.0);
    }

    #[test]
    fn nod_detected_after_large_pitch_oscillation() {
        let mut t = HeadPoseTracker::new();
        // Settle upright.
        for _ in 0..30 { t.update([0.0, 0.0, 1.0], [0.0, 0.0, 0.0]); }
        // Simulate a nod: large pitch gyro oscillation.
        // NOD_THRESHOLD_DEG = 12° — drive ±15°/sample accumulation via gyro.
        for _ in 0..20 { t.update([0.0, 0.0, 1.0], [100.0, 0.0, 0.0]); }  // look up
        for _ in 0..20 { t.update([0.0, 0.0, 1.0], [-100.0, 0.0, 0.0]); } // look down
        assert!(t.metrics().nod_count >= 1, "nod should be detected");
    }

    #[test]
    fn shake_detected_after_large_yaw_oscillation() {
        let mut t = HeadPoseTracker::new();
        // Settle upright.
        for _ in 0..30 { t.update([0.0, 0.0, 1.0], [0.0, 0.0, 0.0]); }
        // Simulate a head-shake: large yaw gyro oscillation.
        // SHAKE_THRESHOLD_DEG = 15°.
        for _ in 0..20 { t.update([0.0, 0.0, 1.0], [0.0, 0.0, 100.0]); }  // yaw right
        for _ in 0..20 { t.update([0.0, 0.0, 1.0], [0.0, 0.0, -100.0]); } // yaw left
        assert!(t.metrics().shake_count >= 1, "shake should be detected");
    }

    #[test]
    fn stillness_is_100_when_completely_still() {
        let mut t = HeadPoseTracker::new();
        // Many samples at rest → EMA should converge near STILL_QUIET_DPS.
        for _ in 0..500 {
            t.update([0.0, 0.0, 1.0], [0.0, 0.0, 0.0]);
        }
        assert_eq!(t.metrics().stillness, 100.0);
    }

    #[test]
    fn metrics_pitch_and_roll_are_rounded_to_one_decimal() {
        let mut t = HeadPoseTracker::new();
        for _ in 0..100 { t.update([0.0, 0.0, 1.0], [0.0, 0.0, 0.0]); }
        let m = t.metrics();
        // Values are rounded to 1 decimal place: (v * 10).round() / 10.
        let pitch_rounded = (m.pitch * 10.0).round() / 10.0;
        let roll_rounded  = (m.roll  * 10.0).round() / 10.0;
        assert!((m.pitch - pitch_rounded).abs() < 1e-9);
        assert!((m.roll  - roll_rounded).abs() < 1e-9);
    }
}
