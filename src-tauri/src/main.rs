// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The Tauri `run()` function has an enormous stack frame because
    // `generate_handler!` expands ~150 command handlers inline, plus deep
    // generics from serde / HNSW / llama.cpp.
    //
    // On Linux, raising RLIMIT_STACK preserves the true main-thread stack.
    // That avoids JavaScriptCoreGTK's stack-sanitizer abort, which is
    // triggered when `stacker` swaps the main-thread stack pointer.
    #[cfg(target_os = "linux")]
    {
        const DESIRED_STACK: libc::rlim_t = 64 * 1024 * 1024;

        // SAFETY: `getrlimit` and `setrlimit` are POSIX-specified syscall
        // wrappers that read/write a plain `rlimit` struct.  We pass a valid
        // mutable pointer to a stack-local struct and check the return value
        // before using the result.  No aliasing or lifetime issues.
        unsafe {
            let mut limit = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::getrlimit(libc::RLIMIT_STACK, &mut limit) == 0 {
                let desired = if limit.rlim_max == libc::RLIM_INFINITY {
                    DESIRED_STACK
                } else {
                    std::cmp::min(limit.rlim_max, DESIRED_STACK)
                };
                if desired > limit.rlim_cur {
                    let raised = libc::rlimit {
                        rlim_cur: desired,
                        rlim_max: limit.rlim_max,
                    };
                    let _ = libc::setrlimit(libc::RLIMIT_STACK, &raised);
                }
            }
        }

        skill_lib::run();
    }

    // On macOS and Windows we still need `stacker`: Cocoa requires the app
    // loop on the original main thread, and Windows' default 1 MiB stack is
    // far too small for the generated Tauri handler stack frame.
    #[cfg(not(target_os = "linux"))]
    {
        const RED_ZONE: usize  = 32 * 1024 * 1024; // 32 MiB remaining trigger
        const NEW_STACK: usize = 64 * 1024 * 1024; // 64 MiB new stack

        stacker::maybe_grow(RED_ZONE, NEW_STACK, || {
            skill_lib::run();
        });
    }
}
