use std::collections::{HashMap, HashSet};
use std::time::Duration;

use btleplug::{
    api::{Central as _, CentralEvent, Manager as _, Peripheral as _, ScanFilter},
    platform::Manager as BtManager,
};
use futures::StreamExt;
use skill_daemon_common::{DiscoveredDeviceResponse, ScannerWifiConfigRequest};
use tokio::sync::oneshot;

use tracing::debug;

use crate::state::AppState;
use crate::util::{now_unix_secs, push_device_log};

/// Return `true` when a BLE advertising name looks like a supported EEG/neurofeedback device.
pub(crate) fn is_known_eeg_ble_name(name: &str) -> bool {
    let n = name.to_lowercase();
    // Muse family (Muse 1/2/S, Muse-S Athena, Muse Monitor)
    n.starts_with("muse")
        // OpenBCI Ganglion
        || n.starts_with("ganglion")
        || n.starts_with("simblee")
        // OpenBCI Cyton
        || n.starts_with("openbci")
        || n.starts_with("cyton")
        // Neurable MW75
        || n.contains("mw75")
        || n.contains("neurable")
        // Hermes
        || n.starts_with("hermes")
        // Emotiv EPOC/Insight/Flex/MN8
        || n.starts_with("emotiv")
        || n.starts_with("epoc")
        || n.starts_with("insight")
        || n.starts_with("mn8")
        // Idun / Guardian
        || n.starts_with("idun")
        || n.starts_with("ige")
        || n.starts_with("guardian")
        // Mendi fNIRS
        || n.starts_with("mendi")
        // CGX / Cognionics
        || n.contains("cognionics")
        || n.contains("cgx")
        || n.starts_with("quick-")
        || n.starts_with("aim-")
        || n.starts_with("patch")
        // AWEAR
        || n.starts_with("awear")
        || n.starts_with("luca")
        // AttentivU
        || n.starts_with("atu")
        || n.starts_with("attentivu")
        // BrainBit
        || n.contains("brainbit")
        // g.tec Unicorn
        || n.contains("unicorn")
        || n.starts_with("un-")
        // NeuroField
        || n.contains("neurofield")
        || n.contains("q21")
        // NeuroSky
        || n.contains("neurosky")
        || n.contains("mindwave")
        // Neurosity Crown / Notion
        || n.contains("neurosity")
        || n.contains("crown")
        || n.contains("notion")
}

/// Return `true` when a discovered device is eligible for automatic pairing.
/// Excludes passive/proxy transports that require manual setup.
pub(crate) fn is_auto_pair_eligible(dev: &skill_daemon_common::DiscoveredDeviceResponse) -> bool {
    let n = dev.name.to_lowercase();
    // NeuroSky serial dongle requires manual pairing in OS Bluetooth first.
    if n.contains("neurosky") || n.contains("mindwave") {
        return false;
    }
    // BrainVision RDA is a network relay, not a physical device in proximity.
    if n.contains("brainvision") || dev.id.starts_with("brainvision:") {
        return false;
    }
    true
}

/// Read the current BLE device cache and return only devices whose names
/// match a known EEG/neurofeedback headset.  Entries not seen within the
/// last 60 seconds are suppressed (but kept in the cache for name recall).
pub(crate) fn read_ble_cache(state: &AppState) -> Vec<DiscoveredDeviceResponse> {
    let now = now_unix_secs();
    let Ok(cache) = state.ble_device_cache.lock() else {
        return Vec::new();
    };
    cache
        .iter()
        .filter_map(|(id, (name_opt, rssi, last_seen))| {
            // Must have a recognised EEG device name.
            let name = name_opt.as_deref()?;
            if !is_known_eeg_ble_name(name) {
                return None;
            }
            // Suppress stale entries (> 120 s since last advertisement).
            // 120 s covers the worst-case connection attempt window:
            // 600 ms pause + 5 s scan + 10 s connect + 15 s discover + margin.
            if now.saturating_sub(*last_seen) > 120 {
                return None;
            }
            Some(DiscoveredDeviceResponse {
                id: id.clone(),
                name: name.to_string(),
                last_seen: *last_seen,
                last_rssi: *rssi,
                is_paired: false,
                is_preferred: false,
                transport: "ble".to_string(),
            })
        })
        .collect()
}

/// Persistent, event-driven BLE scanner.
///
/// Creates the platform BLE manager **once** and subscribes to the adapter
/// event stream.  Each `DeviceDiscovered` / `DeviceUpdated` event is used to
/// update `state.ble_device_cache` with the peripheral's `local_name` and
/// RSSI.  This is far more reliable than the previous approach of tearing
/// down and re-creating the manager every 5 s with an 800 ms poll window,
/// which frequently caused CoreBluetooth to return `None` for `local_name`
/// (making the Muse look like an anonymous UUID and then being filtered out).
async fn run_ble_listener_task(state: AppState) {
    loop {
        // Stop when the outer scanner has been turned off.
        if !state.scanner_running.lock().map(|g| *g).unwrap_or(false) {
            return;
        }

        let Ok(manager) = BtManager::new().await else {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };

        let Ok(adapters) = manager.adapters().await else {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };

        let Some(adapter) = adapters.into_iter().next() else {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };

        let Ok(mut events) = adapter.events().await else {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };

        // Start a continuous scan with no service-UUID filter so we see all
        // advertising packets (including Muse, which uses proprietary UUIDs).
        let _ = adapter.start_scan(ScanFilter::default()).await;

        // Process events until the stream ends or the scanner is stopped.
        loop {
            // When a BLE device is actively connecting, stop our scan so only
            // one CBCentralManager.scanForPeripherals() is active at a time.
            // On macOS, two concurrent scans suppress peripheral.connect()
            // delegate callbacks, causing connections to hang.
            if state.ble_scan_paused.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = adapter.stop_scan().await;
                // Break out so `manager`, `adapter`, and `events` are
                // dropped at the end of the outer scope.  On macOS a
                // second CBCentralManager (created by device-crate
                // connect() functions) cannot discover peripherals while
                // the first one is still alive — even if scanning stopped.
                break;
            }

            // Short timeout so ble_scan_paused and scanner_running are
            // checked frequently even when no advertisements are arriving.
            let maybe_event = tokio::time::timeout(Duration::from_millis(300), events.next()).await;

            if !state.scanner_running.lock().map(|g| *g).unwrap_or(false) {
                return;
            }

            match maybe_event {
                // Adapter stream ended — break to outer loop to restart.
                Ok(None) => break,
                // Timeout — just re-check scanner_running and continue.
                Err(_) => continue,
                Ok(Some(CentralEvent::DeviceDiscovered(id) | CentralEvent::DeviceUpdated(id))) => {
                    if let Ok(peripheral) = adapter.peripheral(&id).await {
                        let mut name: Option<String> = None;
                        let mut rssi = 0i16;
                        if let Ok(Some(props)) = peripheral.properties().await {
                            name = props.local_name;
                            if let Some(rv) = props.rssi {
                                rssi = rv;
                            }
                        }
                        if let Some(ref n) = name {
                            debug!(ble_name = %n, rssi, "BLE advertisement");
                        }
                        let key = format!("ble:{}", id);
                        if let Ok(mut cache) = state.ble_device_cache.lock() {
                            let entry = cache.entry(key).or_insert((None, 0i16, 0u64));
                            // Never overwrite a known name with None.
                            if name.is_some() {
                                entry.0 = name;
                            }
                            if rssi != 0 {
                                entry.1 = rssi;
                            }
                            entry.2 = now_unix_secs();
                        }
                    }
                }
                Ok(Some(_)) => {} // StateUpdate, ManufacturerData, etc. — ignored
            }
        }

        // Stream ended (or paused for a BLE connect attempt).
        // Wait for the pause flag to clear before recreating the manager.
        while state.ble_scan_paused.load(std::sync::atomic::Ordering::Relaxed) {
            if !state.scanner_running.lock().map(|g| *g).unwrap_or(false) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        // Brief pause before restarting.
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

pub(crate) fn detect_openbci_serial_ports() -> Vec<(String, String)> {
    let Ok(ports) = serialport::available_ports() else {
        return Vec::new();
    };

    let mut results = Vec::new();
    for port in ports {
        let name = port.port_name.clone();
        let lower = name.to_lowercase();

        let is_openbci = match &port.port_type {
            serialport::SerialPortType::UsbPort(usb) => {
                // FTDI chips used across OpenBCI dongle revisions:
                //   0x6015 = FT231X  (current Cyton dongle)
                //   0x6001 = FT232R  (older Cyton/Ganglion dongles)
                //   0x6014 = FT232H  (rare but seen in some kits)
                let vid_match = usb.vid == 0x0403 && matches!(usb.pid, 0x6015 | 0x6001 | 0x6014);

                let product_match = usb
                    .product
                    .as_deref()
                    .map(|p| {
                        let pl = p.to_lowercase();
                        pl.contains("ft231x") || pl.contains("ft232") || pl.contains("openbci") || pl.contains("ftdi")
                    })
                    .unwrap_or(false);

                let manufacturer_match = usb
                    .manufacturer
                    .as_deref()
                    .map(|m| {
                        let ml = m.to_lowercase();
                        ml.contains("ftdi") || ml.contains("openbci")
                    })
                    .unwrap_or(false);

                vid_match || product_match || manufacturer_match
            }
            // Linux/macOS path-based fallback
            #[cfg(not(target_os = "windows"))]
            _ => lower.contains("ttyusb") || lower.contains("usbserial"),
            // Windows: FTDI dongles appear as generic COM ports when the
            // driver supplies no USB metadata.  Accept any COM port that
            // the system reports as PnP (non-built-in).  This is broader
            // than the USB branch above, but on Windows the fallback arm
            // only fires when `serialport` classifies the port as
            // `Unknown` — built-in COM0/COM1 are typically `PciPort`.
            #[cfg(target_os = "windows")]
            serialport::SerialPortType::Unknown => {
                // Heuristic: COM3 and above are almost always USB/PnP
                // adapters; COM1/COM2 are legacy motherboard UARTs.
                let port_num = lower
                    .strip_prefix("com")
                    .and_then(|n| n.parse::<u32>().ok())
                    .unwrap_or(0);
                port_num >= 3
            }
            #[cfg(target_os = "windows")]
            _ => false,
        };

        if is_openbci {
            let display = format!("OpenBCI ({name})");
            results.push((name, display));
        }
    }

    results
}

pub(crate) fn detect_cgx_serial_ports() -> Vec<(String, String)> {
    ::cognionics::prelude::enumerate_devices()
        .into_iter()
        .map(|d| {
            let display = if d.description.is_empty() {
                format!("CGX ({})", d.port)
            } else {
                format!("CGX {} ({})", d.description, d.port)
            };
            (d.port, display)
        })
        .collect()
}

pub(crate) fn detect_brainbit_devices() -> Vec<DiscoveredDeviceResponse> {
    use brainbit::prelude::*;
    let Ok(scanner) = Scanner::new(&[SensorFamily::LEBrainBit]) else {
        return Vec::new();
    };
    if scanner.start().is_err() {
        return Vec::new();
    }
    std::thread::sleep(std::time::Duration::from_secs(3));
    let _ = scanner.stop();
    let devices = scanner.devices().unwrap_or_default();
    devices
        .into_iter()
        .map(|d| {
            let name = d.name_str();
            let addr = d.address_str();
            let id = format!("brainbit:{addr}");
            let display = if name.is_empty() {
                format!("BrainBit ({addr})")
            } else {
                format!("BrainBit {name}")
            };
            DiscoveredDeviceResponse {
                id,
                name: display,
                last_seen: now_unix_secs(),
                last_rssi: 0,
                is_paired: false,
                is_preferred: false,
                transport: "ble".to_string(),
            }
        })
        .collect()
}

pub(crate) fn detect_brainmaster_devices() -> Vec<DiscoveredDeviceResponse> {
    let ports = brainmaster::device::BrainMasterDevice::scan().unwrap_or_default();
    ports
        .into_iter()
        .map(|port| DiscoveredDeviceResponse {
            id: format!("brainmaster:{port}"),
            name: format!("BrainMaster ({port})"),
            last_seen: now_unix_secs(),
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "usb_serial".to_string(),
        })
        .collect()
}

pub(crate) fn detect_gtec_devices() -> Vec<DiscoveredDeviceResponse> {
    let serials = gtec::device::UnicornDevice::scan(false).unwrap_or_default();
    serials
        .into_iter()
        .map(|serial| DiscoveredDeviceResponse {
            id: format!("gtec:{serial}"),
            name: format!("g.tec Unicorn ({serial})"),
            last_seen: now_unix_secs(),
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "ble".to_string(),
        })
        .collect()
}

pub(crate) fn detect_neurofield_devices() -> Vec<DiscoveredDeviceResponse> {
    let mut out = Vec::new();
    let online = neurofield::q21_api::Q21Api::get_online_pcan_interfaces();
    for bus in online {
        let bus_name = format!("{bus:?}");
        // Try to connect briefly to get device info.
        match neurofield::q21_api::Q21Api::new(bus) {
            Ok(mut api) => {
                let serial = api.eeg_device_serial();
                let dev_type = api.eeg_device_type();
                let name = format!("NeuroField Q21 ({dev_type:?} #{serial})");
                let id = format!("neurofield:{bus_name}:{serial}");
                api.release();
                out.push(DiscoveredDeviceResponse {
                    id,
                    name,
                    last_seen: now_unix_secs(),
                    last_rssi: 0,
                    is_paired: false,
                    is_preferred: false,
                    transport: "usb_serial".to_string(),
                });
            }
            Err(_) => {
                // PCAN interface online but no Q21 connected — report as available bus.
                out.push(DiscoveredDeviceResponse {
                    id: format!("neurofield:{bus_name}"),
                    name: format!("NeuroField PCAN ({bus_name})"),
                    last_seen: now_unix_secs(),
                    last_rssi: 0,
                    is_paired: false,
                    is_preferred: false,
                    transport: "usb_serial".to_string(),
                });
            }
        }
    }
    out
}

async fn cortex_probe_headsets(
    client: &skill_devices::emotiv::client::CortexClient,
) -> anyhow::Result<Vec<skill_devices::emotiv::types::HeadsetInfo>> {
    let (mut rx, handle) = client.connect().await.map_err(|e| anyhow::anyhow!("{e}"))?;

    use skill_devices::emotiv::types::CortexEvent;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);

    loop {
        let ev = tokio::time::timeout_at(deadline, rx.recv()).await;
        match ev {
            Ok(Some(CortexEvent::Authorized)) => break,
            Ok(Some(CortexEvent::Error(e))) => anyhow::bail!("{e}"),
            Ok(None) => anyhow::bail!("Channel closed before authorized"),
            Err(_) => anyhow::bail!("Timed out waiting for authorization"),
            _ => continue,
        }
    }

    handle.query_headsets().await.map_err(|e| anyhow::anyhow!("{e}"))?;

    loop {
        let ev = tokio::time::timeout_at(deadline, rx.recv()).await;
        match ev {
            Ok(Some(CortexEvent::HeadsetsQueried(list))) => return Ok(list),
            Ok(Some(CortexEvent::Error(e))) => anyhow::bail!("{e}"),
            Ok(None) => anyhow::bail!("Channel closed before headset query"),
            Err(_) => anyhow::bail!("Timed out waiting for headset query"),
            _ => continue,
        }
    }
}

pub(crate) async fn detect_cortex_devices(state: &AppState) -> Vec<DiscoveredDeviceResponse> {
    use skill_devices::emotiv::prelude::*;

    let (cfg_id, cfg_secret) = state
        .scanner_cortex_config
        .lock()
        .map(|g| (g.emotiv_client_id.clone(), g.emotiv_client_secret.clone()))
        .unwrap_or_else(|_| (String::new(), String::new()));

    let client_id = if cfg_id.trim().is_empty() {
        std::env::var("EMOTIV_CLIENT_ID").unwrap_or_default()
    } else {
        cfg_id
    };
    let client_secret = if cfg_secret.trim().is_empty() {
        std::env::var("EMOTIV_CLIENT_SECRET").unwrap_or_default()
    } else {
        cfg_secret
    };
    if client_id.trim().is_empty() || client_secret.trim().is_empty() {
        return Vec::new();
    }

    let config = CortexClientConfig {
        client_id,
        client_secret,
        auto_create_session: false,
        ..Default::default()
    };

    let client = CortexClient::new(config);
    let result = tokio::time::timeout(Duration::from_secs(12), cortex_probe_headsets(&client)).await;

    let Ok(Ok(headsets)) = result else {
        return Vec::new();
    };

    if headsets.is_empty() {
        return vec![DiscoveredDeviceResponse {
            id: "cortex:emotiv".to_string(),
            name: "Emotiv (Cortex)".to_string(),
            last_seen: now_unix_secs(),
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "cortex".to_string(),
        }];
    }

    headsets
        .into_iter()
        .map(|hs| DiscoveredDeviceResponse {
            id: format!("cortex:{}", hs.id),
            name: hs.id,
            last_seen: now_unix_secs(),
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "cortex".to_string(),
        })
        .collect()
}

pub(crate) fn detect_wifi_devices(cfg: &ScannerWifiConfigRequest) -> Vec<DiscoveredDeviceResponse> {
    let mut out = Vec::new();
    let now = now_unix_secs();

    let shield = cfg.wifi_shield_ip.trim();
    if !shield.is_empty() {
        out.push(DiscoveredDeviceResponse {
            id: format!("wifi:{shield}"),
            name: format!("OpenBCI WiFi Shield ({shield})"),
            last_seen: now,
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "wifi".to_string(),
        });
    }

    let galea = cfg.galea_ip.trim();
    if !galea.is_empty() {
        out.push(DiscoveredDeviceResponse {
            id: format!("galea:{galea}"),
            name: format!("Galea ({galea})"),
            last_seen: now,
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "wifi".to_string(),
        });
    }

    out
}

pub(crate) fn detect_manual_device_hints(state: &AppState) -> Vec<DiscoveredDeviceResponse> {
    let now = now_unix_secs();
    let mut out = vec![
        DiscoveredDeviceResponse {
            id: "neurosky".to_string(),
            name: "NeuroSky MindWave (serial)".to_string(),
            last_seen: now,
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "usb_serial".to_string(),
        },
        DiscoveredDeviceResponse {
            id: "brainvision:127.0.0.1:51244".to_string(),
            name: "BrainVision RDA (127.0.0.1:51244)".to_string(),
            last_seen: now,
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "wifi".to_string(),
        },
    ];

    let settings_device_id = state
        .skill_dir
        .lock()
        .ok()
        .map(|d| skill_settings::load_settings(&d).device_api.neurosity_device_id)
        .filter(|s| !s.trim().is_empty());

    let neurosity_device_id = settings_device_id
        .or_else(|| {
            std::env::var("SKILL_NEUROSITY_DEVICE_ID")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .or_else(|| {
            std::env::var("NEUROSITY_DEVICE_ID")
                .ok()
                .filter(|s| !s.trim().is_empty())
        });

    if let Some(device_id) = neurosity_device_id {
        out.push(DiscoveredDeviceResponse {
            id: format!("neurosity:{device_id}"),
            name: format!("Neurosity Crown/Notion ({device_id})"),
            last_seen: now,
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "wifi".to_string(),
        });
    }

    out
}

pub(crate) async fn run_usb_scanner_task(state: AppState, mut stop_rx: oneshot::Receiver<()>) {
    // Spawn the persistent event-driven BLE listener.  It runs in a separate
    // task so the 5-second scanner tick is never blocked by BLE I/O, and the
    // CoreBluetooth/BlueZ adapter stays alive between ticks.
    tokio::spawn(run_ble_listener_task(state.clone()));

    let mut tick = tokio::time::interval(Duration::from_secs(5));
    let mut cortex_tick = 0u64;

    loop {
        tokio::select! {
            _ = &mut stop_rx => break,
            _ = tick.tick() => {
                // Timeout serial port enumeration — on Windows the FTDI
                // driver can occasionally stall `serialport::available_ports()`
                // for 10+ seconds when a dongle is mid-reset.  Without a
                // timeout this blocks the entire scanner tick.
                let ports = tokio::time::timeout(
                    Duration::from_secs(3),
                    tokio::task::spawn_blocking(detect_openbci_serial_ports),
                )
                .await
                .ok()
                .and_then(std::result::Result::ok)
                .unwrap_or_default();

                let mut usb_discovered: Vec<DiscoveredDeviceResponse> = ports.into_iter().map(|(port, display)| {
                    DiscoveredDeviceResponse {
                        id: format!("usb:{port}"),
                        name: display,
                        last_seen: now_unix_secs(),
                        last_rssi: 0,
                        is_paired: false,
                        is_preferred: false,
                        transport: "usb_serial".to_string(),
                    }
                }).collect();

                let cgx_ports = tokio::time::timeout(
                    Duration::from_secs(3),
                    tokio::task::spawn_blocking(detect_cgx_serial_ports),
                )
                .await
                .ok()
                .and_then(std::result::Result::ok)
                .unwrap_or_default();

                let cgx_discovered: Vec<DiscoveredDeviceResponse> = cgx_ports.into_iter().map(|(port, display)| {
                    DiscoveredDeviceResponse {
                        id: format!("cgx:{port}"),
                        name: display,
                        last_seen: now_unix_secs(),
                        last_rssi: 0,
                        is_paired: false,
                        is_preferred: false,
                        transport: "usb_serial".to_string(),
                    }
                }).collect();

                usb_discovered.extend(cgx_discovered);

                let ble_discovered = read_ble_cache(&state);

                let cortex_discovered = if cortex_tick.is_multiple_of(2) {
                    detect_cortex_devices(&state).await
                } else {
                    Vec::new()
                };

                let wifi_cfg = state
                    .scanner_wifi_config
                    .lock()
                    .map(|g| g.clone())
                    .unwrap_or(ScannerWifiConfigRequest {
                        wifi_shield_ip: String::new(),
                        galea_ip: String::new(),
                    });
                let wifi_discovered = detect_wifi_devices(&wifi_cfg);

                // NeuroField Q21 (PCAN-USB) — probe every other tick to avoid
                // holding the CAN bus open continuously.
                let neurofield_discovered = if cortex_tick.is_multiple_of(2) {
                    tokio::task::spawn_blocking(detect_neurofield_devices)
                        .await
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                cortex_tick = cortex_tick.wrapping_add(1);

                let mut discovered = usb_discovered;
                discovered.extend(ble_discovered);
                discovered.extend(cortex_discovered);
                discovered.extend(wifi_discovered);
                discovered.extend(neurofield_discovered);

                // BrainBit (BLE via NeuroSDK2) — probe every other tick.
                let brainbit_discovered = if cortex_tick.is_multiple_of(2) {
                    tokio::task::spawn_blocking(detect_brainbit_devices)
                        .await
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                discovered.extend(brainbit_discovered);

                // g.tec Unicorn (BLE) — probe every other tick.
                let gtec_discovered = if cortex_tick.is_multiple_of(2) {
                    tokio::task::spawn_blocking(detect_gtec_devices)
                        .await
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                discovered.extend(gtec_discovered);

                // BrainMaster (USB serial)
                let brainmaster_discovered = tokio::time::timeout(
                    Duration::from_secs(3),
                    tokio::task::spawn_blocking(detect_brainmaster_devices),
                )
                .await
                .ok()
                .and_then(std::result::Result::ok)
                .unwrap_or_default();
                discovered.extend(brainmaster_discovered);

                // Manual-connect device hints (always visible in scanner list).
                discovered.extend(detect_manual_device_hints(&state));

                let discovered_count = discovered.len();

                if let Ok(mut guard) = state.devices.lock() {
                    let old: HashMap<String, DiscoveredDeviceResponse> =
                        guard.iter().map(|d| (d.id.clone(), d.clone())).collect();

                    // Build a set of paired device IDs from the authoritative
                    // status list.  This ensures devices are correctly marked
                    // as paired even on the very first scan tick after a daemon
                    // restart (when `old` is empty and carries no is_paired state).
                    let paired_ids: HashSet<String> = state
                        .status
                        .lock()
                        .map(|s| s.paired_devices.iter().map(|p| p.id.clone()).collect())
                        .unwrap_or_default();

                    let keep_other: Vec<DiscoveredDeviceResponse> = guard
                        .iter()
                        .filter(|d| {
                            !d.id.starts_with("usb:")
                                && !d.id.starts_with("cgx:")
                                && !d.id.starts_with("ble:")
                                && !d.id.starts_with("cortex:")
                                && !d.id.starts_with("wifi:")
                                && !d.id.starts_with("galea:")
                                && !d.id.starts_with("neurofield:")
                                && !d.id.starts_with("brainbit:")
                                && !d.id.starts_with("gtec:")
                                && !d.id.starts_with("brainmaster:")
                                && !d.id.starts_with("neurosky")
                                && !d.id.starts_with("neurosity:")
                                && !d.id.starts_with("brainvision:")
                                && !d.id.starts_with("rda:")
                        })
                        .cloned()
                        .collect();

                    let current_ids: HashSet<String> =
                        discovered.iter().map(|d| d.id.clone()).collect();

                    let mut merged: Vec<DiscoveredDeviceResponse> = keep_other;
                    for mut d in discovered {
                        // paired_ids (from settings) takes precedence so that
                        // devices remain marked as paired after a daemon restart.
                        d.is_paired = paired_ids.contains(&d.id);
                        if let Some(prev) = old.get(&d.id) {
                            if !d.is_paired {
                                d.is_paired = prev.is_paired;
                            }
                            d.is_preferred = prev.is_preferred;
                        }
                        merged.push(d);
                    }

                    merged.retain(|d| {
                        (!d.id.starts_with("usb:")
                            && !d.id.starts_with("cgx:")
                            && !d.id.starts_with("ble:")
                            && !d.id.starts_with("cortex:")
                            && !d.id.starts_with("wifi:")
                            && !d.id.starts_with("galea:")
                            && !d.id.starts_with("neurofield:")
                            && !d.id.starts_with("brainbit:")
                            && !d.id.starts_with("gtec:")
                            && !d.id.starts_with("brainmaster:")
                            && !d.id.starts_with("neurosky")
                            && !d.id.starts_with("neurosity:")
                            && !d.id.starts_with("brainvision:")
                            && !d.id.starts_with("rda:"))
                            || current_ids.contains(&d.id)
                    });
                    *guard = merged;
                }

                push_device_log(
                    &state,
                    "scanner",
                    &format!("scan tick discovered {} devices", discovered_count),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn ble_filter_accepts_known_eeg_devices() {
        // Muse family
        assert!(is_known_eeg_ble_name("Muse-AB12"));
        assert!(is_known_eeg_ble_name("MuseS-F921")); // Athena
        assert!(is_known_eeg_ble_name("Muse S-1234"));
        assert!(is_known_eeg_ble_name("Muse-2-XY99"));
        assert!(is_known_eeg_ble_name("MUSE-AB12")); // case-insensitive

        // Other EEG families
        assert!(is_known_eeg_ble_name("Ganglion-1234"));
        assert!(is_known_eeg_ble_name("MW75-Neuro"));
        assert!(is_known_eeg_ble_name("Hermes-001"));
        assert!(is_known_eeg_ble_name("Mendi-XY"));
        assert!(is_known_eeg_ble_name("IGE-Guardian"));
        assert!(is_known_eeg_ble_name("BrainBit-EEG"));
        assert!(is_known_eeg_ble_name("Unicorn-EEG"));
        assert!(is_known_eeg_ble_name("AWEAR-E04A8471"));
    }

    #[test]
    fn ble_filter_rejects_unrelated_devices() {
        assert!(!is_known_eeg_ble_name("JBL Flip 5"));
        assert!(!is_known_eeg_ble_name("Apple Watch"));
        assert!(!is_known_eeg_ble_name("iPhone 15"));
        assert!(!is_known_eeg_ble_name("AirPods Pro"));
        // Anonymous UUID-only names (empty string)
        assert!(!is_known_eeg_ble_name(""));
        // Random UUID-style names that BLE devices sometimes advertise
        assert!(!is_known_eeg_ble_name("8282ba24-1ffa-8bd5-659a-4b02f6783927"));
    }

    #[test]
    fn scanner_wifi_detects_wifi_transports() {
        let cfg = ScannerWifiConfigRequest {
            wifi_shield_ip: "192.168.4.1".to_string(),
            galea_ip: "10.0.0.42".to_string(),
        };
        let devices = detect_wifi_devices(&cfg);
        assert_eq!(devices.len(), 2);
        assert!(devices
            .iter()
            .any(|d| d.id == "wifi:192.168.4.1" && d.transport == "wifi"));
        assert!(devices
            .iter()
            .any(|d| d.id == "galea:10.0.0.42" && d.transport == "wifi"));
    }

    #[test]
    fn manual_hints_include_usb_serial_and_wifi() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("test".to_string(), td.path().to_path_buf());
        let hints = detect_manual_device_hints(&state);

        assert!(hints.iter().any(|d| d.id == "neurosky" && d.transport == "usb_serial"));
        assert!(hints
            .iter()
            .any(|d| d.id == "brainvision:127.0.0.1:51244" && d.transport == "wifi"));
    }

    #[test]
    fn ble_cache_filters_stale_and_unknown_devices() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("test".to_string(), td.path().to_path_buf());
        let now = now_unix_secs();

        {
            let mut cache = state.ble_device_cache.lock().unwrap();
            cache.insert("ble:muse".to_string(), (Some("Muse-AB12".to_string()), -48, now));
            cache.insert(
                "ble:stale".to_string(),
                (Some("Muse-OLD".to_string()), -70, now.saturating_sub(121)),
            );
            cache.insert("ble:jbl".to_string(), (Some("JBL Flip 5".to_string()), -30, now));
            cache.insert("ble:noname".to_string(), (None, -40, now));
        }

        let found = read_ble_cache(&state);
        assert_eq!(found.len(), 1, "only fresh known EEG BLE devices should remain");
        assert_eq!(found[0].id, "ble:muse");
        assert_eq!(found[0].transport, "ble");
    }

    #[test]
    fn ble_cache_large_scan_is_fast() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("test".to_string(), td.path().to_path_buf());
        let now = now_unix_secs();

        {
            let mut cache = state.ble_device_cache.lock().unwrap();
            for i in 0..10_000 {
                let id = format!("ble:{i}");
                let name = if i % 2 == 0 {
                    Some(format!("Muse-{i:04}"))
                } else {
                    Some(format!("Speaker-{i:04}"))
                };
                cache.insert(id, (name, -50, now));
            }
        }

        let t0 = std::time::Instant::now();
        let found = read_ble_cache(&state);
        let elapsed = t0.elapsed();

        assert_eq!(found.len(), 5_000);
        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "BLE cache filter too slow: {elapsed:?}"
        );
    }

    #[test]
    fn detect_wifi_devices_empty_config_returns_none() {
        let cfg = ScannerWifiConfigRequest {
            wifi_shield_ip: String::new(),
            galea_ip: String::new(),
        };
        let found = detect_wifi_devices(&cfg);
        assert!(found.is_empty());
    }
}
