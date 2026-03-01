// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! GPU utilisation reading via IOKit — mirrors the approach used by Stats.app
//! (<https://github.com/exelban/stats>, `Modules/GPU/reader.swift`).
//!
//! On macOS the function iterates over `IOAccelerator` services and reads the
//! `PerformanceStatistics` dictionary for:
//!
//! | key                    | GPU family |
//! |------------------------|------------|
//! | `Renderer Utilization %` | Apple Silicon |
//! | `Tiler Utilization %`    | Apple Silicon |
//! | `Device Utilization %`   | AMD / Nvidia / fallback |
//! | `GPU Activity(%)`        | AMD alternate key |
//!
//! On non-macOS targets the function always returns `None`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GpuStats {
    /// Render engine utilisation, 0.0–1.0 (Apple Silicon `Renderer Utilization %`).
    pub render:  f32,
    /// Tiler / geometry engine utilisation, 0.0–1.0 (Apple Silicon `Tiler Utilization %`).
    pub tiler:   f32,
    /// Best-effort overall utilisation, 0.0–1.0.
    /// Uses `Device Utilization %` / `GPU Activity(%)` when present,
    /// otherwise averages `render` + `tiler`.
    pub overall: f32,
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

    const KERN_SUCCESS:               KernReturnT  = 0;
    const K_IO_MASTER_PORT_DEFAULT:   u32          = 0;
    const CF_NUMBER_SI32_TYPE:        CFNumberType = 3;   // kCFNumberSInt32Type
    const K_CF_STRING_ENCODING_UTF8:  u32          = 0x0800_0100;

    // ── IOKit framework ───────────────────────────────────────────────────
    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOServiceMatching(name: *const c_char) -> CFMutableDictRef;
        fn IOServiceGetMatchingServices(
            master_port: u32,
            matching:    CFMutableDictRef, // consumed — do NOT release
            existing:    *mut IOIteratorT,
        ) -> KernReturnT;
        fn IOIteratorNext(iterator: IOIteratorT) -> IORegistryEntryT;
        fn IOObjectRelease(object: u32) -> KernReturnT;
        fn IORegistryEntryCreateCFProperties(
            entry:       IORegistryEntryT,
            properties:  *mut CFMutableDictRef, // caller must release
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
        ) -> CFStringRef; // caller must release
        fn CFNumberGetValue(
            number:     CFTypeRef,
            the_type:   CFNumberType,
            value_ptr:  *mut i32,
        ) -> bool;
        fn CFRelease(cf: CFTypeRef);
    }

    // ── Helpers ───────────────────────────────────────────────────────────

    /// Call `f` with a temporary CFStringRef built from `s`.
    /// Releases the CFString when `f` returns.
    fn with_cf_str<T>(s: &str, f: impl FnOnce(CFStringRef) -> T) -> T {
        let c = CString::new(s).unwrap_or_default();
        let cf = unsafe {
            CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), K_CF_STRING_ENCODING_UTF8)
        };
        let result = f(cf);
        if !cf.is_null() {
            unsafe { CFRelease(cf as _) };
        }
        result
    }

    /// Read a 32-bit integer from a CFDictionary value.
    fn dict_i32(dict: CFDictionaryRef, key: &str) -> Option<i32> {
        with_cf_str(key, |k| {
            let v = unsafe { CFDictionaryGetValue(dict, k) };
            if v.is_null() { return None; }
            let mut out: i32 = 0;
            // `v` is owned by `dict` — do NOT release.
            if unsafe { CFNumberGetValue(v, CF_NUMBER_SI32_TYPE, &mut out) } {
                Some(out)
            } else {
                None
            }
        })
    }

    // ── Main reader ───────────────────────────────────────────────────────

    pub fn read() -> Option<super::GpuStats> {
        let matching = unsafe {
            IOServiceMatching(CString::new("IOAccelerator").ok()?.as_ptr())
        };
        if matching.is_null() { return None; }

        let mut iter: IOIteratorT = 0;
        // `matching` is consumed here; do NOT CFRelease it afterwards.
        if unsafe { IOServiceGetMatchingServices(K_IO_MASTER_PORT_DEFAULT, matching, &mut iter) }
            != KERN_SUCCESS
        {
            return None;
        }

        let mut best: Option<super::GpuStats> = None;

        loop {
            let entry = unsafe { IOIteratorNext(iter) };
            if entry == 0 { break; }

            let mut props: CFMutableDictRef = std::ptr::null_mut();
            let kr = unsafe {
                IORegistryEntryCreateCFProperties(
                    entry,
                    &mut props,
                    std::ptr::null(), // kCFAllocatorDefault
                    0,
                )
            };

            if kr == KERN_SUCCESS && !props.is_null() {
                // PerformanceStatistics is a CFDictionary nested inside props.
                let perf = with_cf_str("PerformanceStatistics", |k| unsafe {
                    CFDictionaryGetValue(props as CFDictionaryRef, k)
                });

                if !perf.is_null() {
                    let render = dict_i32(perf as CFDictionaryRef, "Renderer Utilization %");
                    let tiler  = dict_i32(perf as CFDictionaryRef, "Tiler Utilization %");
                    let device = dict_i32(perf as CFDictionaryRef, "Device Utilization %")
                        .or_else(|| dict_i32(perf as CFDictionaryRef, "GPU Activity(%)"));

                    let render_f  = render.unwrap_or(0).clamp(0, 100) as f32 / 100.0;
                    let tiler_f   = tiler.unwrap_or(0).clamp(0, 100) as f32 / 100.0;
                    let overall_f = device
                        .map(|d| d.clamp(0, 100) as f32 / 100.0)
                        .unwrap_or_else(|| (render_f + tiler_f) / 2.0);

                    // Prefer the GPU that reports non-zero activity.
                    if best.is_none() || overall_f > best.as_ref().map_or(0.0, |b| b.overall) {
                        best = Some(super::GpuStats {
                            render:  render_f,
                            tiler:   tiler_f,
                            overall: overall_f,
                        });
                    }
                }

                // perf is borrowed from props — do NOT release separately.
                unsafe { CFRelease(props as CFTypeRef) };
            }

            unsafe { IOObjectRelease(entry) };
        }

        unsafe { IOObjectRelease(iter) };
        best
    }
}
