// SPDX-License-Identifier: GPL-3.0-only
//! Runtime system-resource probe — RAM, swap, free disk, CPU count, and power
//! state — so callers can adapt model/runtime config to the actual hardware.
//!
//! Cheap: one `sysinfo` memory refresh + one `statvfs` + a best-effort power read.
//! GPU/unified memory lives in [`crate::GpuStats`] (this complements it with the
//! system-wide signals the LLM runtime adapts on).

use std::path::Path;

use serde::{Deserialize, Serialize};

/// A snapshot of runtime system resources. All byte fields are `None` when the
/// value can't be read on the current platform.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemResources {
    /// Total physical RAM.
    pub total_ram_bytes: Option<u64>,
    /// Available RAM (Activity-Monitor "available" / `MemAvailable`).
    pub free_ram_bytes: Option<u64>,
    /// Total swap configured.
    pub total_swap_bytes: Option<u64>,
    /// Swap currently in use — high values signal real memory pressure even when
    /// "free RAM" looks OK (the OS is already paging).
    pub used_swap_bytes: Option<u64>,
    /// Free bytes on the filesystem holding the probed path (e.g. the model dir).
    pub free_disk_bytes: Option<u64>,
    /// Logical CPU count (respects cgroup/affinity limits).
    pub cpu_count: usize,
    /// Running on battery (best-effort; `None` on desktops / when unknown).
    pub on_battery: Option<bool>,
    /// OS low-power / power-save mode engaged (best-effort; `None` if unknown).
    pub low_power: Option<bool>,
}

impl SystemResources {
    /// Fraction of swap in use (0.0–1.0), if both fields are known.
    pub fn swap_used_fraction(&self) -> Option<f64> {
        match (self.used_swap_bytes, self.total_swap_bytes) {
            (Some(u), Some(t)) if t > 0 => Some(u as f64 / t as f64),
            _ => None,
        }
    }
}

/// Probe runtime system resources. `disk_path` selects the filesystem to report
/// free space for (e.g. the model download dir); `None` skips the disk read.
pub fn system_resources(disk_path: Option<&Path>) -> SystemResources {
    use sysinfo::{MemoryRefreshKind, RefreshKind, System};
    let mut sys = System::new_with_specifics(RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()));
    sys.refresh_memory();

    let nz = |v: u64| (v > 0).then_some(v);
    let (on_battery, low_power) = power_state();
    SystemResources {
        total_ram_bytes: nz(sys.total_memory()),
        free_ram_bytes: nz(sys.available_memory()),
        total_swap_bytes: nz(sys.total_swap()),
        used_swap_bytes: Some(sys.used_swap()),
        free_disk_bytes: disk_path.and_then(statvfs_free_bytes),
        cpu_count: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        on_battery,
        low_power,
    }
}

/// Free bytes available to an unprivileged user on the filesystem holding `path`.
#[cfg(unix)]
fn statvfs_free_bytes(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `libc::statvfs` is a plain-old-data C struct; an all-zero bit
    // pattern is a valid initial value, fully overwritten by the call below on
    // success (return 0).
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid NUL-terminated path; `st` is a zeroed statvfs that
    // the call fully initialises on success (return 0).
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    Some((st.f_bavail as u64).saturating_mul(st.f_frsize as u64))
}

#[cfg(not(unix))]
fn statvfs_free_bytes(_path: &Path) -> Option<u64> {
    None
}

// ── Power state ────────────────────────────────────────────────────────────────

fn power_state() -> (Option<bool>, Option<bool>) {
    #[cfg(target_os = "linux")]
    {
        linux_power_state()
    }
    #[cfg(target_os = "macos")]
    {
        macos_power_state()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        (None, None)
    }
}

/// Linux: `/sys/class/power_supply/*` — a `type == "Mains"` supply with
/// `online == 0` means we're on battery.
#[cfg(target_os = "linux")]
fn linux_power_state() -> (Option<bool>, Option<bool>) {
    let dir = match std::fs::read_dir("/sys/class/power_supply") {
        Ok(d) => d,
        Err(_) => return (None, None),
    };
    let mut on_battery: Option<bool> = None;
    for entry in dir.flatten() {
        let p = entry.path();
        let ty = std::fs::read_to_string(p.join("type")).unwrap_or_default();
        if ty.trim() == "Mains" {
            if let Ok(online) = std::fs::read_to_string(p.join("online")) {
                on_battery = Some(online.trim() != "1");
            }
        }
    }
    (on_battery, None)
}

/// macOS: `IOPSGetProvidingPowerSourceType` returns "AC Power" or "Battery Power".
#[cfg(target_os = "macos")]
fn macos_power_state() -> (Option<bool>, Option<bool>) {
    use std::ffi::c_void;
    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
        fn IOPSGetProvidingPowerSourceType(snapshot: CFTypeRef) -> CFStringRef;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringGetCString(s: CFStringRef, buf: *mut u8, size: isize, encoding: u32) -> bool;
        fn CFRelease(cf: CFTypeRef);
    }
    const UTF8: u32 = 0x0800_0100;
    // SAFETY: IOPS/CF calls follow the documented ownership rules — we own the
    // `snapshot` from IOPSCopyPowerSourcesInfo and release it; the CFStringRef
    // from IOPSGetProvidingPowerSourceType is a "Get" (borrowed, not released).
    unsafe {
        let snap = IOPSCopyPowerSourcesInfo();
        if snap.is_null() {
            return (None, None);
        }
        let kind = IOPSGetProvidingPowerSourceType(snap);
        let mut buf = [0u8; 64];
        let got = !kind.is_null() && CFStringGetCString(kind, buf.as_mut_ptr(), buf.len() as isize, UTF8);
        CFRelease(snap);
        if !got {
            return (None, None);
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        let s = String::from_utf8_lossy(&buf[..end]);
        // "Battery Power" → on battery; "AC Power" → not.
        (Some(s.starts_with("Battery")), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_does_not_panic_and_reports_cpus() {
        let r = system_resources(Some(std::path::Path::new("/")));
        assert!(r.cpu_count >= 1);
        // On any real host RAM should read; disk of "/" should be Some on unix.
        #[cfg(unix)]
        assert!(r.free_disk_bytes.is_some(), "statvfs('/') should report free bytes");
    }

    #[test]
    fn swap_fraction_math() {
        let r = SystemResources {
            used_swap_bytes: Some(3),
            total_swap_bytes: Some(4),
            ..Default::default()
        };
        assert_eq!(r.swap_used_fraction(), Some(0.75));
        assert_eq!(SystemResources::default().swap_used_fraction(), None);
    }
}
