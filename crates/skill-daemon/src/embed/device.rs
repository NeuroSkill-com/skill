// SPDX-License-Identifier: GPL-3.0-only
//! Runtime RLX device resolution for EXG (EEG/ECG/EMG) inference.

use rlx::Device;

/// Resolve the best available RLX device for EXG inference.
///
/// Respects the user's `Settings::exg_inference_device` preference string:
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
/// - **macOS**: Metal → MLX → CPU
/// - **Linux / Windows**: CUDA → wgpu → CPU
///
/// `is_available` returns `true` only when the backend feature was
/// compiled in AND the hardware/driver probe passes (CUDA/wgpu).
/// Selecting a device that is unavailable silently falls back to CPU.
pub fn resolve_exg_device(pref: &str) -> Device {
    use rlx::runtime::device_ext::is_available;

    match pref {
        "cpu" => return Device::Cpu,
        "metal" => {
            return if is_available(Device::Metal) {
                Device::Metal
            } else {
                Device::Cpu
            }
        }
        "mlx" => {
            return if is_available(Device::Mlx) {
                Device::Mlx
            } else {
                Device::Cpu
            }
        }
        "cuda" => {
            return if is_available(Device::Cuda) {
                Device::Cuda
            } else {
                Device::Cpu
            }
        }
        "gpu" => {
            return if is_available(Device::Gpu) {
                Device::Gpu
            } else {
                Device::Cpu
            }
        }
        "rocm" => {
            return if is_available(Device::Rocm) {
                Device::Rocm
            } else {
                Device::Cpu
            }
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
