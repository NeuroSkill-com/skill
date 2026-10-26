// SPDX-License-Identifier: GPL-3.0-only
//! Canonical `unix_ms` for the per-day EEG store.
//!
//! The table carries three historical timestamp formats. The backfill is
//! deliberately *additive*: readers compare `timestamp` in the format they
//! wrote, so it must never be rewritten. Rows imported from a pre-DuckDB
//! `eeg.sqlite` get `unix_ms` from `eeg_store::backfill_unix_ms`.
#![allow(clippy::unwrap_used)]

use skill_data::eeg_store;
use skill_data::util::epoch_ts_to_unix_ms;

/// A day directory holding a legacy `eeg.sqlite` with the given timestamps
/// (no `unix_ms` column, as older builds wrote it).
fn legacy_day(timestamps: &[i64]) -> tempfile::TempDir {
    let day = tempfile::tempdir().unwrap();
    let conn = rusqlite::Connection::open(day.path().join("eeg.sqlite")).unwrap();
    conn.execute_batch(
        "CREATE TABLE embeddings (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL,
            metrics_json TEXT
        );",
    )
    .unwrap();
    for ts in timestamps {
        conn.execute("INSERT INTO embeddings (timestamp) VALUES (?1)", [ts])
            .unwrap();
    }
    day
}

fn rows(conn: &skill_data::duck::Connection) -> Vec<(i64, i64, Option<i64>)> {
    conn.prepare("SELECT id, timestamp, unix_ms FROM embeddings ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// 2026-04-13 23:48:15 UTC expressed in each of the three formats must
/// normalise to the same instant.
#[test]
fn all_three_formats_normalise_to_the_same_instant() {
    let unix_secs: i64 = 1_776_124_095; // 2026-04-13T23:48:15Z
    let dt14: i64 = 20_260_413_234_815;

    assert_eq!(
        epoch_ts_to_unix_ms(unix_secs * 1000),
        unix_secs * 1000,
        "unix ms passes through"
    );
    assert_eq!(epoch_ts_to_unix_ms(dt14), unix_secs * 1000, "14-digit calendar form");
    assert_eq!(
        epoch_ts_to_unix_ms(dt14 * 1000),
        unix_secs * 1000,
        "17-digit calendar form"
    );
}

#[test]
fn sub_second_precision_is_preserved() {
    assert_eq!(epoch_ts_to_unix_ms(1_776_124_095_594), 1_776_124_095_594);
}

/// The whole point: imported rows get a canonical column while `timestamp`
/// is left byte-for-byte alone.
#[test]
fn import_backfills_without_touching_timestamp() {
    let unix_ms = 1_776_124_095_594i64;
    let dt14 = 20_260_413_234_815i64;
    let dt17 = dt14 * 1000;
    let day = legacy_day(&[unix_ms, dt14, dt17]);

    let conn = eeg_store::open(day.path()).unwrap();
    let got = rows(&conn);
    assert_eq!(got.len(), 3, "every row imported");
    for ((_, ts, um), original) in got.iter().zip([unix_ms, dt14, dt17]) {
        assert_eq!(*ts, original, "timestamp must NOT be rewritten");
        assert_eq!(
            *um,
            Some(epoch_ts_to_unix_ms(original)),
            "unix_ms is the normalised instant"
        );
    }
}

/// Re-running is a no-op: nothing is backfilled twice.
#[test]
fn backfill_is_idempotent() {
    let day = legacy_day(&[20_260_413_234_815]);
    let conn = eeg_store::open(day.path()).unwrap();
    assert_eq!(eeg_store::backfill_unix_ms(&conn), 0, "open already backfilled");
    assert_eq!(eeg_store::backfill_unix_ms(&conn), 0);
}

/// A backfill interrupted partway (crash, full disk) resumes, not restarts.
#[test]
fn backfill_resumes_after_partial_run() {
    let day = legacy_day(&[20_260_413_234_815, 20_260_414_010_000]);
    let conn = eeg_store::open(day.path()).unwrap();
    conn.execute("UPDATE embeddings SET unix_ms = NULL WHERE id = 2", [])
        .unwrap();
    assert_eq!(
        eeg_store::backfill_unix_ms(&conn),
        1,
        "only the unfinished row is redone"
    );
    assert!(rows(&conn).iter().all(|(_, _, um)| um.is_some()));
}

/// A value a writer supplied is never clobbered.
#[test]
fn backfill_leaves_populated_rows_alone() {
    let day = tempfile::tempdir().unwrap();
    let conn = eeg_store::open(day.path()).unwrap();
    conn.execute(
        "INSERT INTO embeddings (timestamp, unix_ms) VALUES (?1, ?2)",
        skill_data::duck::params![20_260_413_234_815i64, 42i64],
    )
    .unwrap();
    assert_eq!(
        eeg_store::backfill_unix_ms(&conn),
        0,
        "populated rows are not revisited"
    );
    assert_eq!(rows(&conn)[0].2, Some(42), "writer-supplied value preserved");
}

/// A fresh day store opens cleanly and accepts writes.
#[test]
fn fresh_store_is_clean() {
    let day = tempfile::tempdir().unwrap();
    let conn = eeg_store::open(day.path()).unwrap();
    assert_eq!(eeg_store::backfill_unix_ms(&conn), 0);
    conn.execute("INSERT INTO embeddings (timestamp, unix_ms) VALUES (1, 2)", [])
        .unwrap();
}

/// Validate the import against a real day store, e.g. before a release:
///
/// ```sh
/// mkdir -p /tmp/fixture-day && cp ~/.skill/20260917/eeg.sqlite /tmp/fixture-day/
/// SKILL_MIGRATION_FIXTURE=/tmp/fixture-day \
///   cargo test -p skill-data --test embeddings_unix_ms_migration_tests -- --ignored --nocapture
/// ```
///
/// Ignored by default: it needs real data, and it must be run on a copy
/// (the import renames `eeg.sqlite` to `eeg.sqlite.bak`).
#[test]
#[ignore = "requires SKILL_MIGRATION_FIXTURE pointing at a directory holding a *copy* of a real eeg.sqlite"]
fn imports_a_real_day_store_fixture() {
    let Ok(dir) = std::env::var("SKILL_MIGRATION_FIXTURE") else {
        panic!("set SKILL_MIGRATION_FIXTURE to a directory with a copy of a real eeg.sqlite");
    };
    let day = std::path::PathBuf::from(dir);
    let before: Vec<(i64, i64)> = {
        let src = day.join("eeg.sqlite");
        let conn = rusqlite::Connection::open(&src).unwrap();
        let mut stmt = conn
            .prepare("SELECT id, timestamp FROM embeddings ORDER BY id")
            .unwrap();
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        rows
    };
    assert!(!before.is_empty(), "fixture has no rows");

    let conn = eeg_store::open(&day).unwrap();
    let after = rows(&conn);
    println!("[fixture] sqlite rows={} duckdb rows={}", before.len(), after.len());
    assert_eq!(after.len(), before.len(), "every row imported");
    for ((id, ts), (id2, ts2, um)) in before.iter().zip(&after) {
        assert_eq!((id, ts), (id2, ts2), "row {id}: id/timestamp untouched");
        let um = um.unwrap();
        assert_eq!(um, epoch_ts_to_unix_ms(*ts), "row {id}: unix_ms mismatch");
        assert!(
            (1_600_000_000_000..2_000_000_000_000).contains(&um),
            "row {id}: unix_ms {um} is not a plausible instant"
        );
    }
    let ooo: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM (SELECT unix_ms, LAG(unix_ms) OVER (ORDER BY id) AS prev FROM embeddings)
             WHERE prev IS NOT NULL AND unix_ms < prev",
            [],
            |r| r.get(0),
        )
        .unwrap();
    println!("[fixture] rows out of order by unix_ms: {ooo}");
}
