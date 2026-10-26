// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Per-day EEG embeddings store — `~/.skill/YYYYMMDD/eeg.duckdb`.
//!
//! One `embeddings` row per epoch: the embedding vector (little-endian f32
//! blob, empty for metrics-only rows), its HNSW id, device, and the epoch's
//! metrics as JSON. The embedding worker writes today's file; history,
//! search, analysis and the label index read across days.
//!
//! Every reader and writer goes through [`open`] / [`open_existing`] so the
//! schema, the one-time import of a legacy `eeg.sqlite`, and the `unix_ms`
//! backfill happen in one place.
//!
//! ## Timestamps
//!
//! `timestamp` carries three historical formats (unix ms, unix seconds,
//! `YYYYMMDDhhmmss`) and is never rewritten, because readers compare in the
//! format they wrote. `unix_ms` is the canonical instant, filled at insert and
//! backfilled for imported rows with [`epoch_ts_to_unix_ms`].
//!
//! ## Metrics
//!
//! `metrics_json` is plain JSON text. DuckDB's `json_extract` returns its
//! `JSON` type rather than a number, so numeric reads go through [`metric`],
//! which yields a `DOUBLE` (NULL when the key is absent or not a number).

use std::path::{Path, PathBuf};

use crate::duck::{params, Conn};
use crate::util::epoch_ts_to_unix_ms;

const SCHEMA: crate::duck::StoreSchema<'static> = crate::duck::StoreSchema {
    ddl: "
        CREATE TABLE IF NOT EXISTS embeddings (
            id              BIGINT  PRIMARY KEY DEFAULT nextval('embeddings_id_seq'),
            timestamp       BIGINT  NOT NULL,
            unix_ms         BIGINT,
            device_id       VARCHAR,
            device_name     VARCHAR,
            hnsw_id         BIGINT  DEFAULT 0,
            eeg_embedding   BLOB,
            label           VARCHAR,
            extra_embedding BLOB,
            ppg_ambient     DOUBLE,
            ppg_infrared    DOUBLE,
            ppg_red         DOUBLE,
            metrics_json    VARCHAR
        );
        CREATE INDEX IF NOT EXISTS idx_embeddings_timestamp ON embeddings (timestamp);
        CREATE INDEX IF NOT EXISTS idx_embeddings_unix_ms ON embeddings (unix_ms);
    ",
    import_tables: &["embeddings"],
    sequences: &[("embeddings_id_seq", "embeddings", "id")],
};

/// `day_dir/eeg.duckdb`.
pub fn db_path(day_dir: &Path) -> PathBuf {
    day_dir.join(skill_constants::SQLITE_FILE)
}

/// Whether `day_dir` has an EEG store (current, or legacy awaiting import).
pub fn exists(day_dir: &Path) -> bool {
    let p = db_path(day_dir);
    p.exists() || crate::duck::legacy_sqlite_path(&p).exists()
}

/// Open (creating if needed) the store for `day_dir`.
pub fn open(day_dir: &Path) -> anyhow::Result<Conn> {
    let conn = crate::duck::open_store(&db_path(day_dir), &SCHEMA)?;
    backfill_unix_ms(&conn);
    Ok(conn)
}

/// Open the store for `day_dir` only if it exists — for readers that must
/// not create empty day databases. Errors are logged and yield `None`.
pub fn open_existing(day_dir: &Path) -> Option<Conn> {
    if !exists(day_dir) {
        return None;
    }
    open(day_dir)
        .map_err(|e| eprintln!("[eeg_store] open {}: {e:#}", day_dir.display()))
        .ok()
}

/// SQL expression for a numeric metric from `metrics_json`, as `DOUBLE`.
/// `field` must be a plain identifier (it is spliced into the SQL).
pub fn metric(field: &str) -> String {
    debug_assert!(field.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
    format!("TRY_CAST(json_extract_string(metrics_json, '$.{field}') AS DOUBLE)")
}

/// Fill `unix_ms` for rows that predate it (imported from old files).
/// Returns the number of rows updated.
pub fn backfill_unix_ms(conn: &Conn) -> usize {
    let pending: Vec<(i64, i64)> = conn
        .prepare("SELECT id, timestamp FROM embeddings WHERE unix_ms IS NULL")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_default();
    if pending.is_empty() {
        return 0;
    }
    let mut done = 0;
    if conn.execute_batch("BEGIN TRANSACTION").is_ok() {
        if let Ok(mut stmt) = conn.prepare("UPDATE embeddings SET unix_ms = ?1 WHERE id = ?2") {
            for (id, ts) in &pending {
                if stmt.execute(params![epoch_ts_to_unix_ms(*ts), id]).is_ok() {
                    done += 1;
                }
            }
        }
        let _ = conn.execute_batch("COMMIT");
    }
    done
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn imports_legacy_sqlite_and_backfills_unix_ms() {
        let day = tempfile::tempdir().unwrap();
        let legacy = day.path().join("eeg.sqlite");
        {
            let s = rusqlite::Connection::open(&legacy).unwrap();
            s.execute_batch(
                "CREATE TABLE embeddings (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL,
                    device_name TEXT, hnsw_id INTEGER DEFAULT 0, eeg_embedding BLOB,
                    metrics_json TEXT);
                 INSERT INTO embeddings (timestamp, eeg_embedding, metrics_json)
                   VALUES (20260413234815, x'0000803f', '{\"relaxation_score\": 61.5, \"faa\": -0.2}');
                 INSERT INTO embeddings (timestamp, metrics_json)
                   VALUES (1776124096000, '{\"relaxation_score\": \"n/a\"}');",
            )
            .unwrap();
        }
        assert!(exists(day.path()));
        let conn = open(day.path()).unwrap();
        assert!(!legacy.exists(), "legacy file moved to .bak");

        type Row = (i64, Option<i64>, Option<f64>, Option<f64>);
        let rows: Vec<Row> = conn
            .prepare(&format!(
                "SELECT id, unix_ms, {}, {} FROM embeddings ORDER BY id",
                metric("relaxation_score"),
                metric("faa")
            ))
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows[0], (1, Some(1_776_124_095_000), Some(61.5), Some(-0.2)));
        // Non-numeric metric reads as NULL instead of failing the query.
        assert_eq!(rows[1], (2, Some(1_776_124_096_000), None, None));

        // New inserts continue the imported id sequence.
        let id: i64 = conn
            .query_row(
                "INSERT INTO embeddings (timestamp, unix_ms) VALUES (1, 1) RETURNING id",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(id, 3);
    }

    #[test]
    fn open_existing_does_not_create() {
        let day = tempfile::tempdir().unwrap();
        assert!(open_existing(day.path()).is_none());
        assert!(!db_path(day.path()).exists());
    }
}
