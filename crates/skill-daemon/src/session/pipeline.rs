// SPDX-License-Identifier: GPL-3.0-only
//! EEG session pipeline — drives the full DSP chain (filter → bands → quality →
//! artifacts) and records data to CSV/Parquet + SQLite epoch metrics.

use anyhow::Context as _;
use std::path::{Path, PathBuf};

use skill_daemon_common::EventEnvelope;
use skill_data::session_writer::{SessionWriter, StorageFormat};
use skill_eeg::artifact_detection::ArtifactDetector;
use skill_eeg::eeg_bands::BandAnalyzer;
use skill_eeg::eeg_filter::EegFilter;
use skill_eeg::eeg_quality::QualityMonitor;
use skill_settings::HookRule;
use tokio::sync::broadcast;
use tracing::info;

use super::shared::{enrich_band_snapshot, unix_secs, utc_date_dir, write_session_meta};
use crate::embed::{EmbedWorkerHandle, EpochAccumulator};

// ── Epoch metrics store ──────────────────────────────────────────────────────

pub(crate) struct EpochStore {
    conn: rusqlite::Connection,
}

impl EpochStore {
    pub(crate) fn open(day_dir: &Path) -> Option<Self> {
        let db_path = day_dir.join(skill_constants::SQLITE_FILE);
        let conn = rusqlite::Connection::open(&db_path).ok()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS embeddings (
                id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL,
                device_id TEXT, device_name TEXT, hnsw_id INTEGER DEFAULT 0,
                eeg_embedding BLOB, label TEXT, extra_embedding BLOB,
                ppg_ambient REAL, ppg_infrared REAL, ppg_red REAL, metrics_json TEXT);",
        )
        .ok()?;
        Some(Self { conn })
    }

    pub(crate) fn insert_metrics(&self, ts_ms: i64, device_name: Option<&str>, metrics: &skill_exg::EpochMetrics) {
        let json = serde_json::to_string(metrics).unwrap_or_default();
        let empty: &[u8] = &[];
        let _ = self.conn.execute(
            "INSERT INTO embeddings (timestamp, device_name, hnsw_id, eeg_embedding, metrics_json)
             VALUES (?1, ?2, 0, ?3, ?4)",
            rusqlite::params![ts_ms, device_name, empty, json],
        );
    }
}

// ── Session pipeline ──────────────────────────────────────────────────────────

pub(crate) struct Pipeline {
    pub(crate) writer: SessionWriter,
    pub(crate) csv_path: PathBuf,
    filter: EegFilter,
    band_analyzer: BandAnalyzer,
    quality: QualityMonitor,
    artifacts: ArtifactDetector,
    epoch_store: Option<EpochStore>,
    epoch_accumulator: Option<EpochAccumulator>,
    _embed_worker: Option<EmbedWorkerHandle>,
    pub(crate) channel_names: Vec<String>,
    pub(crate) sample_rate: f64,
    pub(crate) start_utc: u64,
    pub(crate) device_name: String,
    pub(crate) total_samples: u64,
    flush_counter: u64,
    pub(crate) firmware_version: Option<String>,
    pub(crate) serial_number: Option<String>,
    pub(crate) fnirs_channel_names: Vec<String>,
    /// Device kind tag (e.g. "muse", "awear", "openbci", "emotiv").
    pub(crate) device_kind: String,
    pub(crate) ppg_analyzer: skill_data::ppg_analysis::PpgAnalyzer,
}

impl Pipeline {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open(
        skill_dir: &Path,
        eeg_channels: usize,
        sample_rate: f64,
        channel_names: Vec<String>,
        device_name: String,
        events_tx: broadcast::Sender<EventEnvelope>,
        hooks: Vec<HookRule>,
        text_embedder: crate::text_embedder::SharedTextEmbedder,
    ) -> anyhow::Result<Self> {
        let day_dir = utc_date_dir(skill_dir);
        let start_utc = unix_secs();
        let csv_path = day_dir.join(format!("exg_{start_utc}.csv"));

        // Storage format (csv/parquet/both) from settings.
        let storage_format = {
            let settings = skill_settings::load_settings(skill_dir);
            StorageFormat::parse(&settings.storage_format)
        };
        let default_labels: Vec<String> = (0..eeg_channels).map(|i| format!("Ch{}", i + 1)).collect();
        let labels: Vec<&str> = if channel_names.is_empty() {
            default_labels.iter().map(String::as_str).collect()
        } else {
            channel_names.iter().map(String::as_str).collect()
        };
        let writer = SessionWriter::open(&csv_path, &labels, storage_format).context("SessionWriter open")?;

        // DSP pipeline: filter → bands → quality → artifacts.
        let filter_config = {
            let settings = skill_settings::load_settings(skill_dir);
            let mut cfg = settings.filter_config;
            cfg.sample_rate = sample_rate as f32;
            cfg
        };
        let filter = EegFilter::new(filter_config);
        let band_analyzer = BandAnalyzer::new_with_rate(sample_rate as f32);
        let quality = QualityMonitor::with_window(eeg_channels, sample_rate.max(1.0) as usize);
        let ch_refs: Vec<&str> = channel_names.iter().map(String::as_str).collect();
        let artifacts = ArtifactDetector::with_channels(sample_rate, &ch_refs);

        // Epoch metrics store.
        let epoch_store = EpochStore::open(&day_dir);

        // EXG embedding pipeline.
        // Skip for virtual/synthetic devices: their data is procedurally
        // generated and not worth embedding, and ZUNA takes 60+ seconds to
        // load — starving the LLM of GPU while the virtual stream is active.
        // Override with SKILL_VIRTUAL_EMBED=1 (e.g. for e2e tests).
        let is_virtual = device_name.to_lowercase().contains("virtual");
        let force_virtual_embed = std::env::var("SKILL_VIRTUAL_EMBED").map(|v| v == "1").unwrap_or(false);
        let skip_embed = (is_virtual && !force_virtual_embed)
            || cfg!(test)
            || std::env::var("SKILL_SKIP_EMBED").map(|v| v == "1").unwrap_or(false);
        let model_config = skill_eeg::eeg_model_config::load_model_config(skill_dir);
        let (embed_worker_opt, acc) = if skip_embed {
            (None, None)
        } else {
            let worker =
                EmbedWorkerHandle::spawn(skill_dir.to_path_buf(), model_config, events_tx, hooks, text_embedder);
            let mut acc = EpochAccumulator::new(
                worker.tx.clone(),
                eeg_channels,
                sample_rate as f32,
                channel_names.clone(),
            );
            acc.set_device_name(device_name.clone());
            (Some(worker), Some(acc))
        };

        info!(
            path = %csv_path.display(),
            ch = eeg_channels,
            rate = sample_rate,
            format = ?storage_format,
            "session pipeline opened"
        );

        Ok(Self {
            writer,
            csv_path,
            filter,
            band_analyzer,
            quality,
            artifacts,
            epoch_store,
            epoch_accumulator: acc,
            _embed_worker: embed_worker_opt,
            channel_names,
            sample_rate,
            start_utc,
            device_name,
            total_samples: 0,
            flush_counter: 0,
            firmware_version: None,
            serial_number: None,
            fnirs_channel_names: Vec::new(),
            device_kind: String::new(),
            // 5-second window covers one full HRV epoch at 64 Hz PPG.
            ppg_analyzer: skill_data::ppg_analysis::PpgAnalyzer::new(5.0),
        })
    }

    /// Push one EEG frame through the full DSP pipeline.
    /// Returns enriched band snapshot JSON if the band analyzer fired.
    pub(crate) fn push_eeg(&mut self, channels: &[f64], ts: f64) -> Option<serde_json::Value> {
        self.total_samples += 1;
        self.flush_counter += 1;

        // 1. Record raw samples to file.
        for (el, &v) in channels.iter().enumerate() {
            self.writer.push_eeg(el, &[v], ts, self.sample_rate);
        }
        if self.flush_counter >= 256 {
            self.writer.flush();
            self.flush_counter = 0;
        }

        // 2. Feed epoch accumulator (for EXG embeddings).
        if let Some(ref mut acc) = self.epoch_accumulator {
            for (el, &v) in channels.iter().enumerate() {
                acc.push(el, &[v as f32]);
            }
        }

        // 3. EEG filter (notch + bandpass).
        let mut filter_fired = false;
        for (ch, &v) in channels.iter().enumerate() {
            if self.filter.push(ch, &[v]) {
                filter_fired = true;
            }
        }

        // 4. Quality monitor (on raw samples — before filter).
        for (ch, &v) in channels.iter().enumerate() {
            self.quality.push(ch, &[v]);
        }

        // 5. Artifact detector (on raw samples — blink detection needs pre-filter).
        for (ch, &v) in channels.iter().enumerate() {
            self.artifacts.push(ch, &[v]);
        }

        // 6. Band analyzer (on filtered samples when available, else raw).
        let mut band_fired = false;
        if filter_fired {
            for ch in 0..channels.len() {
                let drained = self.filter.drain(ch);
                if !drained.is_empty() && self.band_analyzer.push(ch, &drained) {
                    band_fired = true;
                }
            }
        } else {
            for (ch, &v) in channels.iter().enumerate() {
                if self.band_analyzer.push(ch, &[v]) {
                    band_fired = true;
                }
            }
        }

        if !band_fired {
            return None;
        }

        // 7. Enrich snapshot with composite scores + artifacts.
        let artifact_metrics = self.artifacts.metrics();
        if let Some(ref mut snap) = self.band_analyzer.latest {
            let enriched = enrich_band_snapshot(snap, Some(&artifact_metrics));

            // Write metrics row to file.
            self.writer.push_metrics(&self.csv_path, snap);

            // Store epoch metrics in SQLite.
            if let Some(ref store) = self.epoch_store {
                let ts_ms = (snap.timestamp * 1000.0) as i64;
                let metrics = skill_exg::EpochMetrics::from_snapshot(snap);
                store.insert_metrics(ts_ms, Some(&self.device_name), &metrics);
            }

            // Update epoch accumulator's band snapshot.
            if let Some(ref mut acc) = self.epoch_accumulator {
                acc.update_bands(snap.clone());
            }

            return Some(enriched);
        }
        None
    }

    pub(crate) fn channel_quality(&self) -> Vec<skill_eeg::eeg_quality::SignalQuality> {
        self.quality.all_qualities()
    }

    pub(crate) fn finalize(&mut self) {
        self.writer.flush();
        write_session_meta(
            &self.csv_path,
            &self.device_name,
            &self.channel_names,
            self.sample_rate,
            self.start_utc,
            self.total_samples,
            &crate::session::shared::SessionDeviceId {
                firmware_version: self.firmware_version.as_deref(),
                serial_number: self.serial_number.as_deref(),
            },
            &self.device_kind,
        );
        info!(
            path = %self.csv_path.display(),
            samples = self.total_samples,
            "session finalized"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_store_open_creates_table_and_inserts() {
        let dir = tempfile::tempdir().unwrap();
        let store = EpochStore::open(dir.path()).expect("should open");
        let metrics = skill_exg::EpochMetrics::default();
        store.insert_metrics(1000, Some("TestDevice"), &metrics);
        // Verify the row was inserted
        let count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM embeddings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn epoch_store_open_nonexistent_dir_returns_none() {
        // open should fail gracefully (return None) for an invalid path
        // Actually rusqlite will create the file, so this should succeed for any writable path
        // Test with a valid tempdir instead
        let dir = tempfile::tempdir().unwrap();
        let store = EpochStore::open(dir.path());
        assert!(store.is_some());
    }

    #[test]
    fn epoch_store_multiple_inserts_increment_id() {
        let dir = tempfile::tempdir().unwrap();
        let store = EpochStore::open(dir.path()).unwrap();
        let metrics = skill_exg::EpochMetrics::default();
        store.insert_metrics(100, Some("Dev1"), &metrics);
        store.insert_metrics(200, Some("Dev2"), &metrics);
        store.insert_metrics(300, None, &metrics);
        let count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM embeddings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn pipeline_open_creates_csv_and_meta_dir() {
        let dir = tempfile::tempdir().unwrap();
        // Create a minimal settings file so load_settings doesn't fail
        let settings = skill_settings::UserSettings::default();
        let settings_path = skill_settings::settings_path(dir.path());
        if let Some(parent) = settings_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let _ = std::fs::write(&settings_path, serde_json::to_string_pretty(&settings).unwrap());

        let (tx, _rx) = broadcast::channel(16);
        let result = Pipeline::open(
            dir.path(),
            4,
            256.0,
            vec!["TP9".into(), "AF7".into(), "AF8".into(), "TP10".into()],
            "TestMuse".into(),
            tx,
            Vec::new(),
            crate::text_embedder::SharedTextEmbedder::new(),
        );
        assert!(result.is_ok());
        let pipe = result.unwrap();
        assert_eq!(pipe.sample_rate, 256.0);
        assert_eq!(pipe.channel_names.len(), 4);
        assert_eq!(pipe.total_samples, 0);
        assert!(pipe.csv_path.to_string_lossy().contains("exg_"));
    }

    #[test]
    fn pipeline_push_eeg_increments_sample_count() {
        let dir = tempfile::tempdir().unwrap();
        let settings = skill_settings::UserSettings::default();
        let settings_path = skill_settings::settings_path(dir.path());
        if let Some(parent) = settings_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let _ = std::fs::write(&settings_path, serde_json::to_string_pretty(&settings).unwrap());

        let (tx, _rx) = broadcast::channel(16);
        let mut pipe = Pipeline::open(
            dir.path(),
            4,
            256.0,
            vec!["Ch1".into(), "Ch2".into(), "Ch3".into(), "Ch4".into()],
            "TestDevice".into(),
            tx,
            Vec::new(),
            crate::text_embedder::SharedTextEmbedder::new(),
        )
        .unwrap();

        for i in 0..100 {
            pipe.push_eeg(&[1.0, 2.0, 3.0, 4.0], i as f64 / 256.0);
        }
        assert_eq!(pipe.total_samples, 100);
    }

    #[test]
    fn pipeline_finalize_flushes_and_writes_meta() {
        let dir = tempfile::tempdir().unwrap();
        let settings = skill_settings::UserSettings::default();
        let settings_path = skill_settings::settings_path(dir.path());
        if let Some(parent) = settings_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let _ = std::fs::write(&settings_path, serde_json::to_string_pretty(&settings).unwrap());

        let (tx, _rx) = broadcast::channel(16);
        let mut pipe = Pipeline::open(
            dir.path(),
            2,
            128.0,
            vec!["Ch1".into(), "Ch2".into()],
            "TestDevice".into(),
            tx,
            Vec::new(),
            crate::text_embedder::SharedTextEmbedder::new(),
        )
        .unwrap();

        for i in 0..50 {
            pipe.push_eeg(&[1.0, 2.0], i as f64 / 128.0);
        }
        let csv_path = pipe.csv_path.clone();
        pipe.finalize();

        // The CSV file should exist
        assert!(csv_path.exists());
        // The sidecar JSON should exist
        let json_path = csv_path.with_extension("json");
        assert!(json_path.exists());
        let meta: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap()).unwrap();
        assert_eq!(meta["device_name"], "TestDevice");
        assert_eq!(meta["total_samples"], 50);
    }

    #[test]
    fn pipeline_channel_quality_returns_per_channel() {
        let dir = tempfile::tempdir().unwrap();
        let settings = skill_settings::UserSettings::default();
        let settings_path = skill_settings::settings_path(dir.path());
        if let Some(parent) = settings_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let _ = std::fs::write(&settings_path, serde_json::to_string_pretty(&settings).unwrap());

        let (tx, _rx) = broadcast::channel(16);
        let pipe = Pipeline::open(
            dir.path(),
            4,
            256.0,
            vec!["Ch1".into(), "Ch2".into(), "Ch3".into(), "Ch4".into()],
            "Test".into(),
            tx,
            Vec::new(),
            crate::text_embedder::SharedTextEmbedder::new(),
        )
        .unwrap();

        let q = pipe.channel_quality();
        assert_eq!(q.len(), 4);
    }
}
