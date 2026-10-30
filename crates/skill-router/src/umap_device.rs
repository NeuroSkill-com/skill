// SPDX-License-Identifier: GPL-3.0-only
//! Runtime RLX device resolution for UMAP projection.

use rlx_umap::Device;

/// Short backend label stored in UMAP cache JSON and logs.
pub fn device_label(device: Device) -> &'static str {
    match device {
        Device::Metal => "metal",
        Device::Mlx => "mlx",
        Device::Cuda => "cuda",
        Device::Gpu => "gpu",
        Device::Rocm => "rocm",
        Device::Cpu => "cpu",
        _ => "other",
    }
}

/// Resolve the device to try first for UMAP from `UmapUserConfig::backend`
/// (`auto`, `metal`, `gpu`, `wgpu`, `cuda`, `rocm`, `mlx`, `cpu`). Policy is
/// shared (`skill_accel`): requested device if usable, else macOS
/// Metal → wgpu → CPU, elsewhere CUDA → wgpu → CPU. The fit walks the rest of
/// the chain if a device fails (`skill_router::run_umap`).
pub fn resolve_umap_device(pref: &str) -> Device {
    skill_accel::best(skill_accel::parse_device(pref))
}

/// Backends compiled into this build that pass a runtime availability probe.
pub fn available_backends() -> Vec<&'static str> {
    let mut v = Vec::new();
    for (device, label) in [
        (Device::Metal, "metal"),
        (Device::Mlx, "mlx"),
        (Device::Cuda, "cuda"),
        (Device::Gpu, "gpu"),
        (Device::Rocm, "rocm"),
    ] {
        if skill_accel::usable(device) {
            v.push(label);
        }
    }
    v.push("cpu");
    v
}
