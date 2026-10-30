// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! One device-fallback policy for every model the daemon loads (LLM, TTS,
//! speech recognition, text / EXG / screenshot embeddings, OCR, UMAP).
//!
//! Order: the requested device first, then the platform chain —
//! **macOS: Metal → wgpu → CPU**, elsewhere **CUDA → wgpu → CPU** — skipping
//! anything not compiled in or not present. A backend that fails to load or
//! *panics* while loading moves on to the next one instead of breaking the
//! feature (a panic crossing rlx FFI would otherwise abort the daemon).
//!
//! MLX is never probed unless it is compiled in: its availability check runs a
//! kernel, and without MLX's Metal library that traps in C++ and kills the
//! process (0.1.0-rc.8). macOS product builds don't include MLX.

use rlx_runtime::device_ext::{feature_compiled, is_available};
pub use rlx_runtime::Device;

/// Whether `d` can run here. Never runs MLX's probe unless MLX is compiled in.
pub fn usable(d: Device) -> bool {
    if d == Device::Cpu {
        return true;
    }
    if d == Device::Mlx && !feature_compiled(Device::Mlx) {
        return false;
    }
    is_available(d)
}

/// GPU backends to try after the requested one, best first, for this platform.
fn platform_order() -> &'static [Device] {
    if cfg!(target_os = "macos") {
        &[Device::Metal, Device::Gpu]
    } else {
        &[Device::Cuda, Device::Rocm, Device::Gpu]
    }
}

/// Devices to try, in order, for a model that asked for `preferred`: it, then
/// the platform GPUs, then CPU — each usable here, no duplicates. Asking for
/// CPU means CPU only.
pub fn fallback_chain(preferred: Device) -> Vec<Device> {
    fallback_chain_excluding(preferred, &[])
}

/// [`fallback_chain`] without the devices in `skip` (CPU is never skipped).
/// For models known to misbehave on a backend.
pub fn fallback_chain_excluding(preferred: Device, skip: &[Device]) -> Vec<Device> {
    if preferred == Device::Cpu {
        return vec![Device::Cpu];
    }
    let mut out = Vec::new();
    for d in std::iter::once(preferred).chain(platform_order().iter().copied()) {
        if !out.contains(&d) && !skip.contains(&d) && usable(d) {
            out.push(d);
        }
    }
    out.push(Device::Cpu);
    out
}

/// Parse a device setting (`auto`, `metal`, `gpu`, `wgpu`, `vulkan`, `cuda`,
/// `rocm`, `mlx`, `ane`, `cpu`, …) into the device to try first. `auto` and
/// unknown values mean the platform's best GPU. On macOS, `gpu` and `mlx`
/// mean Metal first (wgpu follows in the chain anyway).
pub fn parse_device(setting: &str) -> Device {
    let s = setting.trim().to_ascii_lowercase();
    let mac = cfg!(target_os = "macos");
    let parsed = match s.as_str() {
        "cpu" => Some(Device::Cpu),
        "metal" => Some(Device::Metal),
        "gpu" if mac => Some(Device::Metal),
        "gpu" | "wgpu" | "vulkan" => Some(Device::Gpu),
        "mlx" if mac && !feature_compiled(Device::Mlx) => Some(Device::Metal),
        other => other.parse::<Device>().ok(),
    };
    parsed.unwrap_or_else(|| platform_order()[0])
}

/// The device to move to when `current` stops working at run time: the next
/// usable one *after* it in the platform order (macOS Metal → wgpu → CPU),
/// never one before it. `None` once on CPU.
pub fn next_after(current: Device) -> Option<Device> {
    if current == Device::Cpu {
        return None;
    }
    let order: Vec<Device> = platform_order()
        .iter()
        .copied()
        .filter(|d| usable(*d))
        .chain(std::iter::once(Device::Cpu))
        .collect();
    match order.iter().position(|d| *d == current) {
        Some(i) => order.get(i + 1).copied(),
        // Not in the platform order (e.g. ANE): go to the best platform device.
        None => order.first().copied(),
    }
}

/// The first device in `fallback_chain(preferred)` — for code that only needs
/// to pick a device, not load on it.
pub fn best(preferred: Device) -> Device {
    fallback_chain(preferred)[0]
}

/// Try `load` on each device of [`fallback_chain_excluding`]`(preferred, skip)`
/// until one succeeds; a panic counts as a failure. Returns the value and the
/// device it loaded on, or every device's error if all failed. Each failure
/// is logged with `what` so a fallback is never silent.
pub fn load_with_fallback<T>(
    what: &str,
    preferred: Device,
    skip: &[Device],
    mut load: impl FnMut(Device) -> anyhow::Result<T>,
) -> anyhow::Result<(T, Device)> {
    let chain = fallback_chain_excluding(preferred, skip);
    let mut errors = Vec::new();
    for (i, &d) in chain.iter().enumerate() {
        if simulated_failure(d) {
            eprintln!(
                "[accel] {what}: {} failed: simulated (SKILL_ACCEL_SIMULATE_FAIL)",
                d.as_arg()
            );
            errors.push(format!("{}: simulated failure", d.as_arg()));
            continue;
        }
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| load(d)))
            .unwrap_or_else(|p| Err(anyhow::anyhow!("panicked: {}", panic_msg(&p))));
        match r {
            Ok(v) => {
                if i > 0 {
                    eprintln!("[accel] {what}: running on {} after: {}", d.as_arg(), errors.join("; "));
                }
                return Ok((v, d));
            }
            Err(e) => {
                eprintln!("[accel] {what}: {} failed: {e:#}", d.as_arg());
                errors.push(format!("{}: {e:#}", d.as_arg()));
            }
        }
    }
    Err(anyhow::anyhow!("{what}: no device worked ({})", errors.join("; ")))
}

/// Diagnostics: `SKILL_ACCEL_SIMULATE_FAIL=metal,gpu` makes loads on those
/// devices fail as if the backend broke, to exercise the fallback end to end.
fn simulated_failure(d: Device) -> bool {
    std::env::var("SKILL_ACCEL_SIMULATE_FAIL")
        .map(|v| v.split(',').any(|x| x.trim().eq_ignore_ascii_case(d.as_arg())))
        .unwrap_or(false)
}

fn panic_msg(p: &Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<String>()
        .cloned()
        .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown panic".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_ends_in_cpu_without_duplicates_or_mlx() {
        for pref in [Device::Metal, Device::Gpu, Device::Cuda, Device::Mlx] {
            let c = fallback_chain(pref);
            assert_eq!(*c.last().unwrap(), Device::Cpu, "{pref:?}: {c:?}");
            let mut dedup = c.clone();
            dedup.dedup();
            assert_eq!(dedup.len(), c.len());
            if !feature_compiled(Device::Mlx) {
                assert!(!c.contains(&Device::Mlx), "{c:?}");
            }
        }
        assert_eq!(fallback_chain(Device::Cpu), [Device::Cpu]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_order_is_metal_then_wgpu_then_cpu() {
        let c = fallback_chain(Device::Metal);
        let want: Vec<Device> = [Device::Metal, Device::Gpu, Device::Cpu]
            .into_iter()
            .filter(|d| usable(*d))
            .collect();
        assert_eq!(c, want);
        assert_eq!(parse_device("gpu"), Device::Metal);
        assert_eq!(parse_device("mlx"), Device::Metal);
        assert_eq!(parse_device("auto"), Device::Metal);
        assert_eq!(parse_device("wgpu"), Device::Gpu);
        assert_eq!(parse_device("cpu"), Device::Cpu);
    }

    #[test]
    fn falls_through_errors_and_panics_to_the_next_device() {
        let mut tried = Vec::new();
        let (v, d) = load_with_fallback("test", Device::Metal, &[], |d| {
            tried.push(d);
            match d {
                Device::Metal => panic!("metal kernel exploded"),
                Device::Gpu => anyhow::bail!("wgpu adapter lost"),
                _ => Ok(42),
            }
        })
        .unwrap();
        assert_eq!((v, d), (42, Device::Cpu));
        assert_eq!(*tried.last().unwrap(), Device::Cpu);
    }

    #[test]
    fn next_after_only_moves_forward() {
        assert_eq!(next_after(Device::Cpu), None);
        if cfg!(target_os = "macos") && usable(Device::Metal) {
            let after_metal = next_after(Device::Metal).unwrap();
            assert_ne!(after_metal, Device::Metal);
            if usable(Device::Gpu) {
                assert_eq!(after_metal, Device::Gpu);
                assert_eq!(
                    next_after(Device::Gpu),
                    Some(Device::Cpu),
                    "wgpu must not go back to Metal"
                );
            }
        }
    }

    #[test]
    fn skip_removes_a_backend_but_never_cpu() {
        let c = fallback_chain_excluding(Device::Metal, &[Device::Gpu, Device::Cpu]);
        assert!(!c.contains(&Device::Gpu));
        assert_eq!(*c.last().unwrap(), Device::Cpu);
    }
}
