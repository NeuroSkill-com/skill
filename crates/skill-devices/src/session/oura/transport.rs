// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! The ring's BLE link: an [`oura_link::transport::Transport`] over `webbluetooth`.
//!
//! `oura-link` ships its own btleplug backend behind a default `ble` feature.
//! We build with `default-features = false` and implement its `Transport` trait
//! here instead, so the ring reaches the radio through
//! [`webbluetooth::Bluetooth::shared`] — the same process-wide session every
//! other device in this app uses.  Two sessions in one process means two
//! `CBCentralManager`s on macOS, and a device found through one cannot be
//! connected through the other; see `docs/webbluetooth-migration.md`.
//!
//! The ring speaks one vendor service with a request characteristic and one or
//! more notify characteristics.  Ring 5 adds characteristics Ring 3 does not
//! have, so every notify/indicate characteristic in the service is subscribed
//! and merged into a single broadcast channel — which is exactly the shape
//! `Transport::subscribe` wants, and keeps the protocol layer generation-blind.

use std::time::Duration;

use futures::StreamExt as _;
use oura_link::transport::Transport;
use oura_link::{Error, Result};
use oura_protocol::protocol;
use tokio::sync::broadcast;
use webbluetooth::{
    Bluetooth, BluetoothDevice, BluetoothUuid, DeviceFilter, Grant, LeScanOptions, RemoteGattCharacteristic,
};

/// The ring's vendor GATT service, in webbluetooth's spelling.
///
/// `oura_protocol` states these as `uuid::Uuid`; both constructors are `const`,
/// so the conversion costs nothing at runtime.
pub const OURA_SERVICE: BluetoothUuid = BluetoothUuid::from_u128(protocol::OURA_SERVICE.as_u128());

/// The characteristic every request frame is written to.
pub const OURA_WRITE: BluetoothUuid = BluetoothUuid::from_u128(protocol::OURA_WRITE.as_u128());

/// Upper bound on the adopt + connect + discover + subscribe phase.
///
/// CoreBluetooth imposes no deadline of its own: a ring that advertises but
/// will not finish the GATT handshake — typically because a phone still holds
/// its single peripheral link — would otherwise block the caller forever.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Notification fan-in depth.  Matches what `oura-link`'s own backend uses; a
/// reader that falls this far behind loses the oldest frame rather than
/// stalling the radio callback.
const NOTIFY_CAPACITY: usize = 256;

fn ble(e: impl std::fmt::Display) -> Error {
    Error::Ble(e.to_string())
}

/// A ring seen while scanning.
#[derive(Clone, Debug)]
pub struct Discovered {
    /// Per-host device identifier (`BluetoothDevice.id`) — a system UUID on Apple.
    pub id: String,
    /// Advertised name, or `"(unnamed)"` for a bonded ring that stopped sending one.
    pub name: String,
    /// Signal strength in dBm, when the radio reported one.
    pub rssi: Option<i32>,
}

/// Whether an advertised local name matches the caller's needle.
///
/// The scan already requires the vendor service UUID, so the name is only a
/// secondary filter — and a bonded ring routinely advertises no name at all.
/// Treating that as a miss is upstream's
/// [issue #13](https://github.com/Th0rgal/open_oura/issues/13), so an empty
/// name passes the default `"oura"` needle.  A more specific needle still
/// requires a name, so `"Ring 5"` does not match every unnamed device nearby.
pub fn name_matches(advertised: &str, needle: &str) -> bool {
    let needle = needle.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    if advertised.is_empty() {
        return needle == "oura";
    }
    advertised.to_lowercase().contains(&needle)
}

/// Scan options that let the radio filter on the vendor service for us.
///
/// `DeviceFilter::matches` checks the Apple "overflow" service area as well as
/// the normal list, which matters here: macOS moves 128-bit service UUIDs there
/// routinely, and the ring's is 128-bit.
fn scan_options() -> Result<LeScanOptions> {
    let filter = DeviceFilter::new().service(OURA_SERVICE).map_err(ble)?;
    Ok(LeScanOptions::new().filter(filter).keep_repeated_devices(true))
}

/// Collect ring sightings for `timeout`, strongest signal first.
pub async fn scan(name_contains: &str, timeout: Duration) -> Result<Vec<Discovered>> {
    let mut found: Vec<Discovered> = Vec::new();
    for_each_sighting(name_contains, timeout, |cand| {
        let entry = Discovered {
            id: cand.id.clone(),
            name: match cand.name.clone().unwrap_or_default() {
                n if n.is_empty() => "(unnamed)".into(),
                n => n,
            },
            rssi: cand.rssi(),
        };
        match found.iter_mut().find(|d| d.id == entry.id) {
            // A later sighting carries a fresher RSSI.
            Some(existing) => *existing = entry,
            None => found.push(entry),
        }
    })
    .await?;

    found.sort_by_key(|d| std::cmp::Reverse(d.rssi.unwrap_or(i32::MIN)));
    Ok(found)
}

/// Run a filtered LE scan for `timeout`, handing every matching sighting to `on_sighting`.
///
/// The scan stops when the [`webbluetooth::LeScan`] is dropped, which is why it
/// is held for exactly this function's body.
async fn for_each_sighting(
    name_contains: &str,
    timeout: Duration,
    mut on_sighting: impl FnMut(&webbluetooth::AdvertisementEvent),
) -> Result<()> {
    let mut scan = Bluetooth::shared()
        .request_le_scan(scan_options()?)
        .await
        .map_err(ble)?;

    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Ok(());
        }
        match tokio::time::timeout(remaining, scan.next()).await {
            Ok(Some(cand)) => {
                if name_matches(cand.name.as_deref().unwrap_or_default(), name_contains) {
                    on_sighting(&cand);
                }
            }
            // Scan ended early, or the window elapsed mid-wait.
            _ => return Ok(()),
        }
    }
}

/// A connected BLE link to a ring.
pub struct WebBluetoothTransport {
    device: BluetoothDevice,
    write_char: RemoteGattCharacteristic,
    tx: broadcast::Sender<Vec<u8>>,
    /// One pump per subscribed notify characteristic.  Each owns its
    /// `Notifications` stream, so aborting a pump also stops that subscription.
    pumps: Vec<tokio::task::JoinHandle<()>>,
}

impl WebBluetoothTransport {
    /// Scan for a ring and open a link to it.
    ///
    /// Picks the strongest advertiser matching `name_contains`; when `id` is
    /// given, only that device is considered (the caller already knows which
    /// ring it wants, so there is nothing to choose).
    pub async fn connect(name_contains: &str, id: Option<&str>, scan_timeout: Duration) -> Result<Self> {
        let chosen = Self::find(name_contains, id, scan_timeout).await?;
        Self::open(&chosen).await
    }

    /// Resolve the device id to connect to.
    async fn find(name_contains: &str, id: Option<&str>, scan_timeout: Duration) -> Result<String> {
        let mut best: Option<(String, i32)> = None;
        for_each_sighting(name_contains, scan_timeout, |cand| {
            if let Some(want) = id {
                if !cand.id.eq_ignore_ascii_case(want) {
                    return;
                }
            }
            let rssi = cand.rssi().unwrap_or(i32::MIN);
            if best.as_ref().is_none_or(|(_, b)| rssi > *b) {
                best = Some((cand.id.clone(), rssi));
            }
        })
        .await?;

        best.map(|(id, _)| id).ok_or(Error::DeviceNotFound)
    }

    /// Adopt, connect, discover, and subscribe — the whole setup phase, bounded.
    async fn open(id: &str) -> Result<Self> {
        let session = Bluetooth::shared();

        // Web Bluetooth's per-device allowlist: we know the one service we
        // speak, so the grant names it rather than reaching for the
        // `unrestricted` escape hatch meant for BLE explorers.
        let mut grant = Grant::new();
        grant.services.insert(OURA_SERVICE);
        let device = session.adopt_device(id, grant).await.map_err(ble)?;

        tokio::time::timeout(CONNECT_TIMEOUT, Self::setup(device))
            .await
            .map_err(|_| Error::ConnectTimeout)?
    }

    async fn setup(device: BluetoothDevice) -> Result<Self> {
        let gatt = device.gatt();
        if !gatt.connected() {
            gatt.connect().await.map_err(ble)?;
        }

        let service = gatt.get_primary_service(OURA_SERVICE).await.map_err(ble)?;
        let chars = service.get_characteristics(None).await.map_err(ble)?;

        let write_char = chars
            .iter()
            .find(|c| *c.uuid() == OURA_WRITE)
            .cloned()
            .ok_or_else(|| Error::CharacteristicNotFound(OURA_WRITE.to_string()))?;

        let (tx, _) = broadcast::channel(NOTIFY_CAPACITY);
        let mut pumps = Vec::new();
        for c in chars
            .iter()
            .filter(|c| c.properties().notify() || c.properties().indicate())
        {
            // Boxed so the stream is `Unpin` for `next()`; the pump owns it,
            // and dropping it is what unsubscribes.
            let mut notifications = Box::pin(c.start_notifications().await.map_err(ble)?);
            let out = tx.clone();
            pumps.push(tokio::spawn(async move {
                while let Some(frame) = notifications.next().await {
                    // `Err` only means nobody is listening yet, which is
                    // allowed: the protocol layer subscribes per request.
                    let _ = out.send(frame);
                }
            }));
        }

        if pumps.is_empty() {
            return Err(Error::CharacteristicNotFound(format!(
                "no notify characteristic in service {OURA_SERVICE}"
            )));
        }

        Ok(Self {
            device,
            write_char,
            tx,
            pumps,
        })
    }

    /// The ring's per-host device identifier.
    pub fn id(&self) -> &str {
        self.device.id()
    }

    /// The ring's name, when it advertises one (a bonded ring often does not).
    pub fn name(&self) -> Option<String> {
        self.device.name()
    }

    /// Drop the link.  Idempotent.
    pub async fn disconnect(&self) {
        self.stop_pumps();
        self.device.gatt().disconnect();
    }

    fn stop_pumps(&self) {
        for pump in &self.pumps {
            pump.abort();
        }
    }
}

impl Drop for WebBluetoothTransport {
    fn drop(&mut self) {
        // Without this the pumps outlive the transport and hold their
        // subscriptions (and the device handle) open.
        self.stop_pumps();
    }
}

#[async_trait::async_trait]
impl Transport for WebBluetoothTransport {
    async fn write(&self, data: &[u8]) -> Result<()> {
        self.write_char.write_value_with_response(data).await.map_err(ble)
    }

    fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two UUID spellings must agree; a mismatch here is a scan that never
    /// matches and a write that never resolves, with nothing to show why.
    #[test]
    fn uuids_match_the_protocol_crate() {
        assert_eq!(OURA_SERVICE.to_string(), protocol::OURA_SERVICE.to_string());
        assert_eq!(OURA_WRITE.to_string(), protocol::OURA_WRITE.to_string());
    }

    #[test]
    fn default_needle_accepts_an_unnamed_bonded_ring() {
        assert!(name_matches("", "Oura"));
        assert!(name_matches("", "oura"));
        assert!(name_matches("Oura Ring 4", "Oura"));
    }

    #[test]
    fn specific_needle_still_requires_a_name() {
        assert!(!name_matches("", "Ring 5"));
        assert!(name_matches("Oura Ring 5", "Ring 5"));
        assert!(!name_matches("Oura Ring 4", "Ring 5"));
    }

    #[test]
    fn empty_needle_matches_everything() {
        assert!(name_matches("", ""));
        assert!(name_matches("anything", "  "));
    }
}
