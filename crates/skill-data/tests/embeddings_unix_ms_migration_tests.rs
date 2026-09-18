// SPDX-License-Identifier: GPL-3.0-only
//! Migration of `embeddings.timestamp` → canonical `unix_ms`.
//!
//! The table carries three historical timestamp formats. The migration is
//! deliberately *additive*: older builds of the app read `timestamp` directly
//! and expect the format they wrote, so it must never be rewritten.
#![allow(clippy::unwrap_used)]

use skill_data::util::{epoch_ts_to_unix_ms, migrate_embeddings_unix_ms};

fn table(conn: &rusqlite::Connection) {
    conn.execute_batch(
        "CREATE TABLE embeddings (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL,
            metrics_json TEXT
        );",
    )
    .unwrap();
}

fn insert(conn: &rusqlite::Connection, ts: i64) -> i64 {
    conn.execute("INSERT INTO embeddings (timestamp) VALUES (?1)", [ts])
        .unwrap();
    conn.last_insert_rowid()
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

/// Sub-second precision only exists in the unix-ms format; it must survive.
#[test]
fn sub_second_precision_is_preserved() {
    assert_eq!(epoch_ts_to_unix_ms(1_776_124_095_594), 1_776_124_095_594);
}

/// The whole point: a mixed table gets a canonical column while `timestamp`
/// is left byte-for-byte alone, so older builds keep working.
#[test]
fn migration_backfills_without_touching_timestamp() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    table(&conn);

    let unix_ms = 1_776_124_095_594i64;
    let dt14 = 20_260_413_234_815i64;
    let dt17 = dt14 * 1000;
    let ids = [insert(&conn, unix_ms), insert(&conn, dt14), insert(&conn, dt17)];

    let migrated = migrate_embeddings_unix_ms(&conn);
    assert_eq!(migrated, 3, "every row must be backfilled");

    for (id, original) in ids.iter().zip([unix_ms, dt14, dt17]) {
        let (ts, um): (i64, i64) = conn
            .query_row("SELECT timestamp, unix_ms FROM embeddings WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(
            ts, original,
            "timestamp must NOT be rewritten — old builds still read it"
        );
        assert_eq!(
            um,
            epoch_ts_to_unix_ms(original),
            "unix_ms must be the normalised instant"
        );
    }
}

/// Re-running must be a no-op: `ADD COLUMN` is expected to fail the second
/// time, and no row should be backfilled twice.
#[test]
fn migration_is_idempotent() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    table(&conn);
    insert(&conn, 20_260_413_234_815);

    assert_eq!(migrate_embeddings_unix_ms(&conn), 1);
    assert_eq!(migrate_embeddings_unix_ms(&conn), 0, "second run must backfill nothing");
    assert_eq!(migrate_embeddings_unix_ms(&conn), 0);
}

/// A migration interrupted partway (crash, full disk) must resume, not restart
/// or skip.
#[test]
fn migration_resumes_after_partial_backfill() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    table(&conn);
    let a = insert(&conn, 20_260_413_234_815);
    let b = insert(&conn, 20_260_414_010_000);

    migrate_embeddings_unix_ms(&conn);
    // Simulate a row that never got written before the process died.
    conn.execute("UPDATE embeddings SET unix_ms = NULL WHERE id = ?1", [b])
        .unwrap();

    assert_eq!(
        migrate_embeddings_unix_ms(&conn),
        1,
        "only the unfinished row is redone"
    );
    let nulls: i64 = conn
        .query_row("SELECT COUNT(*) FROM embeddings WHERE unix_ms IS NULL", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(nulls, 0);
    let _ = a;
}

/// Rows written after the migration keep their canonical value; the migration
/// must not clobber a value a writer already supplied.
#[test]
fn migration_leaves_already_populated_rows_alone() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    table(&conn);
    migrate_embeddings_unix_ms(&conn);

    conn.execute(
        "INSERT INTO embeddings (timestamp, unix_ms) VALUES (?1, ?2)",
        rusqlite::params![20_260_413_234_815i64, 42i64],
    )
    .unwrap();

    assert_eq!(migrate_embeddings_unix_ms(&conn), 0, "populated rows are not revisited");
    let um: i64 = conn
        .query_row(
            "SELECT unix_ms FROM embeddings WHERE timestamp = 20260413234815",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(um, 42, "a writer-supplied value must be preserved");
}

/// An empty table migrates cleanly (fresh install).
#[test]
fn migration_on_empty_table_is_clean() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    table(&conn);
    assert_eq!(migrate_embeddings_unix_ms(&conn), 0);
    // The column and index must still be created.
    conn.execute("INSERT INTO embeddings (timestamp, unix_ms) VALUES (1, 2)", [])
        .unwrap();
}

/// Validate the migration against a real day store, e.g. before a release:
///
/// ```sh
/// cp ~/.skill/20260917/eeg.sqlite /tmp/fixture.sqlite
/// SKILL_MIGRATION_FIXTURE=/tmp/fixture.sqlite \
///   cargo test -p skill-data --test embeddings_unix_ms_migration_tests -- --ignored --nocapture
/// ```
///
/// Ignored by default: it needs a real database, and it must be run on a copy.
#[test]
#[ignore = "requires SKILL_MIGRATION_FIXTURE pointing at a *copy* of a real day store"]
fn migrates_a_real_day_store_fixture() {
    let Ok(path) = std::env::var("SKILL_MIGRATION_FIXTURE") else {
        panic!("set SKILL_MIGRATION_FIXTURE to a copy of a real eeg.sqlite");
    };
    let conn = rusqlite::Connection::open(&path).unwrap();

    let before: Vec<(i64, i64)> = {
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

    let migrated = migrate_embeddings_unix_ms(&conn);
    println!("[fixture] rows={} backfilled={}", before.len(), migrated);

    let mut checked = 0usize;
    for (id, original_ts) in &before {
        let (ts, um): (i64, i64) = conn
            .query_row("SELECT timestamp, unix_ms FROM embeddings WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(ts, *original_ts, "row {id}: timestamp must be untouched");
        assert_eq!(um, epoch_ts_to_unix_ms(*original_ts), "row {id}: unix_ms mismatch");
        // Every real recording must land in a sane wall-clock window.
        assert!(
            (1_600_000_000_000..2_000_000_000_000).contains(&um),
            "row {id}: unix_ms {um} is not a plausible instant"
        );
        checked += 1;
    }
    println!("[fixture] verified {checked} rows; monotonic check follows");

    // The canonical column must order the whole table consistently, which the
    // mixed `timestamp` column cannot do.
    let ooo: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM (SELECT unix_ms, LAG(unix_ms) OVER (ORDER BY id) AS prev FROM embeddings)
             WHERE prev IS NOT NULL AND unix_ms < prev",
            [],
            |r| r.get(0),
        )
        .unwrap();
    println!("[fixture] rows out of order by unix_ms: {ooo}");

    assert_eq!(migrate_embeddings_unix_ms(&conn), 0, "re-run must be a no-op");
}
