// SPDX-License-Identifier: GPL-3.0-only
//! Per-day HNSW + SQLite store for EEG embeddings.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::{error, info, warn};

fn blob_to_f32(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn rebuild_hnsw_from_sqlite(
    conn: &rusqlite::Connection,
    hnsw_m: usize,
    hnsw_ef_construction: usize,
) -> fast_hnsw::labeled::LabeledIndex<fast_hnsw::distance::Cosine, i64> {
    let cfg = fast_hnsw::hnsw::Config {
        m: hnsw_m,
        ef_construction: hnsw_ef_construction,
        ..Default::default()
    };
    let mut idx = fast_hnsw::labeled::LabeledIndex::new(cfg, fast_hnsw::distance::Cosine);

    let mut rebuilt = 0usize;
    if let Ok(mut stmt) = conn.prepare(
        "SELECT timestamp, eeg_embedding
         FROM embeddings
         WHERE eeg_embedding IS NOT NULL AND length(eeg_embedding) >= 4
         ORDER BY id ASC",
    ) {
        if let Ok(rows) = stmt.query_map([], |row| {
            let ts: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            Ok((ts, blob))
        }) {
            for row in rows.flatten() {
                let (ts, blob) = row;
                if blob.len() < 4 || blob.len() % 4 != 0 {
                    continue;
                }
                let emb = blob_to_f32(&blob);
                if emb.is_empty() {
                    continue;
                }
                idx.insert(emb, ts);
                rebuilt += 1;
            }
        }
    }

    info!(rebuilt, "rebuilt HNSW vectors from SQLite");
    idx
}

/// Per-day storage for embeddings (SQLite) + ANN index (HNSW).
pub(super) struct DayStore {
    pub conn: rusqlite::Connection,
    pub hnsw: Option<fast_hnsw::labeled::LabeledIndex<fast_hnsw::distance::Cosine, i64>>,
    pub index_path: PathBuf,
    pub db_path: PathBuf,
    hnsw_len: usize,
    hnsw_rebuilt: bool,
    hnsw_rebuilt_count: usize,
}

impl DayStore {
    /// Open or create the day store for `date_dir` (e.g. `~/.skill/20260406/`).
    pub fn open(day_dir: &Path, hnsw_m: usize, hnsw_ef_construction: usize) -> Option<Self> {
        let db_path = day_dir.join(skill_constants::SQLITE_FILE);
        let index_path = day_dir.join(skill_constants::HNSW_INDEX_FILE);
        let legacy_index_path = day_dir.join("exg_embeddings.hnsw");

        // One-time migration: older builds used `exg_embeddings.hnsw` while the rest
        // of the app reads `eeg_embeddings.hnsw`.
        if !index_path.exists() && legacy_index_path.exists() {
            match std::fs::rename(&legacy_index_path, &index_path) {
                Ok(()) => {
                    info!(
                        from = %legacy_index_path.display(),
                        to = %index_path.display(),
                        "migrated legacy HNSW filename"
                    );
                }
                Err(e) => {
                    warn!(
                        %e,
                        from = %legacy_index_path.display(),
                        to = %index_path.display(),
                        "failed to migrate legacy HNSW filename"
                    );
                }
            }
        }

        let conn = rusqlite::Connection::open(&db_path).ok()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS embeddings (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp       INTEGER NOT NULL,
                device_id       TEXT,
                device_name     TEXT,
                hnsw_id         INTEGER DEFAULT 0,
                eeg_embedding   BLOB,
                label           TEXT,
                extra_embedding BLOB,
                ppg_ambient     REAL,
                ppg_infrared    REAL,
                ppg_red         REAL,
                metrics_json    TEXT
            );",
        )
        .ok()?;

        // Load or create the HNSW index.
        let (hnsw, hnsw_len, hnsw_rebuilt, hnsw_rebuilt_count) = if index_path.exists() {
            match fast_hnsw::labeled::LabeledIndex::<fast_hnsw::distance::Cosine, i64>::load(
                &index_path,
                fast_hnsw::distance::Cosine,
            ) {
                Ok(idx) => {
                    let len = idx.len();
                    info!(len, path = %index_path.display(), "loaded existing HNSW index");
                    (Some(idx), len, false, 0)
                }
                Err(e) => {
                    error!(path = %index_path.display(), %e, "failed to load HNSW index, rebuilding from SQLite");

                    // Prevent re-logging the same failure on every startup by moving
                    // unreadable/corrupt files aside.
                    let ts = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let bad_path = index_path.with_extension(format!("hnsw.corrupt-{ts}"));
                    match std::fs::rename(&index_path, &bad_path) {
                        Ok(()) => warn!(
                            from = %index_path.display(),
                            to = %bad_path.display(),
                            "moved unreadable HNSW index aside"
                        ),
                        Err(rename_err) => warn!(
                            %rename_err,
                            path = %index_path.display(),
                            "failed to move unreadable HNSW index aside"
                        ),
                    }

                    let idx = rebuild_hnsw_from_sqlite(&conn, hnsw_m, hnsw_ef_construction);
                    let rebuilt_len = idx.len();
                    if rebuilt_len > 0 {
                        if let Err(save_err) = idx.save(&index_path) {
                            warn!(%save_err, path = %index_path.display(), "failed to persist rebuilt HNSW index");
                        } else {
                            info!(path = %index_path.display(), rebuilt_len, "persisted rebuilt HNSW index");
                        }
                    }
                    (Some(idx), rebuilt_len, true, rebuilt_len)
                }
            }
        } else {
            let cfg = fast_hnsw::hnsw::Config {
                m: hnsw_m,
                ef_construction: hnsw_ef_construction,
                ..Default::default()
            };
            let idx = fast_hnsw::labeled::LabeledIndex::new(cfg, fast_hnsw::distance::Cosine);
            (Some(idx), 0, false, 0)
        };

        Some(Self {
            conn,
            hnsw,
            index_path,
            db_path,
            hnsw_len,
            hnsw_rebuilt,
            hnsw_rebuilt_count,
        })
    }

    /// Insert an embedding + metrics into SQLite and HNSW.
    /// Returns the HNSW id (zero-based).
    pub fn insert(
        &mut self,
        timestamp_ms: i64,
        device_name: Option<&str>,
        embedding: &[f32],
        metrics: Option<&skill_exg::EpochMetrics>,
    ) -> usize {
        let metrics_json: Option<String> = metrics.and_then(|m| serde_json::to_string(m).ok());

        // Store embedding as little-endian f32 blob.
        let blob: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();

        // Insert into HNSW.
        let hnsw_id = if let Some(ref mut idx) = self.hnsw {
            let id = self.hnsw_len;
            idx.insert(embedding.to_vec(), timestamp_ms);
            self.hnsw_len += 1;
            id
        } else {
            0
        };

        // Insert into SQLite.
        let _ = self.conn.execute(
            "INSERT INTO embeddings
             (timestamp, device_id, device_name, hnsw_id, eeg_embedding, metrics_json)
             VALUES (?1, NULL, ?2, ?3, ?4, ?5)",
            rusqlite::params![timestamp_ms, device_name, hnsw_id as i64, blob, metrics_json],
        );

        hnsw_id
    }

    /// Insert metrics only (no embedding vector).
    pub fn insert_metrics_only(
        &mut self,
        timestamp_ms: i64,
        device_name: Option<&str>,
        metrics: &skill_exg::EpochMetrics,
    ) {
        let metrics_json = serde_json::to_string(metrics).unwrap_or_default();
        let empty_blob: &[u8] = &[];
        let _ = self.conn.execute(
            "INSERT INTO embeddings
             (timestamp, device_id, device_name, hnsw_id, eeg_embedding, metrics_json)
             VALUES (?1, NULL, ?2, 0, ?3, ?4)",
            rusqlite::params![timestamp_ms, device_name, empty_blob, metrics_json],
        );
    }

    /// Persist the HNSW index to disk.
    pub fn save_hnsw(&self) {
        if let Some(ref idx) = self.hnsw {
            if let Err(e) = idx.save(&self.index_path) {
                error!(%e, "failed to save HNSW index");
            }
        }
    }

    pub fn hnsw_len(&self) -> usize {
        self.hnsw_len
    }

    pub fn hnsw_rebuilt(&self) -> bool {
        self.hnsw_rebuilt
    }

    pub fn hnsw_rebuilt_count(&self) -> usize {
        self.hnsw_rebuilt_count
    }
}
