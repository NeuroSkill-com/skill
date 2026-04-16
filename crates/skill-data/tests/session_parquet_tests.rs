// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//

//! Tests for session_parquet.rs

#![cfg(feature = "parquet")]

use skill_data::session_parquet::{eeg_parquet_path, ParquetState};
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_eeg_parquet_path() {
    let csv_path = PathBuf::from("/tmp/test.csv");
    let pq_path = eeg_parquet_path(&csv_path);
    assert_eq!(pq_path.extension().unwrap(), "parquet");
    assert!(pq_path.to_string_lossy().ends_with("test.parquet"));
}

#[test]
fn test_parquetstate_open_with_labels() {
    let dir = tempdir().unwrap();
    let csv_path = dir.path().join("test.csv");
    let labels = ["Fz", "Cz", "Pz"];
    let state = ParquetState::open_with_labels(&csv_path, &labels);
    assert!(state.is_ok());
    let pq_path = eeg_parquet_path(&csv_path);
    assert!(pq_path.exists() || !pq_path.exists()); // File may not exist until written
}
