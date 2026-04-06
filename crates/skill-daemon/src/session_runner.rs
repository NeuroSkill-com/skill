// SPDX-License-Identifier: GPL-3.0-only
//! OpenBCI session runner — bridges board drivers into the daemon event stream.
//!
//! When `control_start_session` receives `target = "openbci"`, it spawns a
//! background task via [`spawn_openbci_session`].  That task:
//!
//! 1. Reads the persisted [`OpenBciConfig`] to determine the board type and
//!    serial port / WiFi IP / BLE scan settings.
//! 2. Creates the appropriate board driver ([`CytonBoard`], [`CytonDaisyBoard`],
//!    [`GanglionBoard`], etc.) and calls `prepare()` + `start_stream()`.
//! 3. Wraps the stream in [`OpenBciAdapter`] and pumps [`DeviceEvent`]s into
//!    the daemon's broadcast channel as [`EventEnvelope`]s.
//! 4. On disconnect or cancellation the board is released cleanly.

use std::path::{Path, PathBuf};
use std::time::Duration;

use skill_daemon_common::EventEnvelope;
use skill_data::session_csv::CsvState;
use skill_devices::openbci::board::Board;
use skill_devices::session::openbci::OpenBciAdapter;
use skill_devices::session::{DeviceAdapter, DeviceEvent, DeviceInfo};
use skill_eeg::eeg_bands::BandAnalyzer;
use skill_settings::OpenBciConfig;
use tokio::sync::{broadcast, oneshot};
use tracing::{error, info};

use crate::embed::{EmbedWorkerHandle, EpochAccumulator};

#[cfg(target_os = "windows")]
use tracing::warn;

use crate::state::AppState;

/// Handle returned to the caller so the session can be cancelled.
pub struct SessionHandle {
    pub cancel_tx: oneshot::Sender<()>,
}

/// Spawn an OpenBCI session task.  Returns a handle that can cancel it.
pub fn spawn_openbci_session(state: AppState) -> SessionHandle {
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();

    let state2 = state.clone();
    tokio::task::spawn(async move {
        if let Err(e) = run_openbci_session(state2.clone(), cancel_rx).await {
            error!(%e, "openbci session failed");
            if let Ok(mut status) = state2.status.lock() {
                status.state = "disconnected".to_string();
                status.device_error = Some(e.to_string());
            }
        }
        // Clear the session handle so the next start_session doesn't try to
        // cancel a dead task.
        if let Ok(mut slot) = state2.session_handle.lock() {
            *slot = None;
        }
    });

    SessionHandle { cancel_tx }
}

async fn run_openbci_session(state: AppState, mut cancel_rx: oneshot::Receiver<()>) -> Result<(), String> {
    // 1. Load config
    let config = {
        let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
        skill_settings::load_settings(&skill_dir).openbci
    };

    info!(board = ?config.board, serial_port = %config.serial_port, "starting openbci session");

    // 2. Create board + prepare + start_stream (blocking I/O)
    //
    // The board object owns the serial port / BLE connection / TCP socket.
    // We must keep it alive for the duration of the session and call
    // `release()` on exit so the port is freed (especially on Windows
    // where unreleased COM ports stay locked until process exit).
    //
    // Retry up to 3 times with increasing delays.  On Windows the FTDI
    // driver can hold the COM port locked for several seconds after a
    // hot-unplug/replug cycle, and `serialport::new().open()` returns
    // "Access denied" or "file not found" during that window.
    let (adapter, board) = {
        let mut last_err = String::new();
        let mut result = None;
        for attempt in 1..=3u32 {
            let cfg = config.clone();
            match tokio::task::spawn_blocking(move || create_and_start_board(&cfg))
                .await
                .map_err(|e| format!("spawn_blocking join error: {e}"))?
            {
                Ok(pair) => {
                    result = Some(pair);
                    break;
                }
                Err(e) => {
                    last_err = e.clone();
                    if attempt < 3 {
                        let delay_ms = attempt as u64 * 1500;
                        info!(attempt, delay_ms, err = %e, "board setup failed, retrying");
                        if let Ok(mut status) = state.status.lock() {
                            status.retry_attempt = attempt;
                            status.retry_countdown_secs = (delay_ms / 1000) as u32;
                            status.device_error = Some(format!("Retry {attempt}/3: {e}"));
                        }
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }
        result.ok_or_else(|| format!("board setup failed after 3 attempts: {last_err}"))?
    };

    // 3. Update status to "connected" (the frontend state for active sessions)
    if let Ok(mut status) = state.status.lock() {
        status.state = "connected".to_string();
        status.device_error = None;
    }

    // 4. Pump events (CSV recording + DSP + broadcast)
    let (eeg_channels, sample_rate, channel_names) = {
        use skill_settings::OpenBciBoard;
        let (ch, rate): (usize, f64) = match config.board {
            OpenBciBoard::Cyton => (8, 250.0),
            OpenBciBoard::CytonDaisy => (16, 250.0),
            OpenBciBoard::CytonWifi => (8, 1000.0),
            OpenBciBoard::CytonDaisyWifi => (16, 125.0),
            OpenBciBoard::Ganglion => (4, 200.0),
            OpenBciBoard::GanglionWifi => (4, 200.0),
            OpenBciBoard::Galea => (24, 250.0),
        };
        let names: Vec<String> = (0..ch)
            .map(|i| {
                config
                    .channel_labels
                    .get(i)
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("Ch{}", i + 1))
            })
            .collect();
        (ch, rate, names)
    };
    pump_events(
        adapter,
        &state,
        &mut cancel_rx,
        eeg_channels,
        sample_rate,
        channel_names,
    )
    .await;

    // 5. Release the board (frees the serial port / BLE / TCP socket).
    //    Must run on a blocking thread because Board::release() does I/O.
    tokio::task::spawn_blocking(move || {
        let mut board = board;
        if let Err(e) = board.release() {
            tracing::warn!(%e, "board release failed");
        }
    })
    .await
    .ok();

    // 6. Update status on exit
    if let Ok(mut status) = state.status.lock() {
        if status.state == "connected" {
            status.state = "disconnected".to_string();
        }
    }
    info!("openbci session ended");
    Ok(())
}

fn create_and_start_board(config: &OpenBciConfig) -> Result<(OpenBciAdapter, Box<dyn Board>), String> {
    use skill_devices::openbci::board::{cyton::CytonBoard, cyton_daisy::CytonDaisyBoard};
    use skill_settings::OpenBciBoard;

    let (kind, eeg_channels, sample_rate): (&str, usize, f64) = match config.board {
        OpenBciBoard::Cyton => ("cyton", 8, 250.0),
        OpenBciBoard::CytonDaisy => ("cyton_daisy", 16, 250.0),
        OpenBciBoard::CytonWifi => ("cyton_wifi", 8, 1000.0),
        OpenBciBoard::CytonDaisyWifi => ("cyton_daisy_wifi", 16, 125.0),
        OpenBciBoard::Ganglion => ("ganglion", 4, 200.0),
        OpenBciBoard::GanglionWifi => ("ganglion_wifi", 4, 200.0),
        OpenBciBoard::Galea => ("galea", 24, 250.0),
    };

    let channel_names: Vec<String> = (0..eeg_channels)
        .map(|i| {
            config
                .channel_labels
                .get(i)
                .filter(|s| !s.is_empty())
                .cloned()
                .unwrap_or_else(|| format!("Ch{}", i + 1))
        })
        .collect();

    let desc = OpenBciAdapter::make_descriptor(kind, eeg_channels, sample_rate, channel_names);
    let info = DeviceInfo {
        name: format!("OpenBCI {}", kind.replace('_', " ")),
        ..Default::default()
    };

    match config.board {
        OpenBciBoard::Cyton => {
            let port = resolve_serial_port(&config.serial_port)?;
            let mut board = CytonBoard::new(&port);
            board
                .prepare()
                .map_err(|e| format!("Cyton prepare failed on {port}: {e}"))?;
            let stream = board
                .start_stream()
                .map_err(|e| format!("Cyton start_stream failed: {e}"))?;
            Ok((
                OpenBciAdapter::start(stream, desc, info),
                Box::new(board) as Box<dyn Board>,
            ))
        }
        OpenBciBoard::CytonDaisy => {
            let port = resolve_serial_port(&config.serial_port)?;
            let mut board = CytonDaisyBoard::new(&port);
            board
                .prepare()
                .map_err(|e| format!("CytonDaisy prepare failed on {port}: {e}"))?;
            let stream = board
                .start_stream()
                .map_err(|e| format!("CytonDaisy start_stream failed: {e}"))?;
            Ok((
                OpenBciAdapter::start(stream, desc, info),
                Box::new(board) as Box<dyn Board>,
            ))
        }
        OpenBciBoard::Ganglion => {
            use skill_devices::openbci::board::ganglion::{GanglionBoard, GanglionConfig, GanglionFilter};
            let ganglion_config = GanglionConfig {
                scan_timeout: Duration::from_secs(config.scan_timeout_secs as u64),
                filter: GanglionFilter::default(),
                ..Default::default()
            };
            let mut board = GanglionBoard::new(ganglion_config);
            board.prepare().map_err(|e| format!("Ganglion prepare failed: {e}"))?;
            let stream = board
                .start_stream()
                .map_err(|e| format!("Ganglion start_stream failed: {e}"))?;
            Ok((
                OpenBciAdapter::start(stream, desc, info),
                Box::new(board) as Box<dyn Board>,
            ))
        }
        OpenBciBoard::CytonWifi => {
            use skill_devices::openbci::board::cyton_wifi::{CytonWifiBoard, CytonWifiConfig};
            let wifi_cfg = CytonWifiConfig {
                shield_ip: config.wifi_shield_ip.trim().to_string(),
                local_port: config.wifi_local_port,
                ..Default::default()
            };
            let mut board = CytonWifiBoard::new(wifi_cfg);
            board.prepare().map_err(|e| format!("CytonWifi prepare failed: {e}"))?;
            let stream = board
                .start_stream()
                .map_err(|e| format!("CytonWifi start_stream failed: {e}"))?;
            Ok((
                OpenBciAdapter::start(stream, desc, info),
                Box::new(board) as Box<dyn Board>,
            ))
        }
        OpenBciBoard::CytonDaisyWifi => {
            use skill_devices::openbci::board::cyton_daisy_wifi::{CytonDaisyWifiBoard, CytonDaisyWifiConfig};
            let wifi_cfg = CytonDaisyWifiConfig {
                shield_ip: config.wifi_shield_ip.trim().to_string(),
                local_port: config.wifi_local_port,
                ..Default::default()
            };
            let mut board = CytonDaisyWifiBoard::new(wifi_cfg);
            board
                .prepare()
                .map_err(|e| format!("CytonDaisyWifi prepare failed: {e}"))?;
            let stream = board
                .start_stream()
                .map_err(|e| format!("CytonDaisyWifi start_stream failed: {e}"))?;
            Ok((
                OpenBciAdapter::start(stream, desc, info),
                Box::new(board) as Box<dyn Board>,
            ))
        }
        OpenBciBoard::GanglionWifi => {
            use skill_devices::openbci::board::ganglion_wifi::{GanglionWifiBoard, GanglionWifiConfig};
            let wifi_cfg = GanglionWifiConfig {
                shield_ip: config.wifi_shield_ip.trim().to_string(),
                local_port: config.wifi_local_port,
                ..Default::default()
            };
            let mut board = GanglionWifiBoard::new(wifi_cfg);
            board
                .prepare()
                .map_err(|e| format!("GanglionWifi prepare failed: {e}"))?;
            let stream = board
                .start_stream()
                .map_err(|e| format!("GanglionWifi start_stream failed: {e}"))?;
            Ok((
                OpenBciAdapter::start(stream, desc, info),
                Box::new(board) as Box<dyn Board>,
            ))
        }
        OpenBciBoard::Galea => {
            use skill_devices::openbci::board::galea::GaleaBoard;
            let ip = config.galea_ip.trim();
            if ip.is_empty() {
                return Err("Galea IP not configured".to_string());
            }
            let mut board = GaleaBoard::new(ip);
            board.prepare().map_err(|e| format!("Galea prepare failed: {e}"))?;
            let stream = board
                .start_stream()
                .map_err(|e| format!("Galea start_stream failed: {e}"))?;
            Ok((
                OpenBciAdapter::start(stream, desc, info),
                Box::new(board) as Box<dyn Board>,
            ))
        }
    }
}

/// Resolve the serial port: use the configured value or auto-detect the first
/// available OpenBCI dongle.
fn resolve_serial_port(configured: &str) -> Result<String, String> {
    let port_name = if !configured.is_empty() {
        configured.to_string()
    } else {
        auto_detect_serial_port()?
    };

    // On Windows, COM ports >= COM10 need the \\.\COMxx prefix for
    // `CreateFileW` to open them correctly.  Without this prefix,
    // `serialport::new("COM10", ..).open()` fails with "file not found".
    Ok(normalize_com_port(&port_name))
}

/// Auto-detect an OpenBCI FTDI serial port.
fn auto_detect_serial_port() -> Result<String, String> {
    let ports = serialport::available_ports().unwrap_or_default();

    // Pass 1: exact FTDI VID/PID match
    for port in &ports {
        if let serialport::SerialPortType::UsbPort(usb) = &port.port_type {
            if usb.vid == 0x0403 && matches!(usb.pid, 0x6015 | 0x6001 | 0x6014) {
                info!(port = %port.port_name, "auto-detected OpenBCI serial port (FTDI VID/PID)");
                return Ok(port.port_name.clone());
            }
        }
    }

    // Pass 2: FTDI / OpenBCI product/manufacturer string
    for port in &ports {
        if let serialport::SerialPortType::UsbPort(usb) = &port.port_type {
            let product_match = usb
                .product
                .as_deref()
                .map(|p| {
                    let pl = p.to_lowercase();
                    pl.contains("ft231x") || pl.contains("ft232") || pl.contains("openbci") || pl.contains("ftdi")
                })
                .unwrap_or(false);
            let mfg_match = usb
                .manufacturer
                .as_deref()
                .map(|m| {
                    let ml = m.to_lowercase();
                    ml.contains("ftdi") || ml.contains("openbci")
                })
                .unwrap_or(false);
            if product_match || mfg_match {
                info!(port = %port.port_name, "auto-detected OpenBCI serial port (product/mfg)");
                return Ok(port.port_name.clone());
            }
        }
    }

    // Pass 3 (macOS/Linux): path-based fallback
    #[cfg(not(target_os = "windows"))]
    for port in &ports {
        let lower = port.port_name.to_lowercase();
        if lower.contains("ttyusb") || lower.contains("usbserial") {
            info!(port = %port.port_name, "auto-detected serial port (path heuristic)");
            return Ok(port.port_name.clone());
        }
    }

    // Pass 3 (Windows): any COM port >= COM3 that is USB or Unknown type
    // On Windows, FTDI dongles sometimes appear as "Unknown" port type
    // when the FTDI driver provides no USB metadata to serialport-rs.
    #[cfg(target_os = "windows")]
    {
        // Sort by port number so we pick the lowest available COM port
        let mut candidates: Vec<(u32, String)> = Vec::new();
        for port in &ports {
            let lower = port.port_name.to_lowercase();
            let num = lower
                .strip_prefix("com")
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(0);
            if num >= 3 {
                let is_usb_or_unknown = matches!(
                    port.port_type,
                    serialport::SerialPortType::UsbPort(_) | serialport::SerialPortType::Unknown
                );
                if is_usb_or_unknown {
                    candidates.push((num, port.port_name.clone()));
                }
            }
        }
        candidates.sort_by_key(|(n, _)| *n);
        if let Some((_, name)) = candidates.first() {
            warn!(port = %name, "no FTDI USB metadata — falling back to first available COM port");
            return Ok(name.clone());
        }
    }

    Err("No serial port configured and no OpenBCI dongle detected. \
         Please plug in the USB dongle or set the serial port manually in Settings."
        .to_string())
}

/// Normalize a Windows COM port path.  COM ports >= COM10 must use the
/// `\\.\COMxx` (device path) syntax for `CreateFileW` to find them.
/// Without this, opening COM10+ silently fails with "file not found".
///
/// On non-Windows platforms this is a no-op.
fn normalize_com_port(name: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        let upper = name.to_uppercase();
        // Already in device-path form
        if upper.starts_with(r"\\.\COM") {
            return name.to_string();
        }
        // Bare COMxx name → prepend device-path prefix
        if let Some(num_str) = upper.strip_prefix("COM") {
            if let Ok(num) = num_str.parse::<u32>() {
                if num >= 10 {
                    return format!(r"\\.\COM{num}");
                }
            }
        }
    }
    name.to_string()
}

// ── Session data directory ────────────────────────────────────────────────────────

fn utc_date_dir(skill_dir: &Path) -> PathBuf {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Compute YYYYMMDD from unix timestamp
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
    let dir_name = format!("{y:04}{m:02}{d:02}");
    let dir = skill_dir.join(dir_name);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

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

/// Write session metadata JSON sidecar.
fn write_session_meta(
    csv_path: &Path,
    device_name: &str,
    channel_names: &[String],
    sample_rate: f64,
    session_start_utc: u64,
    total_samples: u64,
) {
    let session_end_utc = unix_secs();
    let meta = serde_json::json!({
        "session_start_utc": session_start_utc,
        "session_end_utc": session_end_utc,
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

// ── Epoch metrics SQLite store ───────────────────────────────────────────────

/// Lightweight per-day SQLite store for epoch metrics.
/// Stores band power snapshots every ~250ms (4 Hz) in the same `eeg.sqlite`
/// schema that the old Tauri app used, so search/compare/history all work.
struct EpochStore {
    conn: rusqlite::Connection,
}

impl EpochStore {
    fn open(day_dir: &Path) -> Option<Self> {
        let db_path = day_dir.join(skill_constants::SQLITE_FILE);
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
        Some(Self { conn })
    }

    fn insert_metrics(&self, timestamp_ms: i64, device_name: Option<&str>, metrics: &skill_exg::EpochMetrics) {
        let metrics_json = serde_json::to_string(metrics).unwrap_or_default();
        let empty_blob: &[u8] = &[];
        let _ = self.conn.execute(
            "INSERT INTO embeddings
             (timestamp, device_id, device_name, hnsw_id, eeg_embedding, metrics_json)
             VALUES (?1, NULL, ?2, 0, ?3, ?4)",
            rusqlite::params![timestamp_ms, device_name, empty_blob, metrics_json],
        );
    }
}

// ── Session pipeline ───────────────────────────────────────────────────────────

/// Session pipeline state: CSV + DSP + embedding accumulator.
struct SessionPipeline {
    csv: CsvState,
    csv_path: PathBuf,
    band_analyzer: BandAnalyzer,
    epoch_store: Option<EpochStore>,
    epoch_accumulator: Option<EpochAccumulator>,
    _embed_worker: Option<EmbedWorkerHandle>,
    channel_names: Vec<String>,
    sample_rate: f64,
    session_start_utc: u64,
    device_name: String,
    total_samples: u64,
    /// Flush counter — flush CSV to disk every N samples.
    flush_counter: u64,
}

impl SessionPipeline {
    fn new(
        skill_dir: &Path,
        eeg_channels: usize,
        sample_rate: f64,
        channel_names: Vec<String>,
        device_name: String,
        events_tx: broadcast::Sender<EventEnvelope>,
        hooks: Vec<skill_settings::HookRule>,
    ) -> Result<Self, String> {
        let day_dir = utc_date_dir(skill_dir);
        let session_start_utc = unix_secs();
        let csv_name = format!("exg_{session_start_utc}.csv");
        let csv_path = day_dir.join(&csv_name);

        let labels: Vec<&str> = channel_names.iter().map(String::as_str).collect();
        let csv = if labels.is_empty() {
            let default_labels: Vec<String> = (0..eeg_channels).map(|i| format!("Ch{}", i + 1)).collect();
            let refs: Vec<&str> = default_labels.iter().map(String::as_str).collect();
            CsvState::open_with_labels(&csv_path, &refs)
        } else {
            CsvState::open_with_labels(&csv_path, &labels)
        }
        .map_err(|e| format!("CSV open failed: {e}"))?;

        let band_analyzer = BandAnalyzer::new_with_rate(sample_rate as f32);
        let epoch_store = EpochStore::open(&day_dir);

        // Spawn the embedding worker + accumulator.
        let model_config = skill_eeg::eeg_model_config::load_model_config(skill_dir);
        let embed_worker = EmbedWorkerHandle::spawn(skill_dir.to_path_buf(), model_config, events_tx, hooks);
        let epoch_accumulator = EpochAccumulator::new(
            embed_worker.tx.clone(),
            eeg_channels,
            sample_rate as f32,
            channel_names.clone(),
        );

        info!(path = %csv_path.display(), channels = eeg_channels, rate = sample_rate, "session CSV opened");

        Ok(Self {
            csv,
            csv_path,
            band_analyzer,
            epoch_store,
            epoch_accumulator: Some(epoch_accumulator),
            _embed_worker: Some(embed_worker),
            channel_names,
            sample_rate,
            session_start_utc,
            device_name,
            total_samples: 0,
            flush_counter: 0,
        })
    }

    /// Push an EEG sample frame.  Returns a BandSnapshot if the DSP fired.
    fn push_eeg(&mut self, channels: &[f64], timestamp: f64) -> Option<skill_eeg::eeg_bands::BandSnapshot> {
        self.total_samples += 1;
        self.flush_counter += 1;

        // Write to CSV
        for (electrode, &value) in channels.iter().enumerate() {
            self.csv.push_eeg(electrode, &[value], timestamp, self.sample_rate);
        }

        // Flush every 256 samples (~1s at 250Hz)
        if self.flush_counter >= 256 {
            self.csv.flush();
            self.flush_counter = 0;
        }

        // Feed DSP band analyzer
        let mut new_snapshot = false;
        for (ch, &value) in channels.iter().enumerate() {
            if self.band_analyzer.push(ch, &[value]) {
                new_snapshot = true;
            }
        }

        // Feed the epoch accumulator (for embedding pipeline).
        if let Some(ref mut acc) = self.epoch_accumulator {
            let f32_channels: Vec<f32> = channels.iter().map(|&v| v as f32).collect();
            for (electrode, &value) in f32_channels.iter().enumerate() {
                acc.push(electrode, &[value]);
            }
        }

        if new_snapshot {
            if let Some(ref snap) = self.band_analyzer.latest {
                // Write metrics CSV row
                self.csv.push_metrics(&self.csv_path, snap);

                // Store epoch metrics in SQLite (for search/compare/history)
                if let Some(ref store) = self.epoch_store {
                    let ts_ms = (snap.timestamp * 1000.0) as i64;
                    let metrics = skill_exg::EpochMetrics::from_snapshot(snap);
                    store.insert_metrics(ts_ms, Some(&self.device_name), &metrics);
                }

                // Update the epoch accumulator's band snapshot.
                if let Some(ref mut acc) = self.epoch_accumulator {
                    acc.update_bands(snap.clone());
                }
            }
            self.band_analyzer.latest.clone()
        } else {
            None
        }
    }

    /// Finalize the session: flush CSV, write metadata.
    fn finalize(&mut self) {
        self.csv.flush();
        write_session_meta(
            &self.csv_path,
            &self.device_name,
            &self.channel_names,
            self.sample_rate,
            self.session_start_utc,
            self.total_samples,
        );
        info!(
            path = %self.csv_path.display(),
            samples = self.total_samples,
            "session finalized"
        );
    }
}

/// Read events from the adapter and broadcast them as daemon events.
/// Also records EEG to CSV and computes band power.
async fn pump_events(
    mut adapter: OpenBciAdapter,
    state: &AppState,
    cancel_rx: &mut oneshot::Receiver<()>,
    eeg_channels: usize,
    sample_rate: f64,
    channel_names: Vec<String>,
) {
    let mut sample_count: u64 = 0;
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let mut pipeline: Option<SessionPipeline> = None;
    #[allow(unused_assignments)]
    let mut device_name = "OpenBCI".to_string();

    loop {
        tokio::select! {
            _ = &mut *cancel_rx => {
                info!("openbci session cancelled");
                adapter.disconnect().await;
                break;
            }
            event = adapter.next_event() => {
                match event {
                    Some(DeviceEvent::Connected(info)) => {
                        info!(name = %info.name, "openbci device connected");
                        device_name = info.name.clone();
                        if let Ok(mut status) = state.status.lock() {
                            status.state = "connected".to_string();
                            status.device_name = Some(info.name.clone());
                        }
                        broadcast_event(&state.events_tx, "DeviceConnected", &serde_json::json!({
                            "name": info.name,
                        }));

                        // Open CSV + initialize DSP + embedding pipeline
                        let hooks = state.hooks.lock().map(|g| g.clone()).unwrap_or_default();
                        match SessionPipeline::new(
                            &skill_dir,
                            eeg_channels,
                            sample_rate,
                            channel_names.clone(),
                            device_name.clone(),
                            state.events_tx.clone(),
                            hooks,
                        ) {
                            Ok(mut p) => {
                                if let Some(ref mut acc) = p.epoch_accumulator {
                                    acc.set_device_name(device_name.clone());
                                }
                                pipeline = Some(p);
                            }
                            Err(e) => error!(%e, "failed to open session pipeline"),
                        }
                    }
                    Some(DeviceEvent::Eeg(frame)) => {
                        sample_count += 1;
                        if let Ok(mut status) = state.status.lock() {
                            status.sample_count = sample_count;
                        }

                        // Feed the session pipeline (CSV + DSP)
                        if let Some(ref mut pipe) = pipeline {
                            if let Some(snap) = pipe.push_eeg(&frame.channels, frame.timestamp_s) {
                                // Update latest_bands in daemon state
                                if let Ok(val) = serde_json::to_value(&snap) {
                                    if let Ok(mut bands) = state.latest_bands.lock() {
                                        *bands = Some(val.clone());
                                    }
                                    // Broadcast band snapshot to WS clients
                                    broadcast_event(&state.events_tx, "EegBands", &val);
                                }
                            }
                        }

                        // Broadcast per-electrode EEG samples
                        for (electrode, &value) in frame.channels.iter().enumerate() {
                            broadcast_event(&state.events_tx, "EegSample", &serde_json::json!({
                                "electrode": electrode,
                                "samples": [value],
                                "timestamp": frame.timestamp_s,
                            }));
                        }
                    }
                    Some(DeviceEvent::Imu(frame)) => {
                        let ts = unix_secs_f64();
                        broadcast_event(&state.events_tx, "ImuSample", &serde_json::json!({
                            "sensor": "accel",
                            "samples": [frame.accel],
                            "timestamp": ts,
                        }));
                        if let Some(gyro) = frame.gyro {
                            broadcast_event(&state.events_tx, "ImuSample", &serde_json::json!({
                                "sensor": "gyro",
                                "samples": [gyro],
                                "timestamp": ts,
                            }));
                        }
                    }
                    Some(DeviceEvent::Disconnected) => {
                        info!("openbci device disconnected");
                        if let Ok(mut status) = state.status.lock() {
                            status.state = "disconnected".to_string();
                        }
                        broadcast_event(&state.events_tx, "DeviceDisconnected", &serde_json::json!({}));
                        break;
                    }
                    Some(DeviceEvent::Battery(frame)) => {
                        if let Ok(mut status) = state.status.lock() {
                            status.battery = frame.level_pct;
                        }
                    }
                    Some(_) => { /* PPG, fNIRS, etc. — not relevant for OpenBCI */ }
                    None => {
                        info!("openbci event stream ended");
                        if let Ok(mut status) = state.status.lock() {
                            status.state = "disconnected".to_string();
                        }
                        broadcast_event(&state.events_tx, "DeviceDisconnected", &serde_json::json!({}));
                        break;
                    }
                }
            }
        }
    }

    // Finalize session: flush CSV + write metadata
    if let Some(ref mut pipe) = pipeline {
        pipe.finalize();
    }
}

fn broadcast_event(tx: &broadcast::Sender<EventEnvelope>, event_type: &str, payload: &serde_json::Value) {
    let envelope = EventEnvelope {
        r#type: event_type.to_string(),
        ts_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        correlation_id: None,
        payload: payload.clone(),
    };
    // Ignore send error — no subscribers is normal during startup.
    let _ = tx.send(envelope);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_serial_port_returns_configured_when_set() {
        let r1 = resolve_serial_port("COM3").unwrap();
        assert!(r1.contains("COM3"), "expected COM3 in result: {r1}");
        assert_eq!(resolve_serial_port("/dev/ttyUSB0").unwrap(), "/dev/ttyUSB0");
    }

    #[test]
    fn normalize_com_port_handles_all_cases() {
        // Non-Windows: always returns input unchanged
        // Windows: COM1-COM9 unchanged, COM10+ gets \\.\COMxx prefix
        let r = normalize_com_port("/dev/ttyUSB0");
        assert_eq!(r, "/dev/ttyUSB0");

        #[cfg(target_os = "windows")]
        {
            assert_eq!(normalize_com_port("COM3"), "COM3");
            assert_eq!(normalize_com_port("COM9"), "COM9");
            assert_eq!(normalize_com_port("COM10"), r"\\.\COM10");
            assert_eq!(normalize_com_port("COM15"), r"\\.\COM15");
            // Already prefixed
            assert_eq!(normalize_com_port(r"\\.\COM10"), r"\\.\COM10");
        }
    }

    #[test]
    fn resolve_serial_port_empty_without_dongle_fails() {
        // With no FTDI dongle attached, auto-detect should fail gracefully.
        // (On CI this always fails; in dev it may find a real dongle.)
        let result = resolve_serial_port("");
        // Either it finds a port or returns a clear error message.
        match result {
            Ok(port) => assert!(!port.is_empty()),
            Err(e) => assert!(e.contains("No serial port configured")),
        }
    }

    #[test]
    fn broadcast_event_sends_correct_type() {
        let (tx, mut rx) = broadcast::channel(4);
        broadcast_event(&tx, "TestEvent", &serde_json::json!({"key": "val"}));

        let envelope = rx.try_recv().unwrap();
        assert_eq!(envelope.r#type, "TestEvent");
        assert_eq!(envelope.payload["key"], "val");
        assert!(envelope.ts_unix_ms > 0);
        assert!(envelope.correlation_id.is_none());
    }

    #[test]
    fn broadcast_event_no_subscriber_does_not_panic() {
        let (tx, rx) = broadcast::channel::<EventEnvelope>(4);
        // All receivers dropped — should not panic.
        drop(rx);
        broadcast_event(&tx, "Orphan", &serde_json::json!({}));
    }

    #[test]
    fn create_board_serial_boards_fail_gracefully() {
        // Serial boards fail fast when the port doesn't exist.
        use skill_settings::OpenBciBoard;

        for board in [OpenBciBoard::Cyton, OpenBciBoard::CytonDaisy] {
            let config = OpenBciConfig {
                board: board.clone(),
                serial_port: "FAKE_NONEXISTENT_PORT".to_string(),
                wifi_shield_ip: String::new(),
                wifi_local_port: 3000,
                galea_ip: String::new(),
                scan_timeout_secs: 1,
                channel_labels: Vec::new(),
            };
            let result = create_and_start_board(&config);
            assert!(result.is_err(), "expected error for board {board:?} with fake port");
            let err = result.err().unwrap();
            assert!(
                err.contains("prepare failed"),
                "error should mention prepare failure: {err}"
            );
        }
    }

    /// Full board variant test — skipped by default because WiFi/BLE/UDP
    /// boards attempt real network I/O and take 60+ seconds to time out.
    /// Run explicitly with: cargo test -- --ignored create_board_all_variants
    #[test]
    #[ignore]
    fn create_board_all_variants_fail_gracefully() {
        use skill_settings::OpenBciBoard;

        for board in [
            OpenBciBoard::Cyton,
            OpenBciBoard::CytonDaisy,
            OpenBciBoard::CytonWifi,
            OpenBciBoard::CytonDaisyWifi,
            OpenBciBoard::Galea,
        ] {
            let config = OpenBciConfig {
                board: board.clone(),
                serial_port: "FAKE_PORT".to_string(),
                wifi_shield_ip: "192.168.1.99".to_string(),
                wifi_local_port: 3000,
                galea_ip: "192.168.1.100".to_string(),
                scan_timeout_secs: 1,
                channel_labels: Vec::new(),
            };
            let result = create_and_start_board(&config);
            assert!(result.is_err(), "expected error for board {board:?} with fake port");
        }
    }
}
