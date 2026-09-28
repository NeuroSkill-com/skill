//! btleplug's `platform` types, implemented over one shared webbluetooth session.
//!
//! Upstream ships one of these per OS.  There is one here, because webbluetooth
//! already covers CoreBluetooth, WinRT, BlueZ, Android and the browser behind a
//! single surface.
//!
//! ## The invariant this file exists to hold
//!
//! Every path to the radio goes through [`webbluetooth::Bluetooth::shared()`],
//! which memoises into a `OnceLock`.  `Bluetooth::new()` and
//! `Bluetooth::with_chooser()` each build a *separate* session — a second
//! `CBCentralManager` — so neither appears anywhere in this crate, and neither
//! is reachable through btleplug's API, which has no concept that maps to them.
//! There is a test at the bottom asserting `shared()` really is idempotent.

use crate::api::{
    CentralEvent, CentralState, Characteristic, Descriptor, Peripheral as _, PeripheralProperties, Service,
    ValueNotification, WriteType,
};
use crate::common::adapter_manager::AdapterManager;
use crate::common::util::notifications_stream_from_broadcast_receiver;
use crate::{Error, Result};
use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use std::collections::{BTreeSet, HashMap};
use std::fmt::{self, Debug, Formatter};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use webbluetooth::{Bluetooth, BluetoothDevice, Candidate, Grant, LeScanOptions};

pub use crate::api::BDAddr;

// ── PeripheralId ─────────────────────────────────────────────────────────────

/// A peripheral's identity.
///
/// Upstream makes this a per-platform newtype: a `BDAddr` where the OS reveals
/// an address, and an opaque system UUID on Apple, which never does.
/// webbluetooth's `BluetoothDevice::id` is the same thing with the same split —
/// the address where there is one, a system-generated UUID on Apple — so this
/// wraps it as a string and lets `Display` keep printing what callers logged
/// before.
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct PeripheralId(pub(crate) String);

impl fmt::Display for PeripheralId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<BDAddr> for PeripheralId {
    fn from(addr: BDAddr) -> Self {
        PeripheralId(addr.to_string())
    }
}

impl PeripheralId {
    /// The identifier as webbluetooth spells it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ── Manager ──────────────────────────────────────────────────────────────────

/// The entry point. Cheap: it holds nothing.
#[derive(Clone, Debug)]
pub struct Manager {}

impl Manager {
    pub async fn new() -> Result<Self> {
        Ok(Self {})
    }
}

#[async_trait]
impl crate::api::Manager for Manager {
    type Adapter = Adapter;

    async fn adapters(&self) -> Result<Vec<Adapter>> {
        // One adapter, holding the one shared session.  Upstream allocated a
        // CBCentralManager and leaked a thread here, on every call.
        Ok(vec![Adapter::shared()])
    }
}

// ── Adapter ──────────────────────────────────────────────────────────────────

/// A handle onto the process-wide BLE session.
///
/// Cloning is free and gives the same session, which matters: btleplug callers
/// treat an `Adapter` as something they own, and several of the consuming crates
/// build one per operation (`mw75` does it in both `scan_all()` and
/// `connect()`).  All of those now converge instead of multiplying.
#[derive(Clone)]
pub struct Adapter {
    inner: Arc<AdapterInner>,
}

struct AdapterInner {
    session: Bluetooth,
    manager: AdapterManager<Peripheral>,
    /// The running scan, if any.
    ///
    /// btleplug's `start_scan`/`stop_scan` are adapter-wide and idempotent, and
    /// an `Adapter` here is a shared handle rather than something owned — so a
    /// scan is refcounted by *holders asking for it*, not by `Adapter` clones.
    /// Dropping one holder must not stop a scan another still wants, and
    /// `stop_scan` from any holder stops it for all, which is what upstream
    /// does.
    scan: Mutex<Option<ScanState>>,
}

#[derive(Debug)]
struct ScanState {
    /// Cancels the pump task, which owns the `LeScan` and so ends the radio
    /// scan when dropped.
    pump: tokio::task::JoinHandle<()>,
}

impl Debug for Adapter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Adapter")
            .field(
                "scanning",
                &self.inner.scan.lock().map(|g| g.is_some()).unwrap_or(false),
            )
            .finish()
    }
}

impl Adapter {
    fn shared() -> Self {
        // The whole point of this crate. See the module docs.
        static SHARED: std::sync::OnceLock<Adapter> = std::sync::OnceLock::new();
        SHARED
            .get_or_init(|| Adapter {
                inner: Arc::new(AdapterInner {
                    session: Bluetooth::shared(),
                    manager: AdapterManager::default(),
                    scan: Mutex::new(None),
                }),
            })
            .clone()
    }

    /// Record a sighting: register the peripheral if new, and emit the
    /// `CentralEvent`s btleplug callers expect from a scan.
    fn observe(&self, candidate: Candidate) {
        let id = PeripheralId(candidate.id.clone());
        let known = self.inner.manager.peripheral(&id).is_some();

        if known {
            if let Some(mut p) = self.inner.manager.peripheral_mut(&id) {
                p.value_mut().merge_advertisement(&candidate);
            }
        } else {
            self.inner
                .manager
                .add_peripheral(Peripheral::discovered(self.inner.session.clone(), &candidate));
        }

        self.inner.manager.emit(if known {
            CentralEvent::DeviceUpdated(id.clone())
        } else {
            CentralEvent::DeviceDiscovered(id.clone())
        });

        // Upstream emits these alongside discovery, and several consumers read
        // manufacturer data from the event rather than from `properties()`.
        let adv = &candidate.advertisement;
        if !adv.manufacturer_data.is_empty() {
            self.inner.manager.emit(CentralEvent::ManufacturerDataAdvertisement {
                id: id.clone(),
                manufacturer_data: adv.manufacturer_data.clone(),
            });
        }
        if !adv.service_data.is_empty() {
            self.inner.manager.emit(CentralEvent::ServiceDataAdvertisement {
                id: id.clone(),
                service_data: adv
                    .service_data
                    .iter()
                    .filter_map(|(k, v)| to_uuid(k).map(|u| (u, v.clone())))
                    .collect(),
            });
        }
        if !adv.service_uuids.is_empty() {
            self.inner.manager.emit(CentralEvent::ServicesAdvertisement {
                id,
                services: adv.service_uuids.iter().filter_map(to_uuid).collect(),
            });
        }
    }
}

#[async_trait]
impl crate::api::Central for Adapter {
    type Peripheral = Peripheral;

    async fn events(&self) -> Result<Pin<Box<dyn Stream<Item = CentralEvent> + Send>>> {
        Ok(self.inner.manager.event_stream())
    }

    async fn start_scan(&self, filter: crate::api::ScanFilter) -> Result<()> {
        // The lock is taken twice on purpose and never held across the await:
        // it guards a `std::sync::Mutex`, whose guard is not `Send`, and
        // `#[async_trait]` needs this future to be.
        if self.inner.scan.lock()?.is_some() {
            // Already scanning: idempotent, as upstream is.
            return Ok(());
        }

        // btleplug's ScanFilter is a service-UUID allowlist, and an empty one
        // means "everything".  webbluetooth requires the intent to be explicit,
        // so an empty filter becomes accept-all rather than a validation error.
        let mut options = LeScanOptions::accept_all_advertisements();
        if !filter.services.is_empty() {
            let mut f = webbluetooth::DeviceFilter::new();
            for uuid in &filter.services {
                f = f.service(uuid.to_string().as_str())?;
            }
            options = LeScanOptions::new().filter(f);
        }
        // Repeated sightings are what keep `DeviceUpdated` and the manufacturer
        // -data events flowing; upstream reports every advertisement.
        options = options.keep_repeated_devices(true);

        let mut scan = self.inner.session.request_le_scan(options).await?;
        let adapter = self.clone();
        // The task owns the LeScan, so aborting it drops the scan and stops the
        // radio — that is `stop_scan`.
        let pump = tokio::spawn(async move {
            while let Some(candidate) = scan.next().await {
                adapter.observe(candidate);
            }
        });

        // Two callers can reach the await together, since the lock was released.
        // Whoever stores first wins and the loser drops its own scan, so the
        // radio is left with exactly one either way.
        let mut guard = self.inner.scan.lock()?;
        if guard.is_some() {
            pump.abort();
            return Ok(());
        }
        *guard = Some(ScanState { pump });
        Ok(())
    }

    async fn stop_scan(&self) -> Result<()> {
        let mut guard = self.inner.scan.lock()?;
        if let Some(state) = guard.take() {
            state.pump.abort();
        }
        Ok(())
    }

    async fn peripherals(&self) -> Result<Vec<Peripheral>> {
        Ok(self.inner.manager.peripherals())
    }

    async fn peripheral(&self, id: &PeripheralId) -> Result<Peripheral> {
        self.inner.manager.peripheral(id).ok_or(Error::DeviceNotFound)
    }

    async fn add_peripheral(&self, id: &PeripheralId) -> Result<Peripheral> {
        // Reaching a device by identifier without having seen it advertise.
        // webbluetooth can do this — `adopt_device` takes an id — but only for
        // one it already knows, so an unseen device is still DeviceNotFound.
        let device = self
            .inner
            .session
            .adopt_device(id.as_str(), Grant::unrestricted())
            .await?;
        let peripheral = Peripheral::adopted(self.inner.session.clone(), device);
        if self.inner.manager.peripheral(id).is_none() {
            self.inner.manager.add_peripheral(peripheral.clone());
        }
        Ok(peripheral)
    }

    async fn adapter_info(&self) -> Result<String> {
        let info = self.inner.session.adapter().await?;
        Ok(format!("{info:?}"))
    }

    async fn adapter_state(&self) -> Result<CentralState> {
        Ok(if self.inner.session.get_availability().await {
            CentralState::PoweredOn
        } else {
            CentralState::PoweredOff
        })
    }
}

// ── Peripheral ───────────────────────────────────────────────────────────────

/// One remote device.
#[derive(Clone)]
pub struct Peripheral {
    inner: Arc<PeripheralInner>,
}

struct PeripheralInner {
    session: Bluetooth,
    id: PeripheralId,
    /// Set once the device has been adopted (a grant obtained).  btleplug has no
    /// permission model, so this happens lazily on first use rather than being
    /// something a caller asks for.
    device: Mutex<Option<BluetoothDevice>>,
    /// Latest advertisement data, refreshed by every sighting.
    props: Mutex<PeripheralProperties>,
    /// Services discovered by `discover_services`, empty until then — btleplug
    /// specifies exactly that.
    services: Mutex<BTreeSet<Service>>,
    /// The merged notification stream.
    ///
    /// btleplug hands out ONE stream per peripheral covering every subscribed
    /// characteristic, valid before any connection and across reconnects.
    /// webbluetooth subscribes per characteristic, so each `subscribe` spawns a
    /// pump that tags its values with the characteristic UUID and forwards them
    /// here.  The channel is created with the peripheral, which is what makes
    /// `notifications()` work before `connect()`.
    notifications: tokio::sync::broadcast::Sender<ValueNotification>,
    /// One pump per subscribed characteristic, so `unsubscribe` can stop just
    /// that one.
    pumps: Mutex<HashMap<Uuid, tokio::task::JoinHandle<()>>>,
}

impl Debug for Peripheral {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Peripheral")
            .field("id", &self.inner.id)
            .field("name", &self.inner.props.lock().ok().and_then(|p| p.local_name.clone()))
            .finish()
    }
}

impl Peripheral {
    fn new(session: Bluetooth, id: PeripheralId, device: Option<BluetoothDevice>) -> Self {
        let (notifications, _) = tokio::sync::broadcast::channel(256);
        Peripheral {
            inner: Arc::new(PeripheralInner {
                session,
                id,
                device: Mutex::new(device),
                props: Mutex::new(PeripheralProperties::default()),
                services: Mutex::new(BTreeSet::new()),
                notifications,
                pumps: Mutex::new(HashMap::new()),
            }),
        }
    }

    fn discovered(session: Bluetooth, candidate: &Candidate) -> Self {
        let me = Self::new(session, PeripheralId(candidate.id.clone()), None);
        me.merge_advertisement(candidate);
        me
    }

    fn adopted(session: Bluetooth, device: BluetoothDevice) -> Self {
        let id = PeripheralId(device.id().to_string());
        Self::new(session, id, Some(device))
    }

    /// Fold a fresh advertisement into the cached properties.
    fn merge_advertisement(&self, candidate: &Candidate) {
        let adv = &candidate.advertisement;
        if let Ok(mut props) = self.inner.props.lock() {
            props.address = self.address();
            props.address_type = None;
            props.local_name = candidate.name.clone().or_else(|| adv.local_name.clone());
            props.tx_power_level = adv.tx_power.map(|p| p as i16);
            props.rssi = candidate.rssi().map(|r| r as i16);
            props.manufacturer_data = adv.manufacturer_data.clone();
            props.service_data = adv
                .service_data
                .iter()
                .filter_map(|(k, v)| to_uuid(k).map(|u| (u, v.clone())))
                .collect();
            props.services = adv.service_uuids.iter().filter_map(to_uuid).collect();
            props.class = None;
        }
    }

    /// The `BluetoothDevice`, adopting it on first use.
    ///
    /// btleplug callers never ask for permission — they scan and connect — so
    /// the grant is taken here, unrestricted, at the first operation that needs
    /// a handle.
    async fn device(&self) -> Result<BluetoothDevice> {
        if let Some(d) = self.inner.device.lock()?.clone() {
            return Ok(d);
        }
        let device = self
            .inner
            .session
            .adopt_device(self.inner.id.as_str(), Grant::unrestricted())
            .await?;
        *self.inner.device.lock()? = Some(device.clone());
        Ok(device)
    }

    /// Find the webbluetooth characteristic behind a btleplug one.
    async fn resolve(&self, characteristic: &Characteristic) -> Result<webbluetooth::RemoteGattCharacteristic> {
        let device = self.device().await?;
        let service = device
            .gatt()
            .get_primary_service(characteristic.service_uuid.to_string().as_str())
            .await?;
        Ok(service
            .get_characteristic(characteristic.uuid.to_string().as_str())
            .await?)
    }

    /// Find the webbluetooth descriptor behind a btleplug one.
    ///
    /// btleplug addresses a descriptor by the (service, characteristic,
    /// descriptor) UUID triple rather than by handle, so the characteristic is
    /// rebuilt from the first two and resolved. `resolve` reads neither the
    /// flags nor the descriptor set, so the empty ones here cost nothing.
    async fn resolve_descriptor(&self, descriptor: &Descriptor) -> Result<webbluetooth::RemoteGattDescriptor> {
        let ch = self
            .resolve(&Characteristic {
                uuid: descriptor.characteristic_uuid,
                service_uuid: descriptor.service_uuid,
                properties: crate::api::CharPropFlags::empty(),
                descriptors: BTreeSet::new(),
            })
            .await?;
        Ok(ch.get_descriptor(descriptor.uuid.to_string().as_str()).await?)
    }
}

#[async_trait]
impl crate::api::Peripheral for Peripheral {
    fn id(&self) -> PeripheralId {
        self.inner.id.clone()
    }

    fn address(&self) -> BDAddr {
        // webbluetooth reports an address everywhere except Apple, where
        // CoreBluetooth substitutes a system UUID — exactly the platforms where
        // upstream btleplug also cannot produce a real one.  A device with no
        // address parses as all-zeroes, which is upstream's placeholder too.
        let from_device = self
            .inner
            .device
            .lock()
            .ok()
            .and_then(|d| d.as_ref().and_then(|d| d.address()));

        // On Apple the id is that system UUID and will not parse, which is the
        // same all-zeroes placeholder upstream falls back to there.
        from_device
            .as_deref()
            .unwrap_or_else(|| self.inner.id.as_str())
            .parse()
            .unwrap_or_else(|_| BDAddr::from([0; 6]))
    }

    async fn properties(&self) -> Result<Option<PeripheralProperties>> {
        Ok(Some(self.inner.props.lock()?.clone()))
    }

    fn services(&self) -> BTreeSet<Service> {
        self.inner.services.lock().map(|s| s.clone()).unwrap_or_default()
    }

    async fn is_connected(&self) -> Result<bool> {
        let Some(device) = self.inner.device.lock()?.clone() else {
            return Ok(false);
        };
        Ok(device.gatt().connected())
    }

    async fn connect(&self) -> Result<()> {
        let device = self.device().await?;
        device.gatt().connect().await?;
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        if let Some(device) = self.inner.device.lock()?.clone() {
            device.gatt().disconnect();
        }
        // Subscriptions do not survive a disconnect; the broadcast channel does,
        // because btleplug promises the stream stays valid across connections.
        if let Ok(mut pumps) = self.inner.pumps.lock() {
            for (_, pump) in pumps.drain() {
                pump.abort();
            }
        }
        Ok(())
    }

    async fn discover_services(&self) -> Result<()> {
        let device = self.device().await?;
        let mut discovered = BTreeSet::new();

        for service in device.gatt().get_primary_services(None).await? {
            let service_uuid = to_uuid(service.uuid()).ok_or(Error::UnexpectedCharacteristic)?;
            let mut characteristics = BTreeSet::new();

            for ch in service.get_characteristics(None).await? {
                let uuid = to_uuid(ch.uuid()).ok_or(Error::UnexpectedCharacteristic)?;
                let mut descriptors = BTreeSet::new();
                // Descriptors are best-effort: a peer may refuse to enumerate
                // them, and btleplug still expects the service list.
                if let Ok(list) = ch.get_descriptors().await {
                    for d in list {
                        if let Some(duuid) = to_uuid(d.uuid()) {
                            descriptors.insert(Descriptor {
                                uuid: duuid,
                                service_uuid,
                                characteristic_uuid: uuid,
                            });
                        }
                    }
                }
                characteristics.insert(Characteristic {
                    uuid,
                    service_uuid,
                    properties: to_char_props(ch.properties()),
                    descriptors,
                });
            }

            discovered.insert(Service {
                uuid: service_uuid,
                primary: service.is_primary(),
                characteristics,
            });
        }

        *self.inner.services.lock()? = discovered;
        Ok(())
    }

    async fn write(&self, characteristic: &Characteristic, data: &[u8], write_type: WriteType) -> Result<()> {
        let ch = self.resolve(characteristic).await?;
        match write_type {
            WriteType::WithResponse => ch.write_value_with_response(data).await?,
            WriteType::WithoutResponse => ch.write_value_without_response(data).await?,
        }
        Ok(())
    }

    async fn read(&self, characteristic: &Characteristic) -> Result<Vec<u8>> {
        Ok(self.resolve(characteristic).await?.read_value().await?)
    }

    async fn subscribe(&self, characteristic: &Characteristic) -> Result<()> {
        let uuid = characteristic.uuid;
        // Already subscribed: idempotent, and re-subscribing would double every
        // notification into the merged stream.
        if self.inner.pumps.lock()?.contains_key(&uuid) {
            return Ok(());
        }

        let ch = self.resolve(characteristic).await?;
        let mut tagged = ch.start_notifications().await?.tagged();
        let out = self.inner.notifications.clone();

        // The fan-in: one pump per characteristic, all feeding the peripheral's
        // single broadcast channel, each value tagged with the UUID it came
        // from.  This is what turns webbluetooth's per-characteristic streams
        // into btleplug's one-stream-per-peripheral contract.
        let pump = tokio::spawn(async move {
            while let Some((from, value)) = tagged.next().await {
                let Some(uuid) = to_uuid(&from) else { continue };
                // Err only means nobody is listening yet, which is fine: the
                // caller is allowed to subscribe before reading notifications.
                let _ = out.send(ValueNotification { uuid, value });
            }
        });

        self.inner.pumps.lock()?.insert(uuid, pump);
        Ok(())
    }

    async fn unsubscribe(&self, characteristic: &Characteristic) -> Result<()> {
        if let Some(pump) = self.inner.pumps.lock()?.remove(&characteristic.uuid) {
            pump.abort();
        }
        // Best-effort on the wire: the pump is already gone, so nothing reaches
        // the merged stream either way.
        if let Ok(ch) = self.resolve(characteristic).await {
            let _ = ch.stop_notifications().await;
        }
        Ok(())
    }

    async fn notifications(&self) -> Result<Pin<Box<dyn Stream<Item = ValueNotification> + Send>>> {
        Ok(notifications_stream_from_broadcast_receiver(
            self.inner.notifications.subscribe(),
        ))
    }

    async fn write_descriptor(&self, descriptor: &Descriptor, data: &[u8]) -> Result<()> {
        self.resolve_descriptor(descriptor).await?.write_value(data).await?;
        Ok(())
    }

    async fn read_descriptor(&self, descriptor: &Descriptor) -> Result<Vec<u8>> {
        Ok(self.resolve_descriptor(descriptor).await?.read_value().await?)
    }
}

// ── Conversions ──────────────────────────────────────────────────────────────

/// webbluetooth's `BluetoothUuid` is 36 ASCII bytes; btleplug's is `uuid::Uuid`.
fn to_uuid(uuid: &webbluetooth::BluetoothUuid) -> Option<Uuid> {
    uuid.to_string().parse().ok()
}

/// The low 8 bits of webbluetooth's property word are the GATT characteristic
/// property bits, in the same order as btleplug's flags — BROADCAST 0x01
/// through EXTENDED_PROPERTIES 0x80.  The bits above that (the
/// encryption-required pair) have no btleplug equivalent and are dropped.
fn to_char_props(props: webbluetooth::CharacteristicProperties) -> crate::api::CharPropFlags {
    crate::api::CharPropFlags::from_bits_truncate((props.0 & 0xFF) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Manager as _;

    /// No `#[tokio::test]` here — see Cargo.toml for why there are no
    /// dev-dependencies.
    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("current-thread runtime")
            .block_on(fut)
    }

    /// The reason this crate exists.  Two `Manager`s, several `adapters()`
    /// calls, and every one must be the same session — upstream allocated a new
    /// `CBCentralManager` (and leaked a thread) per call.
    #[test]
    fn every_adapter_is_the_same_session() {
        block_on(async {
            let a = Manager::new().await.unwrap().adapters().await.unwrap();
            let b = Manager::new().await.unwrap().adapters().await.unwrap();
            let c = Manager::new().await.unwrap().adapters().await.unwrap();

            assert_eq!(a.len(), 1, "one adapter, as upstream reports on Apple/Windows");
            for adapters in [&a, &b, &c] {
                assert!(Arc::ptr_eq(&a[0].inner, &adapters[0].inner));
            }
        });
    }

    /// Cloning an `Adapter` must not fork the session either — several consumers
    /// build one per operation.
    #[test]
    fn cloning_an_adapter_keeps_one_session() {
        block_on(async {
            let adapter = Manager::new().await.unwrap().adapters().await.unwrap().remove(0);
            let cloned = adapter.clone();
            assert!(Arc::ptr_eq(&adapter.inner, &cloned.inner));
        });
    }

    #[test]
    fn characteristic_properties_map_bit_for_bit() {
        use crate::api::CharPropFlags;
        use webbluetooth::CharacteristicProperties as W;

        assert_eq!(to_char_props(W(0x002)), CharPropFlags::READ);
        assert_eq!(to_char_props(W(0x008)), CharPropFlags::WRITE);
        assert_eq!(to_char_props(W(0x010)), CharPropFlags::NOTIFY);
        assert_eq!(to_char_props(W(0x020)), CharPropFlags::INDICATE);
        assert_eq!(
            to_char_props(W(0x002 | 0x010)),
            CharPropFlags::READ | CharPropFlags::NOTIFY
        );
        // The encryption-required bits are above the byte and have no btleplug
        // equivalent; they must not corrupt the flags below them.
        assert_eq!(to_char_props(W(0x100 | 0x002)), CharPropFlags::READ);
    }

    #[test]
    fn peripheral_id_round_trips_through_display() {
        let id = PeripheralId("AA:BB:CC:DD:EE:FF".into());
        assert_eq!(id.to_string(), "AA:BB:CC:DD:EE:FF");
        assert_eq!(id.as_str(), "AA:BB:CC:DD:EE:FF");
    }
}
