// SPDX-License-Identifier: GPL-3.0-only
//! Per-day HNSW + SQLite store for EEG embeddings.

use std::path::{Path, PathBuf};

use tracing::{error, info};

/// Per-day storage for embeddings (SQLite) + ANN index (HNSW).
pub(super) struct DayStore {
    pub conn: rusqlite::Connection,
    pub hnsw: Option<fast_hnsw::labeled::LabeledIndex<fast_hnsw::distance::Cosine, i64>>,
    pub index_path: PathBuf,
    pub db_path: PathBuf,
    hnsw_len: usize,
}

impl DayStore {
    /// Open or create the day store for `date_dir` (e.g. `~/.skill/20260406/`).
    pub fn open(day_dir: &Path, hnsw_m: usize, hnsw_ef_construction: usize) -> Option<Self> {
        let db_path = day_dir.join(skill_constants::SQLITE_FILE);
        let index_path = day_dir.join("exg_embeddings.hnsw");

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
        let (hnsw, hnsw_len) = if index_path.exists() {
            match fast_hnsw::labeled::LabeledIndex::<fast_hnsw::distance::Cosine, i64>::load(
                &index_path,
                fast_hnsw::distance::Cosine,
            ) {
                Ok(idx) => {
                    let len = idx.len();
                    info!(len, path = %index_path.display(), "loaded existing HNSW index");
                    (Some(idx), len)
                }
                Err(e) => {
                    error!(%e, "failed to load HNSW index, creating new");
                    let cfg = fast_hnsw::hnsw::Config {
                        m: hnsw_m,
                        ef_construction: hnsw_ef_construction,
                        ..Default::default()
                    };
                    let idx = fast_hnsw::labeled::LabeledIndex::new(cfg, fast_hnsw::distance::Cosine);
                    (Some(idx), 0)
                }
            }
        } else {
            let cfg = fast_hnsw::hnsw::Config {
                m: hnsw_m,
                ef_construction: hnsw_ef_construction,
                ..Default::default()
            };
            let idx = fast_hnsw::labeled::LabeledIndex::new(cfg, fast_hnsw::distance::Cosine);
            (Some(idx), 0)
        };

        Some(Self {
            conn,
            hnsw,
            index_path,
            db_path,
            hnsw_len,
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
}
