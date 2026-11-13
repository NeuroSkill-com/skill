// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! One integration-test binary for the fast tests of this crate.
//!
//! Each file directly under `tests/` used to be its own binary, and every one
//! statically linked the full dependency tree. They are modules of this one
//! instead, so it is linked once. Add a new test file as another `mod` line:
//! `autotests = false` in `Cargo.toml` means a bare `tests/foo.rs` is not
//! picked up. `*_e2e*` files stay separate targets because CI and scripts run
//! them by name with their own features.
//!
//! Run one file with `cargo test -p skill-label-index --test it <file_name>::`.

#[path = "daemon_hnsw_loop_test.rs"]
mod daemon_hnsw_loop_test;
#[path = "label_index_state_tests.rs"]
mod label_index_state_tests;
