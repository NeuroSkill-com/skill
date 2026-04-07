// SPDX-License-Identifier: GPL-3.0-only
//! Board drivers and NeuroField session runner.
//!
//! Contains:
//! - `create_and_start_board` — constructs any OpenBCI board variant
//! - `resolve_serial_port` / `normalize_com_port` — Windows COM port helpers
//! - `run_neurofield_session` — dedicated NeuroField Q21 session (PCAN-USB)
//!
//! All other devices route through `session/connect.rs` and the generic
//! `session/runner.rs` adapter runner.

use std::time::Duration;

use skill_devices::openbci::board::Board;
use skill_devices::session::openbci::OpenBciAdapter;
use skill_devices::session::DeviceInfo;
use skill_settings::OpenBciConfig;
use tokio::sync::oneshot;
use tracing::info;

#[cfg(target_os = "windows")]
use tracing::warn;

/// Handle returned to the caller so a session can be cancelled.
pub struct SessionHandle {
    pub cancel_tx: oneshot::Sender<()>,
}

pub(crate) fn create_and_start_board(config: &OpenBciConfig) -> Result<(OpenBciAdapter, Box<dyn Board>), String> {
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

/// Parse the PCAN UsbBus from a device ID like "neurofield:USB1:5".
pub(crate) fn parse_neurofield_bus(device_id: &str) -> neurofield::pcan::UsbBus {
    let parts: Vec<&str> = device_id.split(':').collect();
    let bus_str = parts.get(1).unwrap_or(&"USB1");
    match bus_str.to_uppercase().as_str() {
        "USB2" => neurofield::pcan::UsbBus::USB2,
        "USB3" => neurofield::pcan::UsbBus::USB3,
        "USB4" => neurofield::pcan::UsbBus::USB4,
        "USB5" => neurofield::pcan::UsbBus::USB5,
        "USB6" => neurofield::pcan::UsbBus::USB6,
        "USB7" => neurofield::pcan::UsbBus::USB7,
        "USB8" => neurofield::pcan::UsbBus::USB8,
        _ => neurofield::pcan::UsbBus::USB1,
    }
}

#[cfg(test)]
mod tests {
    use skill_daemon_common::EventEnvelope;
    use tokio::sync::broadcast;

    use super::*;
    use crate::session::shared::broadcast_event;

    #[test]
    fn resolve_serial_port_returns_configured_when_set() {
        let r1 = resolve_serial_port("COM3").expect("configured COM port should resolve");
        assert!(r1.contains("COM3"), "expected COM3 in result: {r1}");
        assert_eq!(
            resolve_serial_port("/dev/ttyUSB0").expect("configured tty path should resolve"),
            "/dev/ttyUSB0"
        );
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

        let envelope = rx.try_recv().expect("event should be available");
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
            let err = result.err().expect("error expected for fake serial board");
            assert!(
                err.contains("prepare failed"),
                "error should mention prepare failure: {err}"
            );
        }
    }

    /// Verify that the `usb:` target board-type promotion logic correctly
    /// maps every non-serial board to Cyton while preserving Cyton / CytonDaisy.
    ///
    /// This mirrors the guard in `connect_openbci` and `control_start_session`
    /// that prevents a BLE / WiFi / UDP board from being used when the user
    /// plugs in a USB dongle.
    #[test]
    fn usb_target_promotes_non_serial_boards_to_cyton() {
        use skill_settings::OpenBciBoard;

        // Helper: apply the same guard used in connect_openbci.
        fn resolve_board_for_usb(board: OpenBciBoard) -> OpenBciBoard {
            if !board.is_serial() {
                OpenBciBoard::Cyton
            } else {
                board
            }
        }

        // Non-serial boards — all should become Cyton.
        for board in [
            OpenBciBoard::Ganglion,
            OpenBciBoard::GanglionWifi,
            OpenBciBoard::CytonWifi,
            OpenBciBoard::CytonDaisyWifi,
            OpenBciBoard::Galea,
        ] {
            let promoted = resolve_board_for_usb(board.clone());
            assert_eq!(
                promoted,
                OpenBciBoard::Cyton,
                "usb: with board {board:?} should promote to Cyton"
            );
        }

        // Serial boards — must be preserved as-is.
        assert_eq!(
            resolve_board_for_usb(OpenBciBoard::Cyton),
            OpenBciBoard::Cyton,
            "Cyton should stay Cyton"
        );
        assert_eq!(
            resolve_board_for_usb(OpenBciBoard::CytonDaisy),
            OpenBciBoard::CytonDaisy,
            "CytonDaisy should stay CytonDaisy (16-ch user choice preserved)"
        );
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
