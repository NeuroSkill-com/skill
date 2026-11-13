// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! One integration-test binary for the whole crate.
//!
//! Every file directly under `tests/` used to be its own binary, and each one
//! statically linked DuckDB and the rest of the dependency tree (~55 MB apiece
//! in a dev build). They are modules of this one instead, so the tree is linked
//! once. Add a new test file here as another `mod` line; `autotests = false`
//! in `Cargo.toml` means a bare `tests/foo.rs` is not picked up.
//!
//! Run one file with `cargo test -p skill-data --test it <file_name>::`.

#[path = "embeddings_unix_ms_migration_tests.rs"]
mod embeddings_unix_ms_migration_tests;
#[path = "hooks_log_tests.rs"]
mod hooks_log_tests;
#[path = "import_real_stores.rs"]
mod import_real_stores;
#[path = "screenshot_store_tests.rs"]
mod screenshot_store_tests;
#[path = "session_csv_roundtrip_tests.rs"]
mod session_csv_roundtrip_tests;
#[path = "session_csv_tests.rs"]
mod session_csv_tests;
#[path = "session_parquet_tests.rs"]
mod session_parquet_tests;
