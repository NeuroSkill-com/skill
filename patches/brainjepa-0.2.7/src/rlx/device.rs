//! RLX device parsing and actionable errors for Metal / MLX / wgpu / CUDA.
//!
//! `rlx::Session::new` panics when a backend is missing; this module checks
//! availability up front and maps devices to **brainjepa** Cargo features
//! (`rlx-metal`, `rlx-gpu`, …) in error text.

use rlx::Device;

/// Parse a device name (CLI, env). Accepts common aliases (`wgpu` → GPU).
pub fn parse_device(s: &str) -> anyhow::Result<Device> {
    let key = s.trim().to_ascii_lowercase();
    match key.as_str() {
        "cpu" => Ok(Device::Cpu),
        "metal" | "mtl" => Ok(Device::Metal),
        "mlx" => Ok(Device::Mlx),
        "gpu" | "wgpu" => Ok(Device::Gpu),
        "vulkan" | "vk" => Ok(Device::Vulkan),
        "cuda" | "nvidia" => Ok(Device::Cuda),
        "rocm" | "hip" | "amd" => Ok(Device::Rocm),
        "tpu" => Ok(Device::Tpu),
        "" => anyhow::bail!("empty device name (try: cpu, metal, mlx, gpu, cuda)"),
        other => {
            anyhow::bail!("unknown device '{other}' — try: cpu, metal, mlx, gpu, cuda, rocm, tpu")
        }
    }
}

/// Cargo feature set to enable `device` in **this** crate (for error messages).
pub fn recommended_features(device: Device) -> &'static str {
    match device {
        Device::Cpu => "rlx-engine",
        Device::Metal => "rlx-engine,rlx-metal",
        Device::Mlx => "rlx-engine,rlx-mlx",
        Device::Gpu | Device::Vulkan | Device::WebGpu => "rlx-engine,rlx-gpu",
        Device::Cuda => "rlx-engine,rlx-cuda",
        Device::Rocm => "rlx-engine,rlx-rocm",
        Device::Tpu => "rlx-engine,rlx-tpu",
        Device::Ane | Device::OpenGl | Device::DirectX => "rlx-engine",
    }
}

/// @deprecated — use [`recommended_features`]
pub fn brainjepa_features(device: Device) -> &'static str {
    recommended_features(device)
}

/// Whether this **binary** was built with the RLX backend for `device`.
pub fn feature_enabled_in_build(device: Device) -> bool {
    match device {
        Device::Cpu => cfg!(feature = "rlx-cpu"),
        Device::Metal => cfg!(feature = "rlx-metal"),
        Device::Mlx => cfg!(feature = "rlx-mlx"),
        Device::Gpu | Device::Vulkan | Device::WebGpu => cfg!(feature = "rlx-gpu"),
        Device::Cuda => cfg!(feature = "rlx-cuda"),
        Device::Rocm => cfg!(feature = "rlx-rocm"),
        Device::Tpu => cfg!(feature = "rlx-tpu"),
        _ => false,
    }
}

/// RLX runtime probe (driver / adapter present).
pub fn runtime_available(device: Device) -> bool {
    rlx::runtime::is_available(device)
}

/// Devices compiled into this binary that pass the runtime probe.
pub fn available_devices() -> Vec<Device> {
    rlx::runtime::available_devices()
}

fn device_user_name(device: Device) -> &'static str {
    match device {
        Device::Cpu => "cpu",
        Device::Metal => "metal",
        Device::Mlx => "mlx",
        Device::Gpu => "gpu",
        Device::Vulkan => "vulkan",
        Device::Cuda => "cuda",
        Device::Rocm => "rocm",
        Device::Tpu => "tpu",
        _ => "unknown",
    }
}

fn mlx_extra_note() -> Option<&'static str> {
    if cfg!(feature = "rlx-mlx") {
        None
    } else {
        Some(
            "MLX needs rlx with the mlx feature enabled:\n\
               cargo build --release --features rlx-engine,rlx-mlx --bin infer",
        )
    }
}

fn runtime_hint(device: Device) -> &'static str {
    match device {
        Device::Metal => "Requires macOS with Metal support.",
        Device::Mlx => "Requires macOS with MLX (Apple Silicon).",
        Device::Gpu | Device::Vulkan | Device::WebGpu => {
            "Requires a wgpu adapter (Metal on macOS, Vulkan on Linux, DX12 on Windows)."
        }
        Device::Cuda => "Requires an NVIDIA GPU with CUDA drivers installed.",
        Device::Rocm => "Requires an AMD GPU with ROCm installed.",
        Device::Tpu => "Requires a TPU runtime (libtpu / GCP TPU).",
        _ => "Check that the platform driver for this backend is installed.",
    }
}

fn format_available_list(devices: &[Device]) -> String {
    if devices.is_empty() {
        return "  (none — rebuild with e.g. `--features rlx-engine,rlx-metal --bin infer`)".into();
    }
    devices
        .iter()
        .map(|d| device_user_name(*d))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Configure process environment for known backend quirks before compiling.
pub fn prepare_device(device: Device) {
    match device {
        Device::Metal => {
            if std::env::var_os("RLX_METAL_UNFUSE_REGIONS").is_none()
                && std::env::var_os("RLX_METAL_NO_FUSION").is_none()
            {
                unsafe { std::env::set_var("RLX_METAL_UNFUSE_REGIONS", "1") };
            }
            if std::env::var_os("RLX_METAL_SGEMM_MPS").is_none() {
                unsafe { std::env::set_var("RLX_METAL_SGEMM_MPS", "1") };
            }
            // Keep RLX MPSGraph lowering enabled (default). Forcing `RLX_DISABLE_MPSGRAPH=1`
            // routes long-sequence SDPA through MSL thunks and drifts ~3% vs CPU on the
            // JEPA predictor; unset MPSGraph only when benchmarking raw encode latency.
        }
        Device::Mlx => {
            // Prefer compiled MLX graphs when the user has not set a mode.
            if std::env::var_os("RLX_MLX_MODE").is_none() {
                unsafe { std::env::set_var("RLX_MLX_MODE", "compiled") };
            }
        }
        Device::Gpu | Device::Vulkan | Device::WebGpu => {
            // Large ViT attention: keep packed BSHD path enabled (RLX default).
            // Opt out with RLX_WGPU_NO_PACKED_BSHD_ATTN=1 if debugging layout issues.
            let _ = ();
        }
        _ => {}
    }
}

/// Validate before `Session::new` — returns an `anyhow` error instead of panicking.
pub fn ensure_device(device: Device) -> anyhow::Result<()> {
    prepare_device(device);
    let name = device_user_name(device);
    let feats = recommended_features(device);

    if !feature_enabled_in_build(device) {
        let mut msg = format!(
            "RLX device '{name}' is not enabled in this brainjepa build.\n\n\
             Rebuild with:\n\
               cargo build --release --no-default-features --features {feats}\n\n\
             Example infer:\n\
               cargo run --release --no-default-features --features {feats} --bin infer -- \\\n\
                 --device {name} --input <fmri.safetensors>",
        );
        if device == Device::Mlx {
            if let Some(note) = mlx_extra_note() {
                msg.push_str("\n\n");
                msg.push_str(note);
            }
        }
        let avail = available_devices();
        msg.push_str("\n\nBackends that work in this binary right now: ");
        msg.push_str(&format_available_list(&avail));
        return Err(anyhow::anyhow!("{msg}"));
    }

    if !runtime_available(device) {
        let mut msg = format!(
            "RLX device '{name}' is compiled in but not available on this machine.\n\n\
             {}",
            runtime_hint(device),
        );
        if matches!(device, Device::Gpu | Device::Vulkan) {
            msg.push_str(
                "\n\nOn macOS, `--device metal` is usually faster than `--device gpu` (native Metal vs wgpu).",
            );
        }
        let avail = available_devices();
        msg.push_str("\n\nDevices that work here: ");
        msg.push_str(&format_available_list(&avail));
        return Err(anyhow::anyhow!("{msg}"));
    }

    Ok(())
}

/// Pretty label for logs (includes BLAS variant on CPU when known).
pub fn display_name(device: Device) -> String {
    rlx::runtime::full_name(device).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_aliases() {
        assert_eq!(parse_device("wgpu").unwrap(), Device::Gpu);
        assert_eq!(parse_device("MTL").unwrap(), Device::Metal);
        assert_eq!(parse_device("nvidia").unwrap(), Device::Cuda);
    }

    #[test]
    fn cpu_always_ok_in_default_build() {
        ensure_device(Device::Cpu).expect("cpu should be available");
    }
}
