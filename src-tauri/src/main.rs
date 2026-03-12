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
    // generics from serde / HNSW / llama.cpp.  The default main-thread
    // stack (8 MB on macOS / Linux, 1 MB on Windows) overflows.
    //
    // We cannot spawn a separate thread because macOS Cocoa / AppKit
    // requires the NSApplication run-loop on the original main thread.
    // Linker flags (-Wl,-stack_size) are unreliable across Tauri's
    // mixed crate-type build (staticlib + cdylib + rlib).
    //
    // `stacker::maybe_grow` dynamically extends the stack using mmap +
    // inline-asm stack-pointer swap (via the `psm` crate).  The main
    // thread identity is preserved so Cocoa is happy.  Pages are lazily
    // committed, so the 64 MiB reservation costs only what is actually
    // touched.
    const RED_ZONE: usize  = 32 * 1024 * 1024; // 32 MiB remaining trigger
    const NEW_STACK: usize = 64 * 1024 * 1024; // 64 MiB new stack

    stacker::maybe_grow(RED_ZONE, NEW_STACK, || {
        skill_lib::run();
    });
}
