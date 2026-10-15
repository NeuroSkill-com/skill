//! Ganglion board — 4-channel EEG via native Bluetooth LE.
//!
//! Uses the [`webbluetooth`] crate to communicate directly with the Ganglion's
//! BLE radio.  No USB dongle or proprietary BGLIB library required.
//!
//! ## BLE characteristics
//! - **Write**: `2d30c083-f39f-4ce6-923f-3484ea480596`
//! - **Notify**: `2d30c082-f39f-4ce6-923f-3484ea480596`
//! - **Software revision**: `00002a28-0000-1000-8000-00805f9b34fb` (firmware version)
//!
//! ## Why webbluetooth and not btleplug
//!
//! btleplug has no session singleton: every `Manager::new().adapters()`
//! allocates another `CBCentralManager` on a thread that never exits, and on
//! macOS a second manager cannot discover peripherals while another one is
//! scanning.  `webbluetooth::Bluetooth::shared()` memoises into a `OnceLock`,
//! so a process has one session no matter how many drivers ask for one — which
//! matters here because the application embedding this driver also talks to
//! other BLE headsets.
//!
//! Two things about Web Bluetooth's model are visible in the code below and
//! have no btleplug counterpart:
//!
//! 1. **Services are named up front.**  A grant covers exactly the services it
//!    lists, and discovery returns nothing else.  [`GanglionConfig::services`]
//!    is that list, and it is configuration rather than a constant because the
//!    Ganglion's service UUID could not be confirmed against hardware here —
//!    see the note on [`SIMBLEE_SERVICE_UUID`].
//! 2. **Notifications are per characteristic.**  `start_notifications()`
//!    returns the stream for one characteristic, so there is no
//!    `notification.uuid` to match on — the Ganglion has exactly one notify
//!    characteristic and the stream is consumed directly.

#[cfg(feature = "ble")]
mod impl_ {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use webbluetooth::chooser::Candidate;
    use webbluetooth::stream::StreamExt;
    use webbluetooth::uuid::BluetoothUuid;
    use webbluetooth::{
        Bluetooth, BluetoothDevice, Grant, LeScanOptions, RemoteGattCharacteristic,
    };

    use crate::board::Board;
    use crate::electrode::ElectrodeLayout;
    use crate::error::{OpenBciError, Result};
    use crate::packet::{decode_ganglion, GanglionState};
    use crate::sample::{Sample, StreamHandle};

    // ── BLE UUIDs ─────────────────────────────────────────────────────────────

    const WRITE_CHAR_UUID:  &str = "2d30c083-f39f-4ce6-923f-3484ea480596";
    const NOTIFY_CHAR_UUID: &str = "2d30c082-f39f-4ce6-923f-3484ea480596";
    const SW_REVISION_UUID: &str = "00002a28-0000-1000-8000-00805f9b34fb";

    /// The service the Ganglion's two data characteristics live under.
    ///
    /// The Ganglion's radio is a Simblee module and `0xfe84` is the service it
    /// advertises.  Taken from OpenBCI's own reference implementation rather
    /// than guessed — `OpenBCI_JavaScript_Utilities/src/constants.js` declares
    /// the whole set, and its test suite asserts each one:
    ///
    /// ```text
    /// const simbleeUuidService    = 'fe84';
    /// const simbleeUuidReceive    = '2d30c082f39f4ce6923f3484ea480596';  // NOTIFY_CHAR_UUID
    /// const simbleeUuidSend       = '2d30c083f39f4ce6923f3484ea480596';  // WRITE_CHAR_UUID
    /// const simbleeUuidDisconnect = '2d30c084f39f4ce6923f3484ea480596';  // unused here
    /// ```
    ///
    /// The two characteristic UUIDs match what this driver already used, which
    /// is the cross-check that makes the service trustworthy: all three come
    /// from the same table.  It has still not been exercised against a physical
    /// Ganglion from this crate, and that is what
    /// [`GanglionConfig::services`] is for — under Web Bluetooth a wrong or
    /// missing service means `get_characteristics` returns nothing and
    /// [`Board::prepare`] fails with "Write char not found", and the fix should
    /// be a config field rather than a new release.  btleplug needed no service
    /// named at all, which is why upstream never recorded one.
    pub const SIMBLEE_SERVICE_UUID: &str = "0000fe84-0000-1000-8000-00805f9b34fb";

    /// Device Information — where the software-revision characteristic used for
    /// firmware autodetection lives (`0x180a`, characteristic `0x2a28`).
    pub const DEVICE_INFORMATION_SERVICE: &str = "0000180a-0000-1000-8000-00805f9b34fb";

    /// Pick one characteristic out of a connection's discovered set.
    ///
    /// Comparison is on the parsed 128-bit value rather than on strings: a
    /// 16-bit UUID has two spellings (`0xfe84` and `0000fe84-0000-1000-8000-…`)
    /// and `BluetoothUuid` normalises both to the same bytes.
    fn find_char(
        chars: &[RemoteGattCharacteristic],
        uuid_str: &str,
    ) -> Option<RemoteGattCharacteristic> {
        let want = BluetoothUuid::parse(uuid_str).ok()?;
        chars.iter().find(|c| *c.uuid() == want).cloned()
    }

    // ─────────────────────────────────────────────────────────────────────────

    /// How to identify the Ganglion during BLE scanning.
    #[derive(Debug, Clone, Default)]
    pub struct GanglionFilter {
        /// If set, only connect to this exact device identifier.
        ///
        /// Formerly the hardware address.  Web Bluetooth reports a per-host
        /// identifier instead — [`webbluetooth::BluetoothDevice::id`] — because
        /// Apple never exposes a peripheral's address to an application.  On
        /// Linux and Windows that identifier *is* the address, so a MAC string
        /// still matches there; on macOS it is a stable per-machine UUID, and a
        /// MAC will match nothing.  Comparison is case-insensitive.
        pub mac_address: Option<String>,
        /// If set, only match peripherals whose name contains this substring
        /// (case-insensitive).  Defaults to matching "Ganglion" or "Simblee".
        pub device_name: Option<String>,
    }

    /// Which firmware version the Ganglion is running.
    #[derive(Debug, Clone, Default)]
    pub enum GanglionFirmware {
        /// Read the software revision characteristic to determine the version.
        #[default]
        Auto,
        V2,
        V3,
    }

    /// Configuration for [`GanglionBoard`].
    #[derive(Debug, Clone)]
    pub struct GanglionConfig {
        /// How long to scan for the device before giving up.
        pub scan_timeout: Duration,
        pub firmware:     GanglionFirmware,
        pub filter:       GanglionFilter,
        /// The GATT services to request access to.
        ///
        /// Web Bluetooth grants exactly what is listed here and discovery
        /// returns nothing outside it, so this is the one setting that can stop
        /// a correct connection from finding its characteristics.  The default
        /// is the Simblee service plus Device Information, which is what the
        /// write/notify characteristics and the firmware probe need.  Anything
        /// [`webbluetooth::uuid::IntoUuid`] accepts is valid: a 128-bit string,
        /// a 16-bit `"fe84"`, or an assigned-numbers name.
        pub services:     Vec<String>,
    }

    impl Default for GanglionConfig {
        fn default() -> Self {
            Self {
                scan_timeout: Duration::from_secs(10),
                firmware:     GanglionFirmware::Auto,
                filter:       GanglionFilter::default(),
                services:     vec![
                    SIMBLEE_SERVICE_UUID.to_owned(),
                    DEVICE_INFORMATION_SERVICE.to_owned(),
                ],
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────

    /// Ganglion 4-channel BLE board.
    ///
    /// # Example
    /// ```rust,no_run
    /// use openbci::board::ganglion::{GanglionBoard, GanglionConfig};
    /// use openbci::board::Board;
    /// use openbci::electrode::ElectrodeLayout;
    ///
    /// let mut board = GanglionBoard::new(GanglionConfig::default())
    ///     .with_electrode_layout(ElectrodeLayout::from_labels(&["Fp1","Fp2","C3","C4"]));
    ///
    /// board.prepare().unwrap();
    /// let stream = board.start_stream().unwrap();
    /// for sample in stream.into_iter().take(200) {
    ///     println!("{:?}", sample.eeg);
    /// }
    /// board.release().unwrap();
    /// ```
    pub struct GanglionBoard {
        config:           GanglionConfig,
        electrode_layout: ElectrodeLayout,
        runtime:          tokio::runtime::Runtime,
        device:           Option<BluetoothDevice>,
        write_char:       Option<RemoteGattCharacteristic>,
        /// The notify characteristic, resolved by [`Board::prepare`].
        ///
        /// btleplug separated `subscribe()` from `notifications()`, so the old
        /// code subscribed in `prepare` and asked for the stream in
        /// `start_stream`.  Here `start_notifications()` *is* the subscription
        /// and it returns the stream, so what is kept between the two calls is
        /// the characteristic and the subscription happens per stream.
        ///
        /// That ordering also keeps `stop_stream` → `start_stream` working: a
        /// consumed stream cannot be restarted, and holding one subscription
        /// for the board's lifetime would make the second `start_stream` fail
        /// with `BoardNotPrepared`.
        notify_char:      Option<RemoteGattCharacteristic>,
        firmware:         u8,
        streaming:        bool,
        keep_alive:       Arc<AtomicBool>,
    }

    impl GanglionBoard {
        /// Create a new Ganglion BLE board driver.
        ///
        /// A multi-threaded Tokio runtime is created internally to drive the
        /// `webbluetooth` futures from this crate's blocking [`Board`] API.
        /// webbluetooth has no runtime of its own — it is `futures-core` plus a
        /// platform backend — so any executor would do; this one is owned by
        /// the struct and lives until [`release`](crate::board::Board::release)
        /// is called.
        pub fn new(config: GanglionConfig) -> Self {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime");

            Self {
                config,
                electrode_layout: ElectrodeLayout::new(4),
                runtime,
                device:      None,
                write_char:  None,
                notify_char: None,
                firmware:    3,
                streaming:   false,
                keep_alive:  Arc::new(AtomicBool::new(false)),
            }
        }

        /// Builder: attach an electrode layout describing the 4 channel sites.
        pub fn with_electrode_layout(mut self, layout: ElectrodeLayout) -> Self {
            self.electrode_layout = layout;
            self
        }

        /// Does this sighting match `filter`?
        ///
        /// Unchanged from the btleplug version apart from where the fields come
        /// from: a scan reports a [`Candidate`], whose `id` replaces the
        /// hardware address and whose `label()` is the advertised local name.
        fn candidate_matches(candidate: &Candidate, filter: &GanglionFilter) -> bool {
            if let Some(ref wanted_id) = filter.mac_address {
                return candidate.id.eq_ignore_ascii_case(wanted_id);
            }
            let Some(ref name) = candidate.name else {
                return false;
            };
            let lc = name.to_lowercase();
            match filter.device_name {
                Some(ref target) => lc.contains(&target.to_lowercase()),
                None => lc.contains("ganglion") || lc.contains("simblee"),
            }
        }

        /// Scan for a Ganglion peripheral matching `filter` and adopt it.
        ///
        /// Returns as soon as something matching advertises, rather than
        /// scanning for the whole window — the same behaviour as the old
        /// 200 ms `adapter.peripherals()` poll, without the polling.  Returns
        /// `None` on timeout.
        ///
        /// A scan grants nothing, so the sighting is adopted with the grant the
        /// connection will need; `grant` is built from
        /// [`GanglionConfig::services`].
        async fn scan_for_ganglion(
            bluetooth: &Bluetooth,
            filter:    &GanglionFilter,
            grant:     Grant,
            timeout:   Duration,
        ) -> Option<BluetoothDevice> {
            // No filter on the scan itself: `GanglionFilter` matches a name
            // *substring* and an id, neither of which `DeviceFilter` expresses
            // (it offers an exact name or a prefix), so the match stays here
            // where the old code did it.
            //
            // `keep_repeated_devices` is load-bearing, not tuning.  Without it
            // each device is reported exactly once, and a peripheral whose
            // first advertisement carries no local name — the name arrives in
            // the scan response — would be judged on that one sighting and
            // never offered again.  The name is what the default filter
            // matches on, so dropping repeats means never finding a Ganglion
            // that advertises its name late.
            let mut scan = bluetooth
                .request_le_scan(
                    LeScanOptions::accept_all_advertisements().keep_repeated_devices(true),
                )
                .await
                .ok()?;

            let found = tokio::time::timeout(timeout, async {
                while let Some(candidate) = scan.next().await {
                    if Self::candidate_matches(&candidate, filter) {
                        log::info!(
                            "Found Ganglion: {} (id={})",
                            candidate.label(),
                            candidate.id
                        );
                        return Some(candidate);
                    }
                }
                None
            })
            .await
            .unwrap_or(None);

            scan.stop(); // stop the radio before connecting to anything

            let candidate = found?;
            match bluetooth.adopt_candidate(&candidate, grant).await {
                Ok(device) => Some(device),
                Err(e) => {
                    log::error!("could not adopt {}: {e}", candidate.id);
                    None
                }
            }
        }

        /// Write `data` to the Ganglion's BLE write characteristic
        /// (without a response) using the internal Tokio runtime.
        fn write_ble(&self, data: &[u8]) -> Result<()> {
            let ch = self.write_char.as_ref().ok_or(OpenBciError::BoardNotPrepared)?;
            let data = data.to_vec();
            let ch_clone = ch.clone();
            self.runtime.block_on(async move {
                ch_clone.write_value_without_response(&data).await
            })?;
            Ok(())
        }
    }

    // ─── Board trait ──────────────────────────────────────────────────────────

    impl Board for GanglionBoard {
        fn prepare(&mut self) -> Result<()> {
            // A live connection needs nothing done.  A STALE one has to be
            // discarded rather than reused: under Web Bluetooth, services and
            // characteristics belong to the connection that produced them, so
            // after the link drops the stored handles fail with
            // `InvalidStateError` rather than reconnecting.  btleplug
            // re-resolved them transparently, which is why the old code could
            // return early on `peripheral.is_some()` alone — a headset that
            // had been switched off and on again would otherwise leave this
            // board permanently unusable.
            if let Some(ref device) = self.device {
                if device.gatt().connected() {
                    return Ok(());
                }
                log::info!("Ganglion link is down — discarding stale handles and rescanning");
                self.keep_alive.store(false, Ordering::Release);
                self.streaming   = false;
                self.device      = None;
                self.write_char  = None;
                self.notify_char = None;
            }

            let timeout  = self.config.scan_timeout;
            let filter   = self.config.filter.clone();
            let services = self.config.services.clone();

            let device = self.runtime.block_on(async move {
                let bluetooth = Bluetooth::shared();

                // Waits for the adapter's first state report — what a btleplug
                // caller did by polling `adapter_state()` for `PoweredOn`.
                bluetooth.availability().await.map_err(|e| {
                    OpenBciError::BoardNotReady(format!("Bluetooth is not usable: {e}"))
                })?;

                let grant = Grant::new().services(&services)?;
                let device =
                    Self::scan_for_ganglion(&bluetooth, &filter, grant, timeout)
                        .await
                        .ok_or_else(|| {
                            OpenBciError::BoardNotReady("Ganglion not found".into())
                        })?;
                Ok::<_, OpenBciError>(device)
            })?;

            let gatt = device.gatt();

            // Connect with retries.  `connect()` has no deadline of its own —
            // it waits for the peripheral to come into range for as long as
            // that takes — so each attempt gets one.
            let mut ok = false;
            for attempt in 1..=3 {
                let connected = self.runtime.block_on(async {
                    tokio::time::timeout(Duration::from_secs(10), gatt.connect()).await
                });
                match connected {
                    Ok(Ok(())) => { ok = true; break; }
                    Ok(Err(e)) => log::warn!("BLE connect attempt {}/3 failed: {e}", attempt),
                    Err(_)     => log::warn!("BLE connect attempt {}/3 timed out", attempt),
                }
                std::thread::sleep(Duration::from_secs(1));
            }
            if !ok {
                return Err(OpenBciError::BoardNotReady("Could not connect to Ganglion".into()));
            }

            // One flat set across every granted service, which is the shape
            // btleplug's `characteristics()` had.  Service discovery is
            // implicit in the call.
            let chars = self.runtime.block_on(gatt.get_characteristics())?;

            let write_char = find_char(&chars, WRITE_CHAR_UUID)
                .ok_or_else(|| OpenBciError::BoardNotReady("Write char not found".into()))?;
            let notify_char = find_char(&chars, NOTIFY_CHAR_UUID)
                .ok_or_else(|| OpenBciError::BoardNotReady("Notify char not found".into()))?;

            // Firmware autodetect
            let firmware = match self.config.firmware {
                GanglionFirmware::V2   => 2,
                GanglionFirmware::V3   => 3,
                GanglionFirmware::Auto => {
                    if let Some(sw_char) = find_char(&chars, SW_REVISION_UUID) {
                        let data = self.runtime
                            .block_on(sw_char.read_value())
                            .unwrap_or_default();
                        if data.first().copied() == Some(b'3') { 3 } else { 2 }
                    } else {
                        3
                    }
                }
            };
            log::info!("Ganglion firmware: {}", firmware);

            self.firmware    = firmware;
            self.write_char  = Some(write_char);
            self.notify_char = Some(notify_char);
            self.device      = Some(device);
            Ok(())
        }

        fn start_stream(&mut self) -> Result<StreamHandle> {
            if self.streaming { return Err(OpenBciError::AlreadyStreaming); }

            // Subscribe before telling the board to stream, or the first
            // packets arrive with nothing listening.  btleplug's ordering was
            // the same; it just happened one call earlier, in `prepare`.
            let notify_char = self.notify_char.clone()
                .ok_or(OpenBciError::BoardNotPrepared)?;
            let mut notifications =
                self.runtime.block_on(notify_char.start_notifications())?;

            self.write_ble(b"b")?;

            let firmware = self.firmware;

            let (sample_tx, sample_rx) = std::sync::mpsc::sync_channel::<Sample>(512);
            let (stop_tx, stop_rx)     = std::sync::mpsc::sync_channel::<()>(1);

            let keep_alive = self.keep_alive.clone();
            keep_alive.store(true, Ordering::Release);

            let tx = sample_tx.clone();
            self.runtime.spawn(async move {
                let mut state = GanglionState::default();

                loop {
                    if stop_rx.try_recv().is_ok() || !keep_alive.load(Ordering::Acquire) { break; }

                    match tokio::time::timeout(
                        Duration::from_millis(50),
                        notifications.next(),
                    ).await {
                        Ok(Some(value)) => {
                            for s in decode_ganglion(&value, &mut state, firmware) {
                                if tx.send(s).is_err() { return; }
                            }
                        }
                        Ok(None) => break,
                        Err(_)   => continue, // timeout → loop back and check stop
                    }
                }
            });

            self.streaming = true;
            Ok(StreamHandle { receiver: sample_rx, stop_tx: Some(stop_tx) })
        }

        fn stop_stream(&mut self) -> Result<()> {
            if !self.streaming { return Err(OpenBciError::NotStreaming); }
            self.keep_alive.store(false, Ordering::Release);
            let _ = self.write_ble(b"s");
            self.streaming = false;
            // The subscription is deliberately left enabled, as it was under
            // btleplug, which had no `unsubscribe` call here either: `s` stops
            // the board from sending, and `start_notifications` is safe to call
            // again on a characteristic that is still notifying — it
            // re-subscribes to the session's hub and re-sends the enable.
            Ok(())
        }

        fn release(&mut self) -> Result<()> {
            if self.streaming { let _ = self.stop_stream(); }
            if let Some(ref device) = self.device {
                let gatt = device.gatt();
                // `disconnect_and_wait` rather than `disconnect`, which only
                // asks: this is the teardown path, and the next `prepare` on a
                // half-disconnected peripheral fails.
                self.runtime.block_on(gatt.disconnect_and_wait());
            }
            self.device      = None;
            self.write_char  = None;
            self.notify_char = None;
            Ok(())
        }

        fn send_command(&mut self, cmd: &str) -> Result<String> {
            self.write_ble(cmd.as_bytes())?;
            Ok(String::new())
        }

        fn electrode_layout(&self) -> &ElectrodeLayout        { &self.electrode_layout }
        fn set_electrode_layout(&mut self, l: ElectrodeLayout) { self.electrode_layout = l; }
        fn channel_count(&self) -> usize                       { 4 }
        fn sampling_rate(&self) -> u32                         { 200 }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public re-exports
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "ble")]
pub use impl_::{
    GanglionBoard, GanglionConfig, GanglionFilter, GanglionFirmware,
    DEVICE_INFORMATION_SERVICE, SIMBLEE_SERVICE_UUID,
};

#[cfg(not(feature = "ble"))]
compile_error!(
    "GanglionBoard requires the `ble` feature.  \
     Enable it in Cargo.toml, or use GanglionWifiBoard instead."
);
