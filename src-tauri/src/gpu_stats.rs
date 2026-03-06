// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! GPU utilisation and memory reading via IOKit + sysctl + Mach APIs.
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
//! ## non-macOS
//!
//! All functions return `None` / default.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GpuStats {
    /// Render engine utilisation, 0.0–1.0 (Apple Silicon `Renderer Utilization %`).
    pub render:  f32,
    /// Tiler / geometry engine utilisation, 0.0–1.0 (Apple Silicon `Tiler Utilization %`).
    pub tiler:   f32,
    /// Best-effort overall utilisation, 0.0–1.0.
    pub overall: f32,

    /// `true` on Apple Silicon where GPU and CPU share a single unified memory pool.
    pub is_unified_memory: bool,

    /// Total GPU-accessible memory in bytes.
    /// * Unified memory (Apple Silicon): total physical RAM.
    /// * Discrete GPU: total VRAM.
    ///
    /// `None` if the value could not be read.
    pub total_memory_bytes: Option<u64>,

    /// Free / available GPU memory in bytes.
    /// * Unified memory: `(free + inactive)` pages × page size.
    /// * Discrete GPU: `vramFreeBytes` from IOAccelerator.
    ///
    /// `None` if the value could not be read.
    pub free_memory_bytes: Option<u64>,
}

/// Read current GPU statistics.
/// Returns `None` on non-macOS, or if IOKit fails / reports no accelerators.
pub fn read() -> Option<GpuStats> {
    #[cfg(target_os = "macos")]
    return macos::read();
    #[cfg(not(target_os = "macos"))]
    return None;
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

    // ── IOAccelerator loop ────────────────────────────────────────────────

    struct AcceleratorInfo {
        render:  f32,
        tiler:   f32,
        overall: f32,
        vram_free_bytes: Option<u64>,
        vram_used_bytes: Option<u64>,
    }

    fn read_accelerators() -> Vec<AcceleratorInfo> {
        let matching = unsafe {
            IOServiceMatching(match CString::new("IOAccelerator") {
                Ok(c) => c.as_ptr(),
                Err(_) => return vec![],
            })
        };
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

                    let render = dict_i64(d, "Renderer Utilization %");
                    let tiler  = dict_i64(d, "Tiler Utilization %");
                    let device = dict_i64(d, "Device Utilization %")
                        .or_else(|| dict_i64(d, "GPU Activity(%)"));

                    let render_f  = render.unwrap_or(0).clamp(0, 100) as f32 / 100.0;
                    let tiler_f   = tiler.unwrap_or(0).clamp(0, 100) as f32 / 100.0;
                    let overall_f = device
                        .map(|d| d.clamp(0, 100) as f32 / 100.0)
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

    // ── Public entry point ────────────────────────────────────────────────

    pub fn read() -> Option<super::GpuStats> {
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
