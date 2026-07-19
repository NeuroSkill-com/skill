//! RLX device parsing and aliases (`mps` → Metal, `wgpu` → Gpu, …).

use crate::error::{EegDinoError, Result};

/// Parse a CLI / env device string into [`rlx::Device`].
///
/// Accepted names (case-insensitive):
///
/// | Input | Device |
/// |-------|--------|
/// | `cpu`, `ndarray` | `Cpu` |
/// | `metal`, `mps`, `apple` | `Metal` |
/// | `mlx` | `Mlx` |
/// | `gpu`, `wgpu`, `vulkan`, `webgpu` | `Gpu` |
/// | `cuda`, `nvidia` | `Cuda` |
/// | `rocm`, `hip` | `Rocm` |
/// | `tpu` | `Tpu` |
pub fn parse_device(s: &str) -> Result<rlx::Device> {
    match s.trim().to_lowercase().as_str() {
        "cpu" | "ndarray" => Ok(rlx::Device::Cpu),

        "metal" | "mps" | "apple" => Ok(rlx::Device::Metal),
        "mlx" => Ok(rlx::Device::Mlx),

        "gpu" | "wgpu" | "vulkan" | "webgpu" | "dx12" => Ok(rlx::Device::Gpu),

        "cuda" | "nvidia" | "cudnn" => Ok(rlx::Device::Cuda),
        "rocm" | "hip" | "amd" => Ok(rlx::Device::Rocm),
        "tpu" => Ok(rlx::Device::Tpu),

        other => Err(EegDinoError::InvalidInput(format!(
            "unknown RLX device {other:?} — expected cpu|metal|mps|mlx|gpu|wgpu|cuda|rocm|tpu"
        ))),
    }
}

/// Whether this device can run on the current host (feature + hardware).
pub fn is_device_available(device: rlx::Device) -> bool {
    rlx::runtime::is_available(device)
}

/// Human-readable label for benchmark tables.
pub fn device_label(device: rlx::Device) -> &'static str {
    match device {
        rlx::Device::Cpu => "CPU",
        rlx::Device::Metal => "Metal (MPS)",
        rlx::Device::Mlx => "MLX",
        rlx::Device::Gpu => "wgpu",
        rlx::Device::Cuda => "CUDA",
        rlx::Device::Rocm => "ROCm",
        rlx::Device::Tpu => "TPU",
        rlx::Device::Ane => "ANE",
        rlx::Device::Vulkan => "Vulkan",
        rlx::Device::OpenGl => "OpenGL",
        rlx::Device::DirectX => "DirectX",
        rlx::Device::WebGpu => "WebGPU",
        _ => "unknown",
    }
}

/// Cargo feature flag needed to compile a given device (always includes `rlx-cpu`).
pub fn feature_for(device: rlx::Device) -> &'static str {
    match device {
        rlx::Device::Cpu => "rlx-cpu",
        rlx::Device::Metal => "rlx-metal",
        rlx::Device::Mlx => "rlx-mlx",
        rlx::Device::Gpu => "rlx-gpu",
        rlx::Device::Cuda => "rlx-cuda",
        rlx::Device::Rocm => "rlx-rocm",
        rlx::Device::Tpu => "rlx-tpu",
        _ => "rlx-cpu",
    }
}
