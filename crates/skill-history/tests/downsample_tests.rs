// SPDX-License-Identifier: GPL-3.0-only
//! Tests for downsample_timeseries.
#![allow(clippy::unwrap_used)]

use skill_history::cache::downsample_timeseries;
use skill_history::EpochRow;

fn make_rows(n: usize) -> Vec<EpochRow> {
    (0..n)
        .map(|i| {
            let mut row = EpochRow::default();
            row.t = i as f64;
            row.ra = (i as f64) * 0.01; // alpha increases linearly
            row
        })
        .collect()
}

#[test]
fn downsample_no_op_when_under_max() {
    let mut rows = make_rows(5);
    downsample_timeseries(&mut rows, 10);
    assert_eq!(rows.len(), 5);
}

#[test]
fn downsample_no_op_at_exact_max() {
    let mut rows = make_rows(10);
    downsample_timeseries(&mut rows, 10);
    assert_eq!(rows.len(), 10);
}

#[test]
fn downsample_reduces_to_max() {
    let mut rows = make_rows(100);
    downsample_timeseries(&mut rows, 20);
    assert!(rows.len() <= 20);
}

#[test]
fn downsample_preserves_order() {
    let mut rows = make_rows(50);
    downsample_timeseries(&mut rows, 10);
    for w in rows.windows(2) {
        assert!(w[0].t <= w[1].t, "timestamps should be monotonic");
    }
}

#[test]
fn downsample_empty_input() {
    let mut rows: Vec<EpochRow> = vec![];
    downsample_timeseries(&mut rows, 10);
    assert!(rows.is_empty());
}

#[test]
fn downsample_max_below_2_is_noop() {
    // max < 2 is a no-op (can't meaningfully downsample to 0 or 1 points)
    let mut rows = make_rows(20);
    downsample_timeseries(&mut rows, 1);
    assert_eq!(rows.len(), 20);
}

#[test]
fn downsample_to_2() {
    let mut rows = make_rows(100);
    downsample_timeseries(&mut rows, 2);
    assert_eq!(rows.len(), 2);
    // Should keep first and last
    assert_eq!(rows[0].t as u64, 0);
    assert_eq!(rows[1].t as u64, 99);
}
