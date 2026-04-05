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

use std::time::Duration;

use skill_daemon_common::EventEnvelope;
use skill_devices::openbci::board::Board;
use skill_devices::session::openbci::OpenBciAdapter;
use skill_devices::session::{DeviceAdapter, DeviceEvent, DeviceInfo};
use skill_settings::OpenBciConfig;
use tokio::sync::{broadcast, oneshot};
use tracing::{error, info};

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
    let (adapter, board) =
        tokio::task::spawn_blocking(move || -> Result<_, String> { create_and_start_board(&config) })
            .await
            .map_err(|e| format!("spawn_blocking join error: {e}"))?
            .map_err(|e| format!("board setup failed: {e}"))?;

    // 3. Update status to "connected" (the frontend state for active sessions)
    if let Ok(mut status) = state.status.lock() {
        status.state = "connected".to_string();
        status.device_error = None;
    }

    // 4. Pump events
    pump_events(adapter, &state, &mut cancel_rx).await;

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
    if !configured.is_empty() {
        return Ok(configured.to_string());
    }

    // Auto-detect: pick the first FTDI-like serial port.
    let ports = serialport::available_ports().unwrap_or_default();
    for port in &ports {
        let dominated = match &port.port_type {
            serialport::SerialPortType::UsbPort(usb) => {
                usb.vid == 0x0403 && matches!(usb.pid, 0x6015 | 0x6001 | 0x6014)
            }
            _ => false,
        };
        if dominated {
            info!(port = %port.port_name, "auto-detected OpenBCI serial port");
            return Ok(port.port_name.clone());
        }
    }

    // On Windows, fall back to first COM port >= COM3
    #[cfg(target_os = "windows")]
    for port in &ports {
        let lower = port.port_name.to_lowercase();
        let num = lower
            .strip_prefix("com")
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(0);
        if num >= 3 {
            warn!(port = %port.port_name, "no FTDI USB metadata — falling back to first available COM port");
            return Ok(port.port_name.clone());
        }
    }

    Err("No serial port configured and no OpenBCI dongle detected. \
         Please plug in the USB dongle or set the serial port manually in Settings."
        .to_string())
}

/// Read events from the adapter and broadcast them as daemon events.
async fn pump_events(mut adapter: OpenBciAdapter, state: &AppState, cancel_rx: &mut oneshot::Receiver<()>) {
    let mut sample_count: u64 = 0;

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
                        if let Ok(mut status) = state.status.lock() {
                            status.state = "connected".to_string();
                            status.device_name = Some(info.name.clone());
                        }
                        broadcast_event(&state.events_tx, "DeviceConnected", &serde_json::json!({
                            "name": info.name,
                        }));
                    }
                    Some(DeviceEvent::Eeg(frame)) => {
                        sample_count += 1;
                        if let Ok(mut status) = state.status.lock() {
                            status.sample_count = sample_count;
                        }
                        // Emit one event per electrode to match the format
                        // expected by the frontend (electrode + samples[]).
                        for (electrode, &value) in frame.channels.iter().enumerate() {
                            broadcast_event(&state.events_tx, "EegSample", &serde_json::json!({
                                "electrode": electrode,
                                "samples": [value],
                                "timestamp": frame.timestamp_s,
                            }));
                        }
                    }
                    Some(DeviceEvent::Imu(frame)) => {
                        let ts = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs_f64())
                            .unwrap_or(0.0);
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
                        // Stream ended (board powered off or dongle disconnected)
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
        assert_eq!(resolve_serial_port("COM3").unwrap(), "COM3");
        assert_eq!(resolve_serial_port("/dev/ttyUSB0").unwrap(), "/dev/ttyUSB0");
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
