// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! GPU utilisation and memory reading — cross-platform.
//!
//! ## macOS — Apple Silicon (unified memory)
//!
//! The GPU shares the same physical memory pool as the CPU.  There is no
//! separate VRAM bank.  We report:
//! * `is_unified_memory = true`
//! * `total_memory_bytes` — `sysctl hw.memsize` (total physical RAM)
//! * `free_memory_bytes`  — Mach `host_statistics64(HOST_VM_INFO64)`:
//!   `(free_count + inactive_count) * vm_page_size`
//!
//! ## macOS — Intel + discrete AMD / Nvidia GPU
//!
//! We iterate `IOAccelerator` services and read `PerformanceStatistics`:
//! * `vramFreeBytes`        — AMD / some Nvidia
//! * `vramUsedBytesCurrent` — AMD
//! * `VRAM,totalMB`         — fallback from the parent PCI device
//!
//! Utilisation is polled at 100 ms and smoothed with an EWMA (τ = 500 ms)
//! on a dedicated background thread so callers pay no IOKit cost.
//!
//! ## Linux / Windows — via `llmfit-core`
//!
//! [`llmfit_core::hardware::SystemSpecs::detect`] covers:
//! * NVIDIA via `nvidia-smi` (with sysfs fallback for containerised setups)
//! * AMD via `rocm-smi` (with sysfs fallback for non-ROCm systems)
//! * Intel Arc via sysfs / lspci
//! * Apple Silicon via `system_profiler` (same binary also runs on macOS,
//!   but the IOKit path above is used there instead)
//! * Windows GPUs via PowerShell WMI / wmic
//! * Unified-memory APUs (AMD Ryzen AI) and SoCs (NVIDIA Grace Blackwell)
//!
//! GPU utilisation is **not** available on Linux/Windows (no equivalent of
//! IOKit's `PerformanceStatistics`); `render`, `tiler`, and `overall` are
//! always 0.0 on those platforms.
//!
//! `free_memory_bytes` is refreshed every 5 s on unified-memory platforms
//! (where it equals available system RAM) by a lightweight `sysinfo` poll.
//! For discrete GPUs on Linux/Windows it is `None` — live VRAM-free tracking
//! would require NVML/NVAPI/ADL, which are not in scope here.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GpuStats {
    /// Render engine utilisation, 0.0–1.0 (Apple Silicon `Renderer Utilization %`).
    /// Always 0.0 on Linux / Windows (IOKit not available).
    pub render:  f32,
    /// Tiler / geometry engine utilisation, 0.0–1.0 (Apple Silicon `Tiler Utilization %`).
    /// Always 0.0 on Linux / Windows.
    pub tiler:   f32,
    /// Best-effort overall utilisation, 0.0–1.0.
    /// Always 0.0 on Linux / Windows.
    pub overall: f32,

    /// `true` on Apple Silicon and other unified-memory architectures (AMD
    /// Ryzen AI APUs, NVIDIA Grace Blackwell) where GPU and CPU share a single
    /// physical memory pool.
    pub is_unified_memory: bool,

    /// Total GPU-accessible memory in bytes.
    /// * Unified memory: total physical RAM.
    /// * Discrete GPU: total VRAM.
    ///
    /// `None` if the value could not be read.
    pub total_memory_bytes: Option<u64>,

    /// Free / available GPU memory in bytes.
    /// * macOS unified: `(free + inactive)` pages × page size.
    /// * macOS discrete: `vramFreeBytes` from IOAccelerator.
    /// * Linux/Windows unified: available system RAM (refreshed every 5 s).
    /// * Linux/Windows discrete: `None` (NVML/ADL not in scope).
    ///
    /// `None` if the value could not be read.
    pub free_memory_bytes: Option<u64>,
}

/// Return smoothed GPU statistics.
///
/// On macOS the sampler polls IOKit every 100 ms and applies an EWMA so
/// brief render bursts are visible.  The first call initialises the poller
/// thread; subsequent calls just clone from a shared cache — no IOKit work
/// on the caller's thread.
///
/// On Linux / Windows a one-shot `llmfit-core` detection run is cached
/// (GPU info is static) while free memory for unified-memory platforms is
/// refreshed every 5 s from `sysinfo`.
///
/// Returns `None` if no GPU could be detected on the current platform.
pub fn read() -> Option<GpuStats> {
    #[cfg(target_os = "macos")]
    return macos::cached_read();

    #[cfg(not(target_os = "macos"))]
    return non_macos::read();
}

// ── macOS implementation ──────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{c_char, c_void, CString};

    // ── Type aliases matching IOKit / CoreFoundation C headers ─────────────
    type IORegistryEntryT  = u32;
    type IOIteratorT       = u32;
    type KernReturnT       = i32;
    type CFTypeRef         = *const c_void;
    type CFDictionaryRef   = *const c_void;
    type CFMutableDictRef  = *mut c_void;
    type CFStringRef       = *const c_void;
    type CFAllocatorRef    = *const c_void;
    type CFNumberType      = i32;

    const KERN_SUCCESS:              KernReturnT  = 0;
    const K_IO_MASTER_PORT_DEFAULT:  u32          = 0;
    const CF_NUMBER_SI32_TYPE:       CFNumberType = 3;   // kCFNumberSInt32Type
    const CF_NUMBER_SI64_TYPE:       CFNumberType = 4;   // kCFNumberSInt64Type
    const CF_NUMBER_FLOAT64_TYPE:    CFNumberType = 6;   // kCFNumberFloat64Type
    const K_CF_STRING_ENCODING_UTF8: u32          = 0x0800_0100;

    // ── IOKit framework ───────────────────────────────────────────────────
    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOServiceMatching(name: *const c_char) -> CFMutableDictRef;
        fn IOServiceGetMatchingServices(
            master_port: u32,
            matching:    CFMutableDictRef,
            existing:    *mut IOIteratorT,
        ) -> KernReturnT;
        fn IOIteratorNext(iterator: IOIteratorT) -> IORegistryEntryT;
        fn IOObjectRelease(object: u32) -> KernReturnT;
        fn IORegistryEntryCreateCFProperties(
            entry:       IORegistryEntryT,
            properties:  *mut CFMutableDictRef,
            allocator:   CFAllocatorRef,
            options:     u32,
        ) -> KernReturnT;
    }

    // ── CoreFoundation framework ──────────────────────────────────────────
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFDictionaryGetValue(dict: CFDictionaryRef, key: CFStringRef) -> CFTypeRef;
        fn CFStringCreateWithCString(
            alloc:    CFAllocatorRef,
            c_str:    *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFNumberGetValue(number: CFTypeRef, the_type: CFNumberType, value_ptr: *mut i64) -> bool;
        fn CFRelease(cf: CFTypeRef);
    }

    // ── Mach (for unified-memory free page count) ─────────────────────────
    // These types and constants come from <mach/mach.h>.
    type MachPortT   = u32;
    type KernReturn  = i32;
    type HostFlavorT = i32;
    type MachMsgTypeNumberT = u32;

    const HOST_VM_INFO64:        HostFlavorT = 4;
    // sizeof(vm_statistics64_data_t) / sizeof(integer_t)
    // = 160 bytes / 4 bytes = 40
    const HOST_VM_INFO64_COUNT:  u32         = 40;

    // vm_statistics64_data_t — exact layout from <mach/vm_statistics.h>.
    //
    // The first four page counters and several later ones are `uint32_t`
    // (not uint64_t).  Getting this wrong produces garbage values that
    // overflow when multiplied by the page size.
    //
    //  u32: free_count, active_count, inactive_count, wire_count
    //  u64: zero_fill_count … hits, purges
    //  u32: purgeable_count, speculative_count
    //  u64: decompressions … swapouts
    //  u32: compressor_page_count, throttled_count,
    //       external_page_count, internal_page_count
    //  u64: total_uncompressed_pages_in_compressor
    #[repr(C)]
    struct VmStatistics64 {
        free_count:             u32,   // uint32_t
        active_count:           u32,   // uint32_t
        inactive_count:         u32,   // uint32_t
        wire_count:             u32,   // uint32_t
        zero_fill_count:        u64,
        reactivations:          u64,
        pageins:                u64,
        pageouts:               u64,
        faults:                 u64,
        cow_faults:             u64,
        lookups:                u64,
        hits:                   u64,
        purges:                 u64,
        purgeable_count:        u32,   // uint32_t
        speculative_count:      u32,   // uint32_t
        decompressions:         u64,
        compressions:           u64,
        swapins:                u64,
        swapouts:               u64,
        compressor_page_count:  u32,   // uint32_t
        throttled_count:        u32,   // uint32_t
        external_page_count:    u32,   // uint32_t
        internal_page_count:    u32,   // uint32_t
        total_uncompressed_pages_in_compressor: u64,
    }

    #[link(name = "System", kind = "dylib")]
    extern "C" {
        fn mach_host_self() -> MachPortT;
        fn host_statistics64(
            host:         MachPortT,
            flavor:       HostFlavorT,
            host_info_out: *mut VmStatistics64,
            host_info_outCnt: *mut MachMsgTypeNumberT,
        ) -> KernReturn;
    }

    // ── sysctl helpers ────────────────────────────────────────────────────

    /// Read a `u64` value from `sysctlbyname`.
    fn sysctl_u64(name: &str) -> Option<u64> {
        let c_name = CString::new(name).ok()?;
        let mut val: u64 = 0;
        let mut size = std::mem::size_of::<u64>();
        let ret = unsafe {
            libc::sysctlbyname(
                c_name.as_ptr(),
                &mut val as *mut u64 as *mut _,
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if ret == 0 { Some(val) } else { None }
    }

    /// Read a `i32` value from `sysctlbyname`.
    fn sysctl_i32(name: &str) -> Option<i32> {
        let c_name = CString::new(name).ok()?;
        let mut val: i32 = 0;
        let mut size = std::mem::size_of::<i32>();
        let ret = unsafe {
            libc::sysctlbyname(
                c_name.as_ptr(),
                &mut val as *mut i32 as *mut _,
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if ret == 0 { Some(val) } else { None }
    }

    /// `true` when running natively on Apple Silicon (arm64).
    fn is_apple_silicon() -> bool {
        sysctl_i32("hw.optional.arm64") == Some(1)
    }

    /// Total physical RAM in bytes (`hw.memsize`).
    fn system_ram_bytes() -> Option<u64> {
        sysctl_u64("hw.memsize")
    }

    /// `(free + inactive)` pages × `vm_page_size`, giving a practical "available"
    /// figure that matches Activity Monitor's "Memory Available" metric.
    fn unified_free_bytes() -> Option<u64> {
        let page_size = sysctl_u64("hw.pagesize").unwrap_or(16_384);
        let mut stats = std::mem::MaybeUninit::<VmStatistics64>::zeroed();
        let mut count: MachMsgTypeNumberT = HOST_VM_INFO64_COUNT;
        let kr = unsafe {
            host_statistics64(
                mach_host_self(),
                HOST_VM_INFO64,
                stats.as_mut_ptr(),
                &mut count,
            )
        };
        if kr != 0 { return None; }
        let s = unsafe { stats.assume_init() };
        let available_pages = s.free_count as u64 + s.inactive_count as u64;
        Some(available_pages.saturating_mul(page_size))
    }

    // ── CoreFoundation helpers ────────────────────────────────────────────

    fn with_cf_str<T>(s: &str, f: impl FnOnce(CFStringRef) -> T) -> T {
        let c = CString::new(s).unwrap_or_default();
        let cf = unsafe {
            CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), K_CF_STRING_ENCODING_UTF8)
        };
        let result = f(cf);
        if !cf.is_null() { unsafe { CFRelease(cf as _) }; }
        result
    }

    fn dict_i64(dict: CFDictionaryRef, key: &str) -> Option<i64> {
        with_cf_str(key, |k| {
            let v = unsafe { CFDictionaryGetValue(dict, k) };
            if v.is_null() { return None; }
            let mut out: i64 = 0;
            if unsafe { CFNumberGetValue(v, CF_NUMBER_SI64_TYPE, &mut out) } {
                Some(out)
            } else {
                // Fall back to 32-bit read
                let mut out32: i64 = 0;
                if unsafe { CFNumberGetValue(v, CF_NUMBER_SI32_TYPE, &mut out32) } {
                    Some(out32)
                } else {
                    None
                }
            }
        })
    }

    /// Read a CFNumber as a 64-bit float.
    ///
    /// CFNumberGetValue's third argument is declared `void *` in C; we pass a
    /// `*mut f64` cast to `*mut i64` (both are 8-byte pointers — the cast is
    /// safe because we immediately re-interpret through the float pointer).
    fn dict_f64(dict: CFDictionaryRef, key: &str) -> Option<f64> {
        with_cf_str(key, |k| {
            let v = unsafe { CFDictionaryGetValue(dict, k) };
            if v.is_null() { return None; }
            let mut out: f64 = 0.0;
            // kCFNumberFloat64Type converts from any stored numeric type.
            if unsafe {
                CFNumberGetValue(
                    v,
                    CF_NUMBER_FLOAT64_TYPE,
                    &mut out as *mut f64 as *mut i64,
                )
            } {
                Some(out)
            } else {
                None
            }
        })
    }

    /// Read a GPU utilisation percentage stored as **either**:
    ///
    /// * An integer 0–100  (`kCFNumberSInt32Type` / `kCFNumberSInt64Type`) —
    ///   the format used by all known Apple Silicon (AGX) and Intel drivers.
    ///   Matches the `as? Int` pattern used by the Stats app, which is the
    ///   reference implementation proven to work on macOS 13–15.
    /// * A float 0.0–100.0 or 0.0–1.0 (`kCFNumberFloat64Type`) —
    ///   fallback for discrete AMD/Nvidia drivers that store float values.
    ///
    /// Returns a normalised 0.0–1.0 fraction, or `None` if the key is absent.
    fn dict_pct(dict: CFDictionaryRef, key: &str) -> Option<f32> {
        // Read as integer first — this matches exactly what the Stats app does
        // (`stats["key"] as? Int`) and is the correct format for AGX / Intel.
        if let Some(i) = dict_i64(dict, key) {
            return Some(i.clamp(0, 100) as f32 / 100.0);
        }
        // Fallback: float (some discrete AMD/Nvidia drivers).
        // Values > 1.0 are already percentage-style (0–100); ≤ 1.0 are fractional.
        dict_f64(dict, key).map(|f| if f > 1.0 { (f / 100.0) as f32 } else { f as f32 })
    }

    // ── IOAccelerator loop ────────────────────────────────────────────────

    struct AcceleratorInfo {
        render:  f32,
        tiler:   f32,
        overall: f32,
        vram_free_bytes: Option<u64>,
        vram_used_bytes: Option<u64>,
    }

    fn read_accelerators() -> Vec<AcceleratorInfo> {
        // Keep `name` alive for the entire call — IOServiceMatching reads the
        // C string immediately, but the pointer must remain valid until the
        // function returns.  Using a match-arm temporary drops it before the
        // outer call, causing a use-after-free and an empty service list.
        let name = match CString::new("IOAccelerator") {
            Ok(s)  => s,
            Err(_) => return vec![],
        };
        let matching = unsafe { IOServiceMatching(name.as_ptr()) };
        if matching.is_null() { return vec![]; }

        let mut iter: IOIteratorT = 0;
        if unsafe { IOServiceGetMatchingServices(K_IO_MASTER_PORT_DEFAULT, matching, &mut iter) }
            != KERN_SUCCESS
        {
            return vec![];
        }

        let mut results = Vec::new();

        loop {
            let entry = unsafe { IOIteratorNext(iter) };
            if entry == 0 { break; }

            let mut props: CFMutableDictRef = std::ptr::null_mut();
            let kr = unsafe {
                IORegistryEntryCreateCFProperties(entry, &mut props, std::ptr::null(), 0)
            };

            if kr == KERN_SUCCESS && !props.is_null() {
                let perf = with_cf_str("PerformanceStatistics", |k| unsafe {
                    CFDictionaryGetValue(props as CFDictionaryRef, k)
                });

                if !perf.is_null() {
                    let d = perf as CFDictionaryRef;

                    // dict_pct handles both the pre-Ventura integer (0–100)
                    // and the post-Ventura float (0.0–1.0) storage formats.
                    let render_f  = dict_pct(d, "Renderer Utilization %").unwrap_or(0.0);
                    let tiler_f   = dict_pct(d, "Tiler Utilization %").unwrap_or(0.0);
                    let overall_f = dict_pct(d, "Device Utilization %")
                        .or_else(|| dict_pct(d, "GPU Activity(%)"))
                        .unwrap_or_else(|| (render_f + tiler_f) / 2.0);

                    let vram_free = dict_i64(d, "vramFreeBytes")
                        .map(|v| v.max(0) as u64);
                    let vram_used = dict_i64(d, "vramUsedBytesCurrent")
                        .map(|v| v.max(0) as u64);

                    results.push(AcceleratorInfo {
                        render:  render_f,
                        tiler:   tiler_f,
                        overall: overall_f,
                        vram_free_bytes: vram_free,
                        vram_used_bytes: vram_used,
                    });
                }

                unsafe { CFRelease(props as CFTypeRef) };
            }

            unsafe { IOObjectRelease(entry) };
        }

        unsafe { IOObjectRelease(iter) };
        results
    }

    // ── Background sampler + EWMA cache ──────────────────────────────────

    use std::sync::{Arc, Mutex, OnceLock};

    /// Shared cache updated by the background poll thread.
    static GPU_CACHE: OnceLock<Arc<Mutex<Option<super::GpuStats>>>> = OnceLock::new();

    /// EWMA poll interval (ms).
    const POLL_MS: u64 = 100;
    /// EWMA time constant (ms).  τ = 500 ms → responds to a 200 ms spike in ~600 ms.
    const EWMA_TAU_MS: f32 = 500.0;
    /// Pre-computed EWMA alpha = 1 − e^(−Δt/τ).
    const EWMA_ALPHA: f32 = 1.0 - {
        // const-eval approximation of 1 − exp(−0.2):
        // Use the series 1 − (1 − x + x²/2 − x³/6) = x − x²/2 + x³/6
        // where x = POLL_MS / EWMA_TAU_MS = 0.2.
        let x: f32 = POLL_MS as f32 / EWMA_TAU_MS;
        1.0 - x + x * x / 2.0 - x * x * x / 6.0
    };

    fn ensure_poller() -> Arc<Mutex<Option<super::GpuStats>>> {
        GPU_CACHE.get_or_init(|| {
            let cache: Arc<Mutex<Option<super::GpuStats>>> = Arc::new(Mutex::new(None));
            let shared = cache.clone();

            std::thread::Builder::new()
                .name("gpu-sampler".into())
                .spawn(move || {
                    // Exponentially-weighted moving-average state per metric.
                    let mut ewma_render:  Option<f32> = None;
                    let mut ewma_tiler:   Option<f32> = None;
                    let mut ewma_overall: Option<f32> = None;

                    loop {
                        if let Some(raw) = raw_read() {
                            let alpha = EWMA_ALPHA;

                            let r = *ewma_render.get_or_insert(raw.render);
                            let t = *ewma_tiler.get_or_insert(raw.tiler);
                            let o = *ewma_overall.get_or_insert(raw.overall);

                            ewma_render  = Some(r + alpha * (raw.render  - r));
                            ewma_tiler   = Some(t + alpha * (raw.tiler   - t));
                            ewma_overall = Some(o + alpha * (raw.overall - o));

                            let smoothed = super::GpuStats {
                                render:  ewma_render.unwrap_or(raw.render),
                                tiler:   ewma_tiler.unwrap_or(raw.tiler),
                                overall: ewma_overall.unwrap_or(raw.overall),
                                ..raw
                            };
                            *shared.lock().unwrap_or_else(|e| e.into_inner()) = Some(smoothed);
                        }

                        std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
                    }
                })
                .expect("gpu-sampler thread spawn");

            cache
        }).clone()
    }

    /// Return the latest smoothed GPU stats from the shared cache.
    pub fn cached_read() -> Option<super::GpuStats> {
        ensure_poller()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    // ── Instantaneous read (used only by the background poller) ──────────

    fn raw_read() -> Option<super::GpuStats> {
        let accs = read_accelerators();

        // Pick the most active accelerator (highest overall utilisation).
        let best = accs.iter().max_by(|a, b| {
            a.overall.partial_cmp(&b.overall).unwrap_or(std::cmp::Ordering::Equal)
        });

        let (render, tiler, overall) = best
            .map(|b| (b.render, b.tiler, b.overall))
            .unwrap_or((0.0, 0.0, 0.0));

        if is_apple_silicon() {
            // Unified memory — GPU and CPU share system RAM.
            let total = system_ram_bytes();
            let free  = unified_free_bytes();

            Some(super::GpuStats {
                render, tiler, overall,
                is_unified_memory:  true,
                total_memory_bytes: total,
                free_memory_bytes:  free,
            })
        } else {
            // Discrete / integrated GPU — report VRAM from IOAccelerator.
            let (vram_free, vram_used) = best
                .map(|b| (b.vram_free_bytes, b.vram_used_bytes))
                .unwrap_or((None, None));

            let total = match (vram_free, vram_used) {
                (Some(f), Some(u)) => Some(f + u),
                _ => None,
            };

            Some(super::GpuStats {
                render, tiler, overall,
                is_unified_memory:  false,
                total_memory_bytes: total,
                free_memory_bytes:  vram_free,
            })
        }
    }
}

// ── Linux / Windows implementation via llmfit-core ────────────────────────────
//
// Strategy
// ────────
// * GPU topology (VRAM total, unified-memory flag, backend) is static — detect
//   it once via `SystemSpecs::detect()` and cache forever in `GPU_STATIC`.
//   `detect()` may spawn short-lived child processes (nvidia-smi, rocm-smi …)
//   so we must not call it on every Tauri poll.
//
// * Available RAM for unified-memory platforms is dynamic — a background
//   thread wakes every `FREE_RAM_INTERVAL` and calls
//   `sysinfo::System::refresh_memory()`, which is a cheap kernel read.
//
// * GPU utilisation (render / tiler / overall) is not provided because there
//   is no cross-platform equivalent of IOKit's PerformanceStatistics.
//   All three fields remain 0.0 on Linux / Windows.

#[cfg(not(target_os = "macos"))]
mod non_macos {
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    // Refresh the free-memory figure for unified-memory platforms every 5 s.
    const FREE_RAM_INTERVAL: Duration = Duration::from_secs(5);

    // ── Static GPU topology (detected once) ───────────────────────────────

    struct StaticGpuInfo {
        /// VRAM or unified memory pool size in bytes (`None` if unknown).
        total_bytes:    Option<u64>,
        /// True for Apple Silicon, AMD Ryzen AI APUs, NVIDIA Grace, etc.
        unified_memory: bool,
    }

    static GPU_STATIC: OnceLock<Option<StaticGpuInfo>> = OnceLock::new();

    fn detect_static() -> Option<StaticGpuInfo> {
        let specs = llmfit_core::hardware::SystemSpecs::detect();
        if !specs.has_gpu {
            return None;
        }
        let gib: f64 = 1024.0 * 1024.0 * 1024.0;
        let total_bytes = specs
            .total_gpu_vram_gb
            .map(|gb| (gb * gib) as u64)
            .filter(|&b| b > 0);

        Some(StaticGpuInfo {
            total_bytes,
            unified_memory: specs.unified_memory,
        })
    }

    // ── Dynamic free-RAM cache for unified-memory platforms ───────────────

    struct FreeRamCache {
        bytes:       Option<u64>,
        refreshed:   Instant,
    }

    static FREE_RAM: OnceLock<Arc<Mutex<FreeRamCache>>> = OnceLock::new();

    fn ensure_free_ram_poller(is_unified: bool) -> Arc<Mutex<FreeRamCache>> {
        FREE_RAM.get_or_init(|| {
            let initial = if is_unified { sample_free_ram() } else { None };
            let cache = Arc::new(Mutex::new(FreeRamCache {
                bytes:     initial,
                refreshed: Instant::now(),
            }));

            // Only bother with a background thread on unified-memory platforms
            // where free RAM is meaningful.
            if is_unified {
                let shared = cache.clone();
                std::thread::Builder::new()
                    .name("gpu-free-ram".into())
                    .spawn(move || loop {
                        std::thread::sleep(FREE_RAM_INTERVAL);
                        let bytes = sample_free_ram();
                        let mut guard = shared.lock().unwrap_or_else(|e| e.into_inner());
                        guard.bytes     = bytes;
                        guard.refreshed = Instant::now();
                    })
                    .ok(); // non-fatal if thread spawn fails
            }

            cache
        })
        .clone()
    }

    /// Cheaply read available system RAM via `sysinfo`.
    ///
    /// `System::refresh_memory()` is a single sysctl / procfs read — orders of
    /// magnitude cheaper than the full `System::new_all()` + `refresh_all()`
    /// used during initial detection.
    fn sample_free_ram() -> Option<u64> {
        use sysinfo::{MemoryRefreshKind, RefreshKind, System};
        let mut sys = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram()),
        );
        sys.refresh_memory();
        let available = sys.available_memory();
        if available > 0 { Some(available) } else { None }
    }

    // ── Public entry point ────────────────────────────────────────────────

    pub fn read() -> Option<super::GpuStats> {
        // One-shot detection (may spawn nvidia-smi / rocm-smi on first call).
        let static_info = GPU_STATIC.get_or_init(detect_static).as_ref()?;

        // For unified-memory platforms, retrieve (and lazily start refreshing)
        // the current available-RAM figure.
        let free_bytes = if static_info.unified_memory {
            let cache = ensure_free_ram_poller(true);
            let guard = cache.lock().unwrap_or_else(|e| e.into_inner());
            guard.bytes
        } else {
            // Discrete GPU: we have no live free-VRAM source without NVML/ADL.
            None
        };

        Some(super::GpuStats {
            // Utilisation is not available outside macOS IOKit.
            render:  0.0,
            tiler:   0.0,
            overall: 0.0,
            is_unified_memory:  static_info.unified_memory,
            total_memory_bytes: static_info.total_bytes,
            free_memory_bytes:  free_bytes,
        })
    }
}
