// SPDX-License-Identifier: GPL-3.0-only
//! Generic device session runner — drives any `DeviceAdapter` through the
//! full daemon pipeline: EEG filter, band power DSP, quality monitor,
//! artifact detection, CSV/Parquet recording, EXG embeddings, hooks, WS events.

use std::path::{Path, PathBuf};

use skill_daemon_common::EventEnvelope;
use skill_data::session_writer::{SessionWriter, StorageFormat};
use skill_devices::session::{DeviceAdapter, DeviceEvent};
use skill_eeg::artifact_detection::ArtifactDetector;
use skill_eeg::eeg_bands::BandAnalyzer;
use skill_eeg::eeg_filter::EegFilter;
use skill_eeg::eeg_quality::QualityMonitor;
use skill_settings::HookRule;
use tokio::sync::{broadcast, oneshot};
use tracing::{error, info};

use super::shared::{
    broadcast_event, enrich_band_snapshot, unix_secs, unix_secs_f64, utc_date_dir, write_session_meta,
};
use crate::embed::{EmbedWorkerHandle, EpochAccumulator};
use crate::state::AppState;

// ── Epoch metrics store ──────────────────────────────────────────────────────

struct EpochStore {
    conn: rusqlite::Connection,
}

impl EpochStore {
    fn open(day_dir: &Path) -> Option<Self> {
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

    fn insert_metrics(&self, ts_ms: i64, device_name: Option<&str>, metrics: &skill_exg::EpochMetrics) {
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

struct Pipeline {
    writer: SessionWriter,
    csv_path: PathBuf,
    filter: EegFilter,
    band_analyzer: BandAnalyzer,
    quality: QualityMonitor,
    artifacts: ArtifactDetector,
    epoch_store: Option<EpochStore>,
    epoch_accumulator: Option<EpochAccumulator>,
    _embed_worker: Option<EmbedWorkerHandle>,
    channel_names: Vec<String>,
    sample_rate: f64,
    start_utc: u64,
    device_name: String,
    total_samples: u64,
    flush_counter: u64,
}

impl Pipeline {
    fn open(
        skill_dir: &Path,
        eeg_channels: usize,
        sample_rate: f64,
        channel_names: Vec<String>,
        device_name: String,
        events_tx: broadcast::Sender<EventEnvelope>,
        hooks: Vec<HookRule>,
    ) -> Result<Self, String> {
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
        let writer =
            SessionWriter::open(&csv_path, &labels, storage_format).map_err(|e| format!("SessionWriter open: {e}"))?;

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
        let model_config = skill_eeg::eeg_model_config::load_model_config(skill_dir);
        let embed_worker = EmbedWorkerHandle::spawn(skill_dir.to_path_buf(), model_config, events_tx, hooks);
        let mut acc = EpochAccumulator::new(
            embed_worker.tx.clone(),
            eeg_channels,
            sample_rate as f32,
            channel_names.clone(),
        );
        acc.set_device_name(device_name.clone());

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
            epoch_accumulator: Some(acc),
            _embed_worker: Some(embed_worker),
            channel_names,
            sample_rate,
            start_utc,
            device_name,
            total_samples: 0,
            flush_counter: 0,
        })
    }

    /// Push one EEG frame through the full DSP pipeline.
    /// Returns enriched band snapshot JSON if the band analyzer fired.
    fn push_eeg(&mut self, channels: &[f64], ts: f64) -> Option<serde_json::Value> {
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

    fn channel_quality(&self) -> Vec<skill_eeg::eeg_quality::SignalQuality> {
        self.quality.all_qualities()
    }

    fn finalize(&mut self) {
        self.writer.flush();
        write_session_meta(
            &self.csv_path,
            &self.device_name,
            &self.channel_names,
            self.sample_rate,
            self.start_utc,
            self.total_samples,
        );
        info!(
            path = %self.csv_path.display(),
            samples = self.total_samples,
            "session finalized"
        );
    }
}

// ── Generic session runner ────────────────────────────────────────────────────

/// Run a device session using any `DeviceAdapter`.
///
/// Drives the full pipeline: EEG filter → DSP → quality → artifacts →
/// CSV/Parquet → embeddings → hooks → WS events.
pub(crate) async fn run_adapter_session(
    state: AppState,
    mut cancel_rx: oneshot::Receiver<()>,
    mut adapter: Box<dyn DeviceAdapter>,
) {
    let desc = adapter.descriptor().clone();
    let sample_rate = desc.eeg_sample_rate;
    let device_kind = desc.kind.to_string();
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let hooks = state.hooks.lock().map(|g| g.clone()).unwrap_or_default();

    let mut pipeline: Option<Pipeline> = None;
    let mut sample_count: u64 = 0;

    loop {
        tokio::select! {
            biased;
            _ = &mut cancel_rx => {
                info!("session cancelled");
                adapter.disconnect().await;
                break;
            }
            ev = adapter.next_event() => {
                let Some(ev) = ev else {
                    info!("event stream ended");
                    if let Ok(mut s) = state.status.lock() {
                        s.state = "disconnected".into();
                        s.device_kind.clear();
                    }
                    broadcast_event(&state.events_tx, "DeviceDisconnected", &serde_json::json!({}));
                    break;
                };

                match ev {
                    DeviceEvent::Connected(info) => {
                        info!(name = %info.name, kind = %device_kind, "device connected");
                        if let Ok(mut s) = state.status.lock() {
                            s.state = "connected".into();
                            s.device_name = Some(info.name.clone());
                            s.device_kind = device_kind.clone();
                            s.device_error = None;
                        }
                        broadcast_event(&state.events_tx, "DeviceConnected", &serde_json::json!({
                            "name": info.name,
                            "kind": device_kind,
                        }));

                        match Pipeline::open(
                            &skill_dir,
                            desc.eeg_channels,
                            sample_rate,
                            desc.channel_names.clone(),
                            info.name.clone(),
                            state.events_tx.clone(),
                            hooks.clone(),
                        ) {
                            Ok(p) => pipeline = Some(p),
                            Err(e) => error!(%e, "pipeline open failed"),
                        }
                    }

                    DeviceEvent::Eeg(frame) => {
                        sample_count += 1;
                        if let Ok(mut s) = state.status.lock() {
                            s.sample_count = sample_count;
                        }

                        if let Some(ref mut pipe) = pipeline {
                            if let Some(enriched) = pipe.push_eeg(&frame.channels, frame.timestamp_s) {
                                // Update latest_bands and broadcast.
                                if let Ok(mut bands) = state.latest_bands.lock() {
                                    *bands = Some(enriched.clone());
                                }
                                broadcast_event(&state.events_tx, "EegBands", &enriched);

                                // Broadcast signal quality (~4 Hz cadence, same as bands).
                                let qualities = pipe.channel_quality();
                                let q_vals: Vec<String> = qualities.iter()
                                    .map(|q| format!("{q:?}").to_lowercase())
                                    .collect();
                                broadcast_event(&state.events_tx, "SignalQuality",
                                    &serde_json::json!({ "quality": q_vals }));
                            }
                        }

                        // Batch all channels into a single event per frame
                        // to avoid flooding the broadcast channel (was 32 events
                        // per frame at 256 Hz = 8192 events/sec).
                        broadcast_event(&state.events_tx, "EegSample", &serde_json::json!({
                            "channels": &frame.channels,
                            "timestamp": frame.timestamp_s,
                        }));

                        // Emit full status once per second.
                        let rate = sample_rate.max(1.0) as u64;
                        if sample_count.is_multiple_of(rate) {
                            if let Ok(status) = state.status.lock() {
                                if let Ok(val) = serde_json::to_value(&*status) {
                                    broadcast_event(&state.events_tx, "StatusUpdate", &val);
                                }
                            }
                        }
                    }

                    DeviceEvent::Imu(frame) => {
                        let ts = unix_secs_f64();
                        if let Some(ref mut pipe) = pipeline {
                            pipe.writer.push_imu(
                                &pipe.csv_path, ts,
                                frame.accel, frame.gyro, None,
                            );
                        }
                        broadcast_event(&state.events_tx, "ImuSample", &serde_json::json!({
                            "sensor": "accel", "samples": [frame.accel], "timestamp": ts,
                        }));
                        if let Some(gyro) = frame.gyro {
                            broadcast_event(&state.events_tx, "ImuSample", &serde_json::json!({
                                "sensor": "gyro", "samples": [gyro], "timestamp": ts,
                            }));
                        }
                    }

                    DeviceEvent::Ppg(frame) => {
                        let ts = unix_secs_f64();
                        broadcast_event(&state.events_tx, "PpgSample", &serde_json::json!({
                            "channel": frame.channel,
                            "samples": frame.samples,
                            "timestamp": ts,
                        }));
                    }

                    DeviceEvent::Battery(frame) => {
                        if let Ok(mut s) = state.status.lock() {
                            s.battery = frame.level_pct;
                        }
                        broadcast_event(&state.events_tx, "Battery", &serde_json::json!({
                            "level_pct": frame.level_pct,
                        }));
                    }

                    DeviceEvent::Disconnected => {
                        info!("device disconnected");
                        if let Ok(mut s) = state.status.lock() {
                            s.state = "disconnected".into();
                            s.device_kind.clear();
                        }
                        broadcast_event(&state.events_tx, "DeviceDisconnected", &serde_json::json!({}));
                        break;
                    }

                    _ => {}
                }
            }
        }
    }

    if let Some(ref mut pipe) = pipeline {
        pipe.finalize();
    }
}
