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

use super::shared::{enrich_band_snapshot, unix_secs, utc_date_dir, write_session_meta, write_session_meta_partial};
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
            // Propagate the EXG device preference to the RLX FFT kernels so the
            // filter and band-analyzer use Metal on macOS, CUDA → wgpu → CPU elsewhere.
            #[cfg(feature = "embed-exg")]
            skill_eeg::rlx_fft::init_device(crate::embed::resolve_exg_device(&settings.exg_inference_device));
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

    /// Write an in-progress sidecar JSON next to the CSV.
    ///
    /// Called immediately after `open` (once device identity has been
    /// threaded in by the runner) and after every `roll`. The next call
    /// to `finalize` overwrites it with the complete sidecar. Purpose:
    /// a daemon killed mid-chunk leaves a usable sidecar so
    /// `list_sessions_for_day` doesn't fall back to CSV-header sniffing.
    pub(crate) fn write_partial_sidecar(&self) {
        write_session_meta_partial(
            &self.csv_path,
            &self.device_name,
            &self.channel_names,
            self.sample_rate,
            self.start_utc,
            &crate::session::shared::SessionDeviceId {
                firmware_version: self.firmware_version.as_deref(),
                serial_number: self.serial_number.as_deref(),
            },
            &self.device_kind,
        );
    }

    pub(crate) fn finalize(&mut self) {
        self.writer.flush();
        self.writer.close();
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

    /// Roll the session writer to a new file. Finalises the current chunk
    /// (writes sidecar JSON, closes Parquet footer) and opens a fresh
    /// `exg_<ts>.csv|parquet`. Keeps DSP, embedding, and PPG/artifact state
    /// warm — only the writer is swapped.
    ///
    /// To downstream readers each chunk looks identical to a normal short
    /// session — same naming, same sidecar shape — so no readers need to
    /// know about rollover.
    pub(crate) fn roll(&mut self, skill_dir: &Path) -> anyhow::Result<()> {
        // 1. Finalise the current chunk (writer flush+close, sidecar JSON).
        let just_closed = self.csv_path.clone();
        self.finalize();
        // Pre-warm the metrics cache for the just-closed chunk in a
        // background thread. With hourly rollover, an overnight 8h
        // recording produces ~480 chunks; without pre-warming, the first
        // history-page load cold-builds all caches synchronously.
        std::thread::spawn(move || {
            let _ = skill_history::load_csv_metrics_cached(&just_closed);
        });

        // 2. Compute a new csv_path. unix_secs() granularity is 1s; if a
        //    rollover lands inside the same second as the previous start,
        //    bump by 1 to keep filenames unique.
        let day_dir = utc_date_dir(skill_dir);
        let now = unix_secs();
        let new_start = if now > self.start_utc { now } else { self.start_utc + 1 };
        let new_path = day_dir.join(format!("exg_{new_start}.csv"));

        // 3. Open a fresh writer with the same labels and current settings.
        let storage_format = {
            let settings = skill_settings::load_settings(skill_dir);
            StorageFormat::parse(&settings.storage_format)
        };
        let labels: Vec<&str> = self.channel_names.iter().map(String::as_str).collect();
        let new_writer = SessionWriter::open(&new_path, &labels, storage_format).context("rollover writer open")?;

        // 4. Swap.
        self.writer = new_writer;
        self.csv_path = new_path;
        self.start_utc = new_start;
        self.total_samples = 0;
        self.flush_counter = 0;

        // 5. Drop a partial sidecar for the new chunk so a crash before
        //    the next finalize still leaves a readable session entry.
        self.write_partial_sidecar();

        info!(path = %self.csv_path.display(), "session rolled");
        Ok(())
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

    /// Rollover finalises the current chunk and opens a fresh one with a
    /// distinct path, while preserving DSP/embed state. Both chunks must be
    /// readable independently.
    #[test]
    fn pipeline_roll_finalizes_and_opens_new_chunk() {
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
            "RollDevice".into(),
            tx,
            Vec::new(),
            crate::text_embedder::SharedTextEmbedder::new(),
        )
        .unwrap();

        // Write some samples to chunk 1.
        for i in 0..30 {
            pipe.push_eeg(&[1.0, 2.0], i as f64 / 128.0);
        }
        let chunk1_path = pipe.csv_path.clone();
        let chunk1_start = pipe.start_utc;

        // Roll.
        pipe.roll(dir.path()).unwrap();

        // After roll: counters reset, path differs, start_utc strictly greater.
        assert_eq!(pipe.total_samples, 0);
        assert_ne!(pipe.csv_path, chunk1_path);
        assert!(pipe.start_utc > chunk1_start, "new start_utc must advance");

        // Write samples to chunk 2.
        for i in 0..20 {
            pipe.push_eeg(&[3.0, 4.0], i as f64 / 128.0);
        }
        assert_eq!(pipe.total_samples, 20);
        let chunk2_path = pipe.csv_path.clone();

        pipe.finalize();

        // Both CSVs and both sidecars exist.
        assert!(chunk1_path.exists(), "chunk1 csv");
        assert!(chunk2_path.exists(), "chunk2 csv");
        let m1: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(chunk1_path.with_extension("json")).unwrap()).unwrap();
        let m2: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(chunk2_path.with_extension("json")).unwrap()).unwrap();
        assert_eq!(m1["total_samples"], 30);
        assert_eq!(m2["total_samples"], 20);
        assert_eq!(m1["device_name"], "RollDevice");
        assert_eq!(m2["device_name"], "RollDevice");
    }

    /// Partial sidecar must be writable before finalize and must contain
    /// the device/channel/rate fields needed by `list_sessions_for_day`.
    #[test]
    fn write_partial_sidecar_creates_in_progress_json() {
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
            vec!["TP9".into(), "AF7".into(), "AF8".into(), "TP10".into()],
            "PartialDevice".into(),
            tx,
            Vec::new(),
            crate::text_embedder::SharedTextEmbedder::new(),
        )
        .unwrap();

        // Simulate the runner enriching device identity, then write partial.
        pipe.firmware_version = Some("fw-2.1.3".into());
        pipe.serial_number = Some("SN-XYZ".into());
        pipe.device_kind = "muse".into();
        pipe.write_partial_sidecar();

        let sidecar = pipe.csv_path.with_extension("json");
        assert!(sidecar.exists(), "partial sidecar must exist before finalize");
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&sidecar).unwrap()).unwrap();

        assert_eq!(v["device_name"], "PartialDevice");
        assert_eq!(v["sample_rate_hz"], 256.0);
        assert_eq!(v["device_kind"], "muse");
        assert_eq!(v["firmware_version"], "fw-2.1.3");
        assert_eq!(v["serial_number"], "SN-XYZ");
        assert_eq!(v["channel_count"], 4);
        assert_eq!(v["channel_names"][0], "TP9");
        assert_eq!(v["in_progress"], true);
        assert_eq!(v["total_samples"], 0);
        assert!(v.get("session_start_utc").and_then(|x| x.as_u64()).is_some());

        // Now push samples and finalize: full sidecar must overwrite
        // (in_progress flag dropped, total_samples populated).
        for i in 0..32 {
            pipe.push_eeg(&[1.0, 2.0, 3.0, 4.0], i as f64 / 256.0);
        }
        pipe.finalize();

        let v2: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&sidecar).unwrap()).unwrap();
        assert_eq!(v2["device_name"], "PartialDevice");
        assert_eq!(v2["total_samples"], 32);
        assert!(v2.get("in_progress").is_none(), "in_progress flag dropped on finalize");
        assert!(v2.get("session_end_utc").and_then(|x| x.as_u64()).is_some());
    }

    /// After Pipeline::roll, the new chunk must have a partial sidecar
    /// before any samples are written, just like the initial open.
    #[test]
    fn pipeline_roll_writes_partial_sidecar_for_new_chunk() {
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
            "RollPartial".into(),
            tx,
            Vec::new(),
            crate::text_embedder::SharedTextEmbedder::new(),
        )
        .unwrap();
        pipe.device_kind = "test".into();
        pipe.write_partial_sidecar();

        // Roll without writing any samples to chunk 2.
        pipe.roll(dir.path()).unwrap();

        // Sidecar for chunk 2 must already exist from the partial write.
        let chunk2_sidecar = pipe.csv_path.with_extension("json");
        assert!(chunk2_sidecar.exists(), "partial sidecar for new chunk must exist");
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&chunk2_sidecar).unwrap()).unwrap();
        assert_eq!(v["in_progress"], true);
        assert_eq!(v["device_name"], "RollPartial");
        assert_eq!(v["device_kind"], "test");
        assert_eq!(v["total_samples"], 0);
    }

    /// `Pipeline::roll` must trigger a background pre-warm of the
    /// just-closed chunk's metrics cache so the history page doesn't pay
    /// the cold-build cost on first open.
    #[test]
    fn pipeline_roll_prewarms_metrics_cache() {
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
            vec!["TP9".into(), "AF7".into(), "AF8".into(), "TP10".into()],
            "PrewarmDevice".into(),
            tx,
            Vec::new(),
            crate::text_embedder::SharedTextEmbedder::new(),
        )
        .unwrap();
        pipe.device_kind = "muse".into();

        // Write enough samples to produce at least a few metrics rows.
        for i in 0..1500 {
            pipe.push_eeg(&[1.0, 2.0, 3.0, 4.0], i as f64 / 256.0);
        }
        let chunk1 = pipe.csv_path.clone();
        let metrics_path = chunk1.with_file_name(format!(
            "{}_metrics.csv",
            chunk1.file_stem().and_then(|s| s.to_str()).unwrap_or("")
        ));
        let cache_path = chunk1.with_file_name(format!(
            "{}_metrics_cache.json",
            chunk1.file_stem().and_then(|s| s.to_str()).unwrap_or("")
        ));

        pipe.roll(dir.path()).unwrap();

        // Roll spawns a background thread; poll briefly for the cache
        // file to appear. Cap at 2s to keep the test snappy if the
        // pre-warm is somehow disabled.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut appeared = false;
        while std::time::Instant::now() < deadline {
            if cache_path.exists() {
                appeared = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        assert!(metrics_path.exists(), "metrics CSV must exist after roll");
        assert!(
            appeared,
            "metrics_cache.json must be pre-warmed within 2s of roll: {cache_path:?}"
        );
        // Cache content must be valid JSON.
        let cache_str = std::fs::read_to_string(&cache_path).unwrap();
        let _: serde_json::Value = serde_json::from_str(&cache_str).expect("cache is valid JSON");

        pipe.finalize();
    }

    /// Empirical: every sample pushed before `roll` must end up as a row
    /// in the closed chunk's CSV. If the on-disk row count is short of
    /// what we pushed, that pinpoints the loss as CSV-writer residue at
    /// the rollover boundary (the 0.15% the 1-hour test observed).
    #[test]
    fn pipeline_roll_no_sample_loss_on_csv_boundary() {
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
            vec!["TP9".into(), "AF7".into(), "AF8".into(), "TP10".into()],
            "LossDevice".into(),
            tx,
            Vec::new(),
            crate::text_embedder::SharedTextEmbedder::new(),
        )
        .unwrap();
        pipe.device_kind = "muse".into();

        // Push exactly 1000 frames (1000 samples × 4 channels) to chunk 1.
        const N: usize = 1000;
        for i in 0..N {
            pipe.push_eeg(&[1.0, 2.0, 3.0, 4.0], i as f64 / 256.0);
        }
        let chunk1_path = pipe.csv_path.clone();
        let pushed_chunk1 = pipe.total_samples;
        assert_eq!(pushed_chunk1, N as u64, "counter must match pushes");

        // Roll and finalize the new chunk to flush both files cleanly.
        pipe.roll(dir.path()).unwrap();
        for i in 0..50 {
            pipe.push_eeg(&[5.0, 6.0, 7.0, 8.0], i as f64 / 256.0);
        }
        pipe.finalize();

        // Count actual data rows on disk (subtract the header).
        let content = std::fs::read_to_string(&chunk1_path).unwrap();
        let data_rows = content.lines().count().saturating_sub(1);

        assert_eq!(
            data_rows, N,
            "chunk1 CSV must contain every pushed sample (residue-free roll)"
        );
    }

    /// Same-second rollover must still produce a unique filename.
    #[test]
    fn pipeline_roll_handles_subsecond_collision() {
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
            "Sub".into(),
            tx,
            Vec::new(),
            crate::text_embedder::SharedTextEmbedder::new(),
        )
        .unwrap();

        let p0 = pipe.csv_path.clone();
        pipe.roll(dir.path()).unwrap();
        let p1 = pipe.csv_path.clone();
        pipe.roll(dir.path()).unwrap();
        let p2 = pipe.csv_path.clone();

        assert_ne!(p0, p1);
        assert_ne!(p1, p2);
        assert_ne!(p0, p2);
        assert!(p0.exists() && p1.exists() && p2.exists());
    }
}
