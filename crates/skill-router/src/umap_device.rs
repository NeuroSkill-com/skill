// SPDX-License-Identifier: GPL-3.0-only
//! Runtime RLX device resolution for UMAP projection.

use rlx_runtime::device_ext::is_available;
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

/// Resolve the best available RLX device for UMAP projection.
///
/// Respects the user's `UmapUserConfig::backend` preference string:
///
/// | value    | behaviour                                      |
/// |----------|------------------------------------------------|
/// | `"auto"` | platform default (see below), CPU fallback     |
/// | `"cpu"`  | always CPU                                     |
/// | `"metal"`| Apple Metal; CPU if unavailable                |
/// | `"mlx"`  | Apple MLX;   CPU if unavailable                |
/// | `"cuda"` | NVIDIA CUDA; CPU if unavailable                |
/// | `"gpu"`  | wgpu;        CPU if unavailable                |
/// | `"rocm"` | AMD ROCm;    CPU if unavailable                |
///
/// Platform defaults for `"auto"`:
/// - **macOS**: Metal → MLX → wgpu → CPU
/// - **Linux / Windows**: CUDA → wgpu → CPU
pub fn resolve_umap_device(pref: &str) -> Device {
    match pref {
        "cpu" => return Device::Cpu,
        "metal" => {
            return if is_available(Device::Metal) {
                Device::Metal
            } else {
                Device::Cpu
            };
        }
        "mlx" => {
            return if is_available(Device::Mlx) {
                Device::Mlx
            } else {
                Device::Cpu
            };
        }
        "cuda" => {
            return if is_available(Device::Cuda) {
                Device::Cuda
            } else {
                Device::Cpu
            };
        }
        "gpu" => {
            return if is_available(Device::Gpu) {
                Device::Gpu
            } else {
                Device::Cpu
            };
        }
        "rocm" => {
            return if is_available(Device::Rocm) {
                Device::Rocm
            } else {
                Device::Cpu
            };
        }
        _ => {} // "auto" or unknown — fall through to platform defaults
    }

    #[cfg(target_os = "macos")]
    {
        if is_available(Device::Metal) {
            return Device::Metal;
        }
        if is_available(Device::Mlx) {
            return Device::Mlx;
        }
        if is_available(Device::Gpu) {
            return Device::Gpu;
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        if is_available(Device::Cuda) {
            return Device::Cuda;
        }
        if is_available(Device::Gpu) {
            return Device::Gpu;
        }
    }

    Device::Cpu
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
        if is_available(device) {
            v.push(label);
        }
    }
    v.push("cpu");
    v
}
