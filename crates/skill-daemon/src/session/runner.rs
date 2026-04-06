// SPDX-License-Identifier: GPL-3.0-only
//! Generic device session runner — drives any `DeviceAdapter` through the
//! full daemon pipeline: CSV recording, DSP band power, EXG embeddings,
//! hook triggers, WS event broadcast.

use std::path::{Path, PathBuf};

use skill_daemon_common::EventEnvelope;
use skill_data::session_writer::{SessionWriter, StorageFormat};
use skill_devices::session::{DeviceAdapter, DeviceEvent};
use skill_eeg::eeg_bands::BandAnalyzer;
use skill_settings::HookRule;
use tokio::sync::{broadcast, oneshot};
use tracing::{error, info};

use crate::embed::{EmbedWorkerHandle, EpochAccumulator};
use crate::state::AppState;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn unix_secs_f64() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn utc_date_dir(skill_dir: &Path) -> PathBuf {
    let secs = unix_secs();
    let days = secs / 86400;
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let dir = skill_dir.join(format!("{y:04}{m:02}{d:02}"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn write_session_meta(
    csv_path: &Path,
    device_name: &str,
    channel_names: &[String],
    sample_rate: f64,
    start_utc: u64,
    total_samples: u64,
) {
    let meta = serde_json::json!({
        "session_start_utc": start_utc,
        "session_end_utc": unix_secs(),
        "device_name": device_name,
        "channel_names": channel_names,
        "sample_rate": sample_rate,
        "total_samples": total_samples,
        "csv_file": csv_path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        "daemon": true,
    });
    let meta_path = csv_path.with_extension("json");
    if let Ok(json) = serde_json::to_string_pretty(&meta) {
        let _ = std::fs::write(meta_path, json);
    }
}

fn broadcast_event(tx: &broadcast::Sender<EventEnvelope>, event_type: &str, payload: &serde_json::Value) {
    let _ = tx.send(EventEnvelope {
        r#type: event_type.to_string(),
        ts_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        correlation_id: None,
        payload: payload.clone(),
    });
}

fn enrich_band_snapshot(snap: &skill_eeg::eeg_bands::BandSnapshot) -> serde_json::Value {
    let mut val = serde_json::to_value(snap).unwrap_or_default();
    if let Some(obj) = val.as_object_mut() {
        let engage_raw = skill_devices::compute_engagement_raw(snap);
        let focus = skill_devices::focus_score(engage_raw);
        let nch = snap.channels.len().max(1) as f64;
        let avg_alpha = snap.channels.iter().map(|c| c.rel_alpha as f64).sum::<f64>() / nch;
        let avg_beta = snap.channels.iter().map(|c| c.rel_beta as f64).sum::<f64>() / nch;
        let relaxation = if (avg_alpha + avg_beta) > 0.0 {
            (avg_alpha / (avg_alpha + avg_beta)) * 100.0
        } else {
            0.0
        };
        let engagement = 100.0 / (1.0 + (-2.0 * (engage_raw as f64 - 0.8)).exp());
        obj.insert("focus".into(), serde_json::json!(focus));
        obj.insert("relaxation".into(), serde_json::json!(relaxation));
        obj.insert("engagement".into(), serde_json::json!(engagement));
    }
    val
}

// ── Epoch store (metrics-only SQLite fallback) ──────────────────────────────

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
            "INSERT INTO embeddings (timestamp, device_name, hnsw_id, eeg_embedding, metrics_json) VALUES (?1, ?2, 0, ?3, ?4)",
            rusqlite::params![ts_ms, device_name, empty, json],
        );
    }
}

// ── Session pipeline ────────────────────────────────────────────────────────

struct Pipeline {
    writer: SessionWriter,
    csv_path: PathBuf,
    band_analyzer: BandAnalyzer,
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

        let labels: Vec<&str> = channel_names.iter().map(String::as_str).collect();
        let default_labels: Vec<String> = (0..eeg_channels).map(|i| format!("Ch{}", i + 1)).collect();
        let refs: Vec<&str> = if labels.is_empty() {
            default_labels.iter().map(String::as_str).collect()
        } else {
            labels
        };

        // Read user's storage format preference (csv / parquet / both).
        let storage_format = {
            let settings = skill_settings::load_settings(skill_dir);
            StorageFormat::parse(&settings.storage_format)
        };
        let writer =
            SessionWriter::open(&csv_path, &refs, storage_format).map_err(|e| format!("SessionWriter open: {e}"))?;
        let band_analyzer = BandAnalyzer::new_with_rate(sample_rate as f32);
        let epoch_store = EpochStore::open(&day_dir);

        let model_config = skill_eeg::eeg_model_config::load_model_config(skill_dir);
        let embed_worker = EmbedWorkerHandle::spawn(skill_dir.to_path_buf(), model_config, events_tx, hooks);
        let mut epoch_acc = EpochAccumulator::new(
            embed_worker.tx.clone(),
            eeg_channels,
            sample_rate as f32,
            channel_names.clone(),
        );
        epoch_acc.set_device_name(device_name.clone());

        info!(path = %csv_path.display(), ch = eeg_channels, rate = sample_rate, "session opened");

        Ok(Self {
            writer,
            csv_path,
            band_analyzer,
            epoch_store,
            epoch_accumulator: Some(epoch_acc),
            _embed_worker: Some(embed_worker),
            channel_names,
            sample_rate,
            start_utc,
            device_name,
            total_samples: 0,
            flush_counter: 0,
        })
    }

    fn push_eeg(&mut self, channels: &[f64], ts: f64) -> Option<skill_eeg::eeg_bands::BandSnapshot> {
        self.total_samples += 1;
        self.flush_counter += 1;

        for (el, &v) in channels.iter().enumerate() {
            self.writer.push_eeg(el, &[v], ts, self.sample_rate);
        }
        if self.flush_counter >= 256 {
            self.writer.flush();
            self.flush_counter = 0;
        }

        // Feed epoch accumulator
        if let Some(ref mut acc) = self.epoch_accumulator {
            for (el, &v) in channels.iter().enumerate() {
                acc.push(el, &[v as f32]);
            }
        }

        // DSP
        let mut new_snap = false;
        for (ch, &v) in channels.iter().enumerate() {
            if self.band_analyzer.push(ch, &[v]) {
                new_snap = true;
            }
        }

        if new_snap {
            if let Some(ref snap) = self.band_analyzer.latest {
                self.writer.push_metrics(&self.csv_path, snap);
                if let Some(ref store) = self.epoch_store {
                    let ts_ms = (snap.timestamp * 1000.0) as i64;
                    let metrics = skill_exg::EpochMetrics::from_snapshot(snap);
                    store.insert_metrics(ts_ms, Some(&self.device_name), &metrics);
                }
                if let Some(ref mut acc) = self.epoch_accumulator {
                    acc.update_bands(snap.clone());
                }
            }
            self.band_analyzer.latest.clone()
        } else {
            None
        }
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
        info!(path = %self.csv_path.display(), samples = self.total_samples, "session finalized");
    }
}

// ── Generic session runner ──────────────────────────────────────────────────

/// Run a device session using any `DeviceAdapter`.
///
/// This is the daemon equivalent of the old Tauri `run_device_session`.
/// Drives the full pipeline: CSV, DSP, embeddings, hooks, WS events.
pub(crate) async fn run_adapter_session(
    state: AppState,
    mut cancel_rx: oneshot::Receiver<()>,
    mut adapter: Box<dyn DeviceAdapter>,
) {
    let desc = adapter.descriptor().clone();
    let sample_rate = desc.eeg_sample_rate;
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
                    }
                    broadcast_event(&state.events_tx, "DeviceDisconnected", &serde_json::json!({}));
                    break;
                };

                match ev {
                    DeviceEvent::Connected(info) => {
                        info!(name = %info.name, "device connected");
                        if let Ok(mut s) = state.status.lock() {
                            s.state = "connected".into();
                            s.device_name = Some(info.name.clone());
                            s.device_error = None;
                        }
                        broadcast_event(&state.events_tx, "DeviceConnected", &serde_json::json!({ "name": info.name }));

                        // Open pipeline
                        let ch_names = desc.channel_names.clone();
                        let eeg_ch = desc.eeg_channels;
                        match Pipeline::open(
                            &skill_dir, eeg_ch, sample_rate, ch_names,
                            info.name.clone(), state.events_tx.clone(), hooks.clone(),
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
                            if let Some(snap) = pipe.push_eeg(&frame.channels, frame.timestamp_s) {
                                let enriched = enrich_band_snapshot(&snap);
                                if let Ok(mut bands) = state.latest_bands.lock() {
                                    *bands = Some(enriched.clone());
                                }
                                broadcast_event(&state.events_tx, "EegBands", &enriched);
                            }
                        }

                        for (el, &v) in frame.channels.iter().enumerate() {
                            broadcast_event(&state.events_tx, "EegSample", &serde_json::json!({
                                "electrode": el, "samples": [v], "timestamp": frame.timestamp_s,
                            }));
                        }
                    }

                    DeviceEvent::Imu(frame) => {
                        let ts = unix_secs_f64();
                        // Record IMU to file.
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
                            "channel": frame.channel, "samples": frame.samples, "timestamp": ts,
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
                        }
                        broadcast_event(&state.events_tx, "DeviceDisconnected", &serde_json::json!({}));
                        break;
                    }

                    _ => {} // fNIRS, Meta, etc.
                }
            }
        }
    }

    if let Some(ref mut pipe) = pipeline {
        pipe.finalize();
    }
}
