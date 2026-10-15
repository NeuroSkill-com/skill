use std::collections::{HashMap, HashSet};
use std::time::Duration;

use futures::StreamExt;
use skill_daemon_common::{ble_id, DiscoveredDeviceResponse, ScannerWifiConfigRequest};
use skill_data::device::DeviceKind;
use tokio::sync::oneshot;
use webbluetooth::uuid::BluetoothUuid;
use webbluetooth::{Advertisement, Bluetooth, LeScanOptions};

use tracing::{debug, info, warn};

use crate::state::AppState;
use crate::util::{now_unix_secs, push_device_log};

/// How long a sighting counts as current.
///
/// Covers the worst-case connection attempt window — 600 ms pause + 5 s scan
/// + 10 s connect + 15 s discover + margin — so a device does not vanish from
/// the list while it is being connected to.
const SIGHTING_FRESH_SECS: u64 = 120;

/// Return `true` when a BLE advertising name looks like a supported EEG/neurofeedback device.
pub(crate) fn is_known_eeg_ble_name(name: &str) -> bool {
    // Families whose name needs more care than a prefix test are left to the
    // device registry, which owns the pattern: Vital Neuro's name is
    // user-settable and the word is shared with unrelated medical wearables.
    // Oura is here for the same reason: "oura" is a substring of unrelated
    // product names, and a bare `starts_with` here would surface something the
    // connect route then declines to treat as a ring.
    if matches!(
        DeviceKind::from_name(Some(name)),
        DeviceKind::Vital | DeviceKind::Oura | DeviceKind::Whoop
    ) {
        return true;
    }

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

/// Device families that an advertised service UUID identifies on its own.
///
/// Names are the usual route, but they are not always usable: Vital Neuro
/// stores its name in the unit's NVS and lets the owner change it, so no
/// prefix reliably matches one — the `vital` crate filters on the service for
/// exactly that reason.  The value is the device-kind tag used for routing.
const SERVICE_KIND_TABLE: &[(BluetoothUuid, &str)] = &[
    // Vital Neuro's single GATT service, `0x2D0D`, named in its advertisement.
    (BluetoothUuid::from_u16(0x2D0D), "vital"),
    // The Oura ring's vendor service, `98ed0001-…`.  Here for a different
    // reason than Vital's: the ring's name is fixed, but a ring that has been
    // bonded routinely advertises no local name at all, so the name is simply
    // not there to match on.  The service UUID always is.
    //
    // Used directly rather than re-spelled as a literal: the types only match
    // if the daemon and `skill-devices` resolved the *same* webbluetooth
    // package, so this also fails the build if the graph ever splits in two —
    // the one-session invariant `docs/webbluetooth-migration.md` describes.
    (skill_devices::session::oura::transport::OURA_SERVICE, "oura"),
    // WHOOP advertises a different vendor service per generation — 4.0 is
    // `61080001-…`, 5.0/MG is `fd4b0001-…` — so both map to the same kind.
    // The strap's name is user-settable and carries no identity, which makes
    // the service the only reliable discriminator, exactly as for Vital.
    //
    // Taken from `poop`'s own constants rather than re-spelled here: a literal
    // that drifts from the driver is a strap that advertises but never routes.
    (whoop_service(skill_devices::poop::protocol::WHOOP4_SERVICE), "whoop"),
    (whoop_service(skill_devices::poop::protocol::WHOOP5_SERVICE), "whoop"),
];

/// `poop` states its services as `uuid::Uuid`; the scanner speaks
/// `BluetoothUuid`. Both constructors are `const`, so this costs nothing and
/// keeps the table a `const`.
const fn whoop_service(u: uuid::Uuid) -> BluetoothUuid {
    BluetoothUuid::from_u128(u.as_u128())
}

/// The device-kind tag for an advertisement, when a service UUID identifies it.
///
/// Apple reports some advertised services in an "overflow" area instead of the
/// normal list, so both are checked.
pub(crate) fn kind_from_advertisement(adv: &Advertisement) -> Option<&'static str> {
    SERVICE_KIND_TABLE
        .iter()
        .find(|(uuid, _)| adv.service_uuids.contains(uuid) || adv.overflow_service_uuids.contains(uuid))
        .map(|(_, kind)| *kind)
}

/// Canonical ids of the devices currently advertising `kind`'s service.
///
/// The connect path uses this to tell "the one headset in the room" from "one
/// of several", which decides whether a scan may stop at the first match or
/// has to run to completion and pick by id.
pub(crate) fn fresh_ids_for_service_kind(state: &AppState, kind: &str) -> Vec<String> {
    let now = now_unix_secs();
    // One lock at a time: every other site takes these in this order, and
    // holding both at once is how that stops being true by accident.
    let candidates: Vec<String> = match state.ble_service_kinds.lock() {
        Ok(kinds) => kinds
            .iter()
            .filter(|(_, k)| **k == kind)
            .map(|(id, _)| id.clone())
            .collect(),
        Err(_) => return Vec::new(),
    };
    if candidates.is_empty() {
        return Vec::new();
    }
    let Ok(cache) = state.ble_device_cache.lock() else {
        return Vec::new();
    };
    candidates
        .into_iter()
        .filter(|id| {
            cache
                .get(id)
                .is_some_and(|(_, _, last_seen)| now.saturating_sub(*last_seen) <= SIGHTING_FRESH_SECS)
        })
        .collect()
}

/// Fallback display name for a device recognised only by its service UUID and
/// advertising no name of its own.
fn service_kind_display_name(kind: &'static str) -> &'static str {
    match kind {
        "vital" => "Vital Neuro",
        "oura" => "Oura Ring",
        "whoop" => "WHOOP",
        other => other,
    }
}

/// Return `true` when a discovered device is known to be an EEG headset —
/// by its advertising name, or because it advertised an identifying service.
///
/// The auto-pair paths use this to prefer a real headset over the manual
/// hints that also live in the discovered list (serial dongles, network
/// relays), so a device whose *name* says nothing is not demoted below them.
pub(crate) fn is_known_eeg_device(state: &AppState, dev: &DiscoveredDeviceResponse) -> bool {
    if is_known_eeg_ble_name(&dev.name) {
        return true;
    }
    state
        .ble_service_kinds
        .lock()
        .map(|kinds| kinds.keys().any(|id| ble_id::same_device(id, &dev.id)))
        .unwrap_or(false)
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

/// Read the current BLE device cache and return the devices that look like a
/// known EEG/neurofeedback headset — by advertising name, or by an advertised
/// service UUID for the families whose names are not dependable.  Entries not
/// seen within the last 60 seconds are suppressed (but kept in the cache for
/// name recall).
pub(crate) fn read_ble_cache(state: &AppState) -> Vec<DiscoveredDeviceResponse> {
    let now = now_unix_secs();
    let service_kinds = state.ble_service_kinds.lock().map(|g| g.clone()).unwrap_or_default();
    let Ok(cache) = state.ble_device_cache.lock() else {
        return Vec::new();
    };
    cache
        .iter()
        .filter_map(|(id, (name_opt, rssi, last_seen))| {
            // Must be recognisable one way or the other.
            let name = match (name_opt.as_deref(), service_kinds.get(id.as_str())) {
                // A name we recognise — or any name at all on a device the
                // advertised service already identified, since that one may
                // simply have been renamed by its owner.
                (Some(n), kind) if kind.is_some() || is_known_eeg_ble_name(n) => n.to_string(),
                // Identified, but advertising no name to show.
                (None, Some(kind)) => service_kind_display_name(kind).to_string(),
                _ => return None,
            };
            // Suppress stale entries; they stay in the cache for name recall.
            if now.saturating_sub(*last_seen) > SIGHTING_FRESH_SECS {
                return None;
            }
            Some(DiscoveredDeviceResponse {
                id: id.clone(),
                name,
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
/// Subscribes to `requestLEScan` on the process-wide [`Bluetooth::shared`]
/// session and folds each advertisement into `state.ble_device_cache` (name,
/// RSSI, last-seen).
///
/// The session is shared with every device crate that connects through
/// webbluetooth, which is the point: one `CBCentralManager` / one BlueZ
/// connection for the whole daemon, so a scan no longer has to be torn down
/// and rebuilt for a connect to be able to succeed.  webbluetooth says so
/// explicitly — "the radio scan is shared, so this coexists with a
/// `request_device` in flight".
///
/// Two consequences worth knowing:
///
/// * `availability()` replaces the old retry-until-a-manager-appears loop.  It
///   waits for the adapter's first state report (bounded, 5 s) and reports
///   *why* the radio is unusable, so `Unauthorized` — the macOS TCC denial
///   that used to present as "the scan finds nothing, silently" — now shows up
///   in the log as itself.
/// * The scan handle is a resource: dropping [`webbluetooth::LeScan`] stops
///   the radio (once no other watcher wants it), which is how the pause below
///   is implemented.  The session itself is never dropped — it is a process
///   singleton, and dropping it would invalidate every device adopted from it.
async fn run_ble_listener_task(state: AppState) {
    let bluetooth = Bluetooth::shared();

    loop {
        // Stop when the outer scanner has been turned off.
        if !state.scanner_running.lock().map(|g| *g).unwrap_or(false) {
            return;
        }

        // Wait for the adapter, and say why when there isn't a usable one.
        if let Err(why) = bluetooth.availability().await {
            warn!(%why, "Bluetooth is not usable — retrying in 5 s");
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }

        // `keep_repeated_devices` is off in the specification by default, which
        // would report each device once and then go quiet — RSSI and last-seen
        // would freeze at their first value and `read_ble_cache`'s 120 s
        // staleness filter would eventually hide every device that is still
        // sitting there advertising.
        let options = LeScanOptions::accept_all_advertisements().keep_repeated_devices(true);

        let mut scan = match bluetooth.request_le_scan(options).await {
            Ok(scan) => scan,
            Err(e) => {
                warn!(error = %e, "could not start BLE scan — retrying in 5 s");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        // Fold advertisements until the stream ends or a flag tells us to stop.
        loop {
            // Short timeout so `scanner_running` is checked frequently even
            // when no advertisements are arriving.
            let maybe_event = tokio::time::timeout(Duration::from_millis(300), scan.next()).await;

            if !state.scanner_running.lock().map(|g| *g).unwrap_or(false) {
                return;
            }

            match maybe_event {
                // Scan ended — break to the outer loop to restart it.
                Ok(None) => break,
                // Timeout — just re-check the flags and carry on.
                Err(_) => continue,
                Ok(Some(sighting)) => {
                    // webbluetooth reports `NSUUID.UUIDString` on Apple, which
                    // is uppercase where btleplug's `Uuid` Display was lower.
                    // Canonicalising keeps the key byte-identical to what every
                    // pre-migration `paired_devices.json` already holds.
                    let key = ble_id::canonical_target(&format!("ble:{}", sighting.id));
                    let name = sighting.name.clone();
                    // Not every platform reports RSSI; 0 is the cache's
                    // "unknown" and must not overwrite a real reading.
                    let rssi = sighting.rssi().unwrap_or(0) as i16;

                    if let Some(ref n) = name {
                        debug!(ble_name = %n, rssi, "BLE advertisement");
                    }
                    // A device whose name cannot identify it may still name
                    // its service in the advertisement.  Remembered per id so
                    // discovery and connect routing can both use it.
                    if let Some(kind) = kind_from_advertisement(&sighting.advertisement) {
                        if let Ok(mut kinds) = state.ble_service_kinds.lock() {
                            if kinds.insert(key.clone(), kind).is_none() {
                                info!(id = %key, kind, "BLE device identified by service UUID");
                            }
                        }
                    }

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
        }

        // Stop the radio: this is the only handle, so dropping it is what
        // `stop_scan()` used to be.  Explicit rather than end-of-scope.
        scan.stop();

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

/// Process-wide ANT Neuro SDK handle, built at most once.
///
/// `AntNeuroSdk::new` constructs a `NativeBackend`, which logs its SDK version
/// at INFO unconditionally. The scanner probes on every other 5 s tick, so
/// building a fresh SDK per probe emitted a `start log version …` line every
/// 10 s for the life of the daemon — even on machines with no ANT Neuro
/// hardware attached. `antneuro::Backend` is `Send + Sync`, so the handle is
/// safe to build once and re-enumerate amplifiers from on every later tick.
/// `None` means construction failed (no vendor library / no USB backend); we
/// cache that too rather than retrying — and logging — forever.
static ANTNEURO_SDK: std::sync::OnceLock<Option<antneuro::prelude::AntNeuroSdk>> = std::sync::OnceLock::new();

pub(crate) fn detect_antneuro_devices() -> Vec<DiscoveredDeviceResponse> {
    use antneuro::prelude::*;
    let sdk = ANTNEURO_SDK.get_or_init(|| {
        let config = AntNeuroConfig::default();
        AntNeuroSdk::new(&config.library_path).ok()
    });
    let Some(sdk) = sdk.as_ref() else {
        return Vec::new();
    };
    let amps = sdk.get_amplifiers_info().unwrap_or_default();
    amps.into_iter()
        .map(|info| {
            let name = format!("ANT Neuro eego ({})", info.serial);
            let id = format!("antneuro:{}", info.id);
            DiscoveredDeviceResponse {
                id,
                name,
                last_seen: now_unix_secs(),
                last_rssi: 0,
                is_paired: false,
                is_preferred: false,
                transport: "usb".to_string(),
            }
        })
        .collect()
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

    let _ = state;
    let settings_device_id = Some(skill_settings::keychain::get_neurosity_device_id()).filter(|s| !s.trim().is_empty());

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

    // Adaptive scan cadence. The scanner auto-starts at daemon boot and would
    // otherwise probe every transport (USB serial, BLE cache, Cortex,
    // NeuroField, BrainBit, g.tec, ANT Neuro, BrainMaster) every 5s forever —
    // including for users with no device paired and nothing on the bus, which
    // is the most expensive shape because every probe runs to completion. We
    // start at the fast cadence so a freshly-plugged-in device appears within
    // a few seconds, then back off to a 30s cadence after ~5 minutes of no
    // discoveries and no paired devices. Any discovery or user-driven
    // start/stop resets us to the fast cadence.
    const FAST_TICK: Duration = Duration::from_secs(5);
    const SLOW_TICK: Duration = Duration::from_secs(30);
    const EMPTY_TICKS_BEFORE_BACKOFF: u32 = 60; // 60 * 5s = 5 min
    let mut tick = tokio::time::interval(FAST_TICK);
    let mut current_period = FAST_TICK;
    let mut empty_ticks: u32 = 0;
    let mut cortex_tick = 0u64;

    loop {
        tokio::select! {
            _ = &mut stop_rx => break,
            _ = tick.tick() => {
                let tick_start = std::time::Instant::now();
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

                // ANT Neuro eego (USB)
                let antneuro_discovered = if cortex_tick.is_multiple_of(2) {
                    tokio::task::spawn_blocking(detect_antneuro_devices)
                        .await
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                discovered.extend(antneuro_discovered);

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
                    // Canonically keyed: these come off disk, so an entry
                    // written by a pre-webbluetooth build can differ from the
                    // scanner's spelling by case alone — and then a paired
                    // headset shows up in the list as unpaired.
                    let paired_ids: HashSet<String> = state
                        .status
                        .lock()
                        .map(|s| {
                            s.paired_devices
                                .iter()
                                .map(|p| ble_id::canonical_target(&p.id))
                                .collect()
                        })
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
                                && !d.id.starts_with("antneuro:")
                        })
                        .cloned()
                        .collect();

                    let current_ids: HashSet<String> =
                        discovered.iter().map(|d| d.id.clone()).collect();

                    let mut merged: Vec<DiscoveredDeviceResponse> = keep_other;
                    for mut d in discovered {
                        // paired_ids (from settings) takes precedence so that
                        // devices remain marked as paired after a daemon restart.
                        d.is_paired = paired_ids.contains(&ble_id::canonical_target(&d.id));
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
                            && !d.id.starts_with("rda:")
                            && !d.id.starts_with("antneuro:"))
                            || current_ids.contains(&d.id)
                    });
                    *guard = merged;
                }

                push_device_log(
                    &state,
                    "scanner",
                    &format!("scan tick discovered {} devices", discovered_count),
                );

                // Adaptive backoff: count this tick as "empty" only when we
                // have no paired devices to reconnect to AND no transport
                // turned anything up. With paired devices we keep the fast
                // cadence so a quick plug-in gets reconnected promptly.
                let has_paired = state
                    .status
                    .lock()
                    .map(|s| !s.paired_devices.is_empty())
                    .unwrap_or(false);
                if discovered_count == 0 && !has_paired {
                    empty_ticks = empty_ticks.saturating_add(1);
                } else {
                    if current_period != FAST_TICK {
                        info!("[scanner] device activity — returning to fast scan cadence");
                        tick = tokio::time::interval(FAST_TICK);
                        // Skip the immediate first tick (interval fires once on creation).
                        tick.tick().await;
                        current_period = FAST_TICK;
                    }
                    empty_ticks = 0;
                }
                if empty_ticks >= EMPTY_TICKS_BEFORE_BACKOFF && current_period != SLOW_TICK {
                    info!(
                        "[scanner] no devices found in {} ticks and none paired — backing off to {}s cadence",
                        empty_ticks,
                        SLOW_TICK.as_secs()
                    );
                    tick = tokio::time::interval(SLOW_TICK);
                    tick.tick().await;
                    current_period = SLOW_TICK;
                }

                state.record_task_heartbeat("device-scanner", tick_start.elapsed().as_millis() as u64);
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
        assert!(is_known_eeg_ble_name("Mendi-XY"));
        assert!(is_known_eeg_ble_name("IGE-Guardian"));
        assert!(is_known_eeg_ble_name("BrainBit-EEG"));
        assert!(is_known_eeg_ble_name("Unicorn-EEG"));
        assert!(is_known_eeg_ble_name("AWEAR-E04A8471"));
        assert!(is_known_eeg_ble_name("Vital Neuro"));
        assert!(is_known_eeg_ble_name("VitalNeuro-1234"));
        assert!(is_known_eeg_ble_name("Vital-04A8"));
    }

    /// "Vital" is a word other medical wearables use.  Offering a cardiac
    /// patch as an EEG headset would be worse than missing a renamed Vital
    /// Neuro, which the service-UUID path catches anyway.
    #[test]
    fn ble_filter_rejects_unrelated_vital_devices() {
        assert!(!is_known_eeg_ble_name("VitalPatch"));
        assert!(!is_known_eeg_ble_name("Vital Signs Monitor"));
        assert!(!is_known_eeg_ble_name("vitality band"));
    }

    /// Auto-pair prefers "a real headset" over the manual hints that share the
    /// discovered list.  A device whose name says nothing still counts when the
    /// scanner saw it advertise an identifying service.
    #[test]
    fn known_eeg_device_accepts_a_service_identified_rename() {
        let td = TempDir::new().unwrap();
        let state = AppState::new_for_tests("test".to_string(), td.path().to_path_buf());
        let renamed = DiscoveredDeviceResponse {
            id: "ble:1234-ABCD".into(),
            name: "Bob".into(),
            last_seen: 0,
            last_rssi: -50,
            is_paired: false,
            is_preferred: false,
            transport: "ble".into(),
        };
        let unrelated = DiscoveredDeviceResponse {
            id: "ble:9999".into(),
            name: "Bob 2".into(),
            ..renamed.clone()
        };

        assert!(!is_known_eeg_device(&state, &renamed), "nothing known about it yet");

        state
            .ble_service_kinds
            .lock()
            .unwrap()
            .insert("ble:1234-abcd".to_string(), "vital");

        // Case-insensitive on the id, like every other comparison site.
        assert!(is_known_eeg_device(&state, &renamed));
        assert!(!is_known_eeg_device(&state, &unrelated));
        // A name that speaks for itself needs no sighting at all.
        assert!(is_known_eeg_device(
            &state,
            &DiscoveredDeviceResponse {
                name: "Muse-AB12".into(),
                ..unrelated.clone()
            }
        ));
    }

    #[test]
    fn fresh_ids_for_service_kind_skips_stale_and_other_kinds() {
        let td = TempDir::new().unwrap();
        let state = AppState::new_for_tests("test".to_string(), td.path().to_path_buf());
        let now = now_unix_secs();

        {
            let mut cache = state.ble_device_cache.lock().unwrap();
            cache.insert("ble:fresh".to_string(), (Some("Vital Neuro".into()), -50, now));
            cache.insert(
                "ble:stale".to_string(),
                (
                    Some("Vital Neuro".into()),
                    -50,
                    now.saturating_sub(SIGHTING_FRESH_SECS + 1),
                ),
            );
            cache.insert("ble:other".to_string(), (Some("Something".into()), -50, now));
            // In the kinds map but never seen advertising: no cache entry.
            let mut kinds = state.ble_service_kinds.lock().unwrap();
            kinds.insert("ble:fresh".to_string(), "vital");
            kinds.insert("ble:stale".to_string(), "vital");
            kinds.insert("ble:other".to_string(), "something-else");
            kinds.insert("ble:unseen".to_string(), "vital");
        }

        assert_eq!(
            fresh_ids_for_service_kind(&state, "vital"),
            vec!["ble:fresh".to_string()]
        );
        assert!(fresh_ids_for_service_kind(&state, "nothing-advertises-this").is_empty());
    }

    /// The service UUID the table matches on must stay the one the `vital`
    /// crate connects with — it is spelled out here because
    /// `BluetoothUuid::from_u16` is `const` and `From<Uuid>` is not.
    #[test]
    fn vital_service_uuid_matches_the_driver() {
        let from_driver = BluetoothUuid::from(skill_devices::vital::protocol::VITAL_SERVICE_UUID);
        assert!(SERVICE_KIND_TABLE
            .iter()
            .any(|(uuid, kind)| *uuid == from_driver && *kind == "vital"));
    }

    /// Every tag in the table has to be a kind the registry knows, because
    /// that string is what both discovery and connect routing resolve against.
    #[test]
    fn service_kind_tags_are_known_device_kinds() {
        for (_, kind) in SERVICE_KIND_TABLE {
            let parsed = DeviceKind::from_kind_str(kind);
            assert_ne!(parsed, DeviceKind::Unknown, "unknown service-kind tag {kind:?}");
            assert_eq!(parsed.as_str(), *kind, "tag {kind:?} is not the registry's spelling");
        }
    }

    #[test]
    fn advertised_service_identifies_a_renamed_device() {
        let mut adv = Advertisement::default();
        assert_eq!(kind_from_advertisement(&adv), None);

        adv.service_uuids.push(BluetoothUuid::from_u16(0x2D0D));
        assert_eq!(kind_from_advertisement(&adv), Some("vital"));

        // Apple can report the same service in the overflow area instead.
        let mut overflow = Advertisement::default();
        overflow.overflow_service_uuids.push(BluetoothUuid::from_u16(0x2D0D));
        assert_eq!(kind_from_advertisement(&overflow), Some("vital"));
    }

    /// The reason the ring is in the service table at all: once bonded it
    /// advertises no local name, so the vendor service UUID is the only thing
    /// left to recognise it by.  A 128-bit UUID is also exactly what Apple
    /// moves into the overflow area, so both lists must be checked.
    #[test]
    fn advertised_service_identifies_a_nameless_oura_ring() {
        let service = skill_devices::session::oura::transport::OURA_SERVICE;

        let mut adv = Advertisement::default();
        adv.service_uuids.push(service);
        assert_eq!(kind_from_advertisement(&adv), Some("oura"));
        // The name is absent, which must not change the verdict.
        assert_eq!(adv.local_name, None);

        let mut overflow = Advertisement::default();
        overflow.overflow_service_uuids.push(service);
        assert_eq!(kind_from_advertisement(&overflow), Some("oura"));
    }

    /// Discovery and connect routing must agree on what a ring is.  The
    /// scanner used to answer this with its own `starts_with("oura")`, which is
    /// wider than the registry's rule — so a device could appear in the list
    /// and then be routed somewhere else entirely.
    #[test]
    fn oura_name_recognition_matches_the_registry() {
        for name in ["Oura", "Oura Ring Gen3", "oura-1234", "OURA_5"] {
            assert!(is_known_eeg_ble_name(name), "should be offered: {name}");
        }
        for name in ["ourabox", "colourable", "aurora"] {
            assert!(!is_known_eeg_ble_name(name), "should not be offered: {name}");
            assert_ne!(DeviceKind::from_name(Some(name)), DeviceKind::Oura);
        }
    }

    /// WHOOP advertises a *different* vendor service per generation, so both
    /// must resolve to the same kind — registering only one silently drops
    /// half the supported hardware.
    #[test]
    fn both_whoop_generations_are_recognised_by_service() {
        for service in [
            skill_devices::poop::protocol::WHOOP4_SERVICE,
            skill_devices::poop::protocol::WHOOP5_SERVICE,
        ] {
            let uuid = BluetoothUuid::from_u128(service.as_u128());

            let mut adv = Advertisement::default();
            adv.service_uuids.push(uuid);
            assert_eq!(kind_from_advertisement(&adv), Some("whoop"), "service {service}");

            // Apple routinely moves 128-bit services to the overflow area.
            let mut overflow = Advertisement::default();
            overflow.overflow_service_uuids.push(uuid);
            assert_eq!(kind_from_advertisement(&overflow), Some("whoop"));
        }
    }

    /// The two generations are different services and must not be confused.
    #[test]
    fn the_two_whoop_services_are_distinct() {
        assert_ne!(
            skill_devices::poop::protocol::WHOOP4_SERVICE,
            skill_devices::poop::protocol::WHOOP5_SERVICE
        );
    }

    #[test]
    fn whoop_has_a_display_name_for_a_nameless_sighting() {
        assert_eq!(service_kind_display_name("whoop"), "WHOOP");
    }

    /// A ring recognised only by its service still needs something to show in
    /// the device list.
    #[test]
    fn oura_has_a_display_name_for_a_nameless_sighting() {
        assert_eq!(service_kind_display_name("oura"), "Oura Ring");
    }

    #[test]
    fn ble_cache_surfaces_devices_identified_by_service_uuid() {
        let td = TempDir::new().unwrap();
        let state = AppState::new_for_tests("test".to_string(), td.path().to_path_buf());
        let now = now_unix_secs();

        {
            let mut cache = state.ble_device_cache.lock().unwrap();
            // Renamed by its owner: the name identifies nothing.
            cache.insert("ble:renamed".to_string(), (Some("Bob".to_string()), -50, now));
            // Advertising no name at all.
            cache.insert("ble:nameless".to_string(), (None, -55, now));
            // Same, but never seen advertising a known service.
            cache.insert("ble:other".to_string(), (Some("Bob 2".to_string()), -60, now));

            let mut kinds = state.ble_service_kinds.lock().unwrap();
            kinds.insert("ble:renamed".to_string(), "vital");
            kinds.insert("ble:nameless".to_string(), "vital");
        }

        let found = read_ble_cache(&state);
        assert_eq!(found.len(), 2, "unidentified devices must stay hidden");
        let renamed = found.iter().find(|d| d.id == "ble:renamed").unwrap();
        assert_eq!(renamed.name, "Bob", "an owner's own name is kept");
        let nameless = found.iter().find(|d| d.id == "ble:nameless").unwrap();
        assert_eq!(nameless.name, "Vital Neuro", "a nameless device gets a label");
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
        let state = AppState::new_for_tests("test".to_string(), td.path().to_path_buf());
        let hints = detect_manual_device_hints(&state);

        assert!(hints.iter().any(|d| d.id == "neurosky" && d.transport == "usb_serial"));
        assert!(hints
            .iter()
            .any(|d| d.id == "brainvision:127.0.0.1:51244" && d.transport == "wifi"));
    }

    #[test]
    fn ble_cache_filters_stale_and_unknown_devices() {
        let td = TempDir::new().unwrap();
        let state = AppState::new_for_tests("test".to_string(), td.path().to_path_buf());
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

    /// The cache key the listener builds from a webbluetooth sighting must come
    /// out byte-identical to what a pre-migration (btleplug) build wrote, or
    /// every device in `paired_devices.json` stops matching after the upgrade.
    ///
    /// webbluetooth reports `NSUUID.UUIDString` on Apple — uppercase — where
    /// btleplug's `PeripheralId(Uuid)` Displayed lowercase.
    #[test]
    fn listener_cache_key_matches_the_pre_migration_spelling() {
        // What CoreBluetooth hands webbluetooth.
        let sighting_id = "5A3C1E88-9B2D-4F60-A1C7-77E0D4B93F12";
        // What the btleplug-era scanner wrote, and what is on disk today.
        let legacy_key = "ble:5a3c1e88-9b2d-4f60-a1c7-77e0d4b93f12";

        let key = ble_id::canonical_target(&format!("ble:{sighting_id}"));
        assert_eq!(key, legacy_key);

        // And it survives a round trip through the cache as a known device.
        let td = TempDir::new().unwrap();
        let state = AppState::new_for_tests("test".to_string(), td.path().to_path_buf());
        state
            .ble_device_cache
            .lock()
            .unwrap()
            .insert(key.clone(), (Some("Muse-AB12".to_string()), -48, now_unix_secs()));

        let found = read_ble_cache(&state);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, legacy_key);
    }

    #[test]
    fn ble_cache_large_scan_is_fast() {
        let td = TempDir::new().unwrap();
        let state = AppState::new_for_tests("test".to_string(), td.path().to_path_buf());
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
