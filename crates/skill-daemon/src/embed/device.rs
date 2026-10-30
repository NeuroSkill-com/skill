// SPDX-License-Identifier: GPL-3.0-only
//! Runtime RLX device resolution for EXG (EEG/ECG/EMG) inference.

use rlx::Device;

/// Resolve the device to *try first* for EXG inference from the user's
/// `Settings::exg_inference_device` (`auto`, `metal`, `gpu`, `wgpu`, `cuda`,
/// `rocm`, `mlx`, `cpu`). Policy lives in [`skill_accel`]: the requested
/// device if usable, else the platform chain — macOS **Metal → wgpu → CPU**,
/// elsewhere **CUDA → wgpu → CPU**. On a Mac `gpu` and `mlx` mean Metal first.
/// Loading falls further down the same chain on failure (`load_encoder`).
pub fn resolve_exg_device(pref: &str) -> Device {
    skill_accel::best(skill_accel::parse_device(pref))
}

/// The setting string that makes [`resolve_exg_device`] pick exactly `d`
/// (`wgpu` rather than `gpu`, which means "best GPU" on a Mac).
pub fn device_setting(d: Device) -> &'static str {
    match d {
        Device::Gpu => "wgpu",
        other => other.as_arg(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rlx::runtime::device_ext::{feature_compiled, is_available};

    /// The macOS product has no MLX: a saved "mlx" or "gpu" must land on
    /// Metal, not drop to CPU (as "gpu" used to) or reach MLX.
    #[test]
    #[cfg(target_os = "macos")]
    fn mlx_and_gpu_choices_run_on_metal_on_a_mac() {
        if !is_available(Device::Metal) {
            return; // headless CI Mac without a GPU
        }
        assert!(
            !feature_compiled(Device::Mlx),
            "MLX must not be compiled into the macOS daemon"
        );
        assert_eq!(resolve_exg_device("mlx"), Device::Metal);
        assert_eq!(resolve_exg_device("auto"), Device::Metal);
        assert_eq!(resolve_exg_device("gpu"), Device::Metal);
        assert_eq!(resolve_exg_device("cpu"), Device::Cpu);
    }

    /// Every device in the chain can be requested exactly, so the loader can
    /// walk the chain through the setting-string API.
    #[test]
    fn device_setting_round_trips() {
        for d in skill_accel::fallback_chain(Device::Metal) {
            assert_eq!(resolve_exg_device(device_setting(d)), d, "{d:?}");
        }
    }
}
