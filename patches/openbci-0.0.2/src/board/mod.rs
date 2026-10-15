//! Board traits and all board-type implementations.
//!
//! The two core traits are:
//! - [`Board`] — lifecycle management and streaming, implemented by every board.
//! - [`ConfigurableBoard`] — per-channel ADS1299 amplifier settings, implemented
//!   by all Cyton-family boards and Galea.
//!
//! # Lifecycle
//!
//! ```text
//! ┌──────────┐   prepare()   ┌──────────────┐  start_stream()  ┌────────────┐
//! │  Created │ ────────────► │    Ready     │ ────────────────► │ Streaming  │
//! └──────────┘               └──────────────┘                   └────────────┘
//!                                   ▲                                  │
//!                                   │           stop_stream() / drop   │
//!                                   └──────────────────────────────────┘
//!                                   │
//!                              release()
//!                            (close port / BLE)
//! ```

use crate::channel_config::ChannelConfig;
use crate::electrode::ElectrodeLayout;
use crate::error::Result;
use crate::sample::StreamHandle;

/// Core trait implemented by every OpenBCI board driver.
///
/// All methods that communicate with the hardware return
/// `Result<_>` so errors can be handled or propagated with `?`.
///
/// # Thread safety
///
/// Boards implement `Send` so they can be moved into worker threads, but they
/// are not `Sync` — concurrent access from multiple threads requires external
/// locking (e.g. `Mutex<Box<dyn Board>>`).
///
/// # Example
///
/// ```rust,no_run
/// use openbci::board::cyton::CytonBoard;
/// use openbci::board::Board;
///
/// let mut board: Box<dyn Board> = Box::new(CytonBoard::new("/dev/ttyUSB0"));
/// board.prepare().unwrap();
/// let stream = board.start_stream().unwrap();
/// for sample in stream.into_iter().take(250) {
///     println!("{:?}", &sample.eeg[..]);
/// }
/// board.release().unwrap();
/// ```
pub trait Board: Send {
    /// Open the communication channel and complete the board handshake.
    ///
    /// For serial boards this opens the USB port, sends a soft-reset (`v`),
    /// and waits for the `$$$` ready marker.  For BLE boards it scans, pairs,
    /// and discovers characteristics.  For WiFi boards it discovers the shield
    /// via SSDP and opens a TCP listener.
    ///
    /// Calling `prepare()` more than once is a no-op if already connected.
    fn prepare(&mut self) -> Result<()>;

    /// Begin data acquisition and return a [`StreamHandle`].
    ///
    /// Sends the `"b"` (begin) command to the board and spawns a background
    /// reader thread.  The returned handle yields decoded [`crate::sample::Sample`]
    /// values; dropping it sends the stop signal automatically.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::OpenBciError::AlreadyStreaming`] if called
    /// while a stream is already active.
    fn start_stream(&mut self) -> Result<StreamHandle>;

    /// Stop the data stream without consuming the [`StreamHandle`].
    ///
    /// Sends the `"s"` (stop) command to the board and signals the reader
    /// thread to exit.  The `StreamHandle` can be dropped after this call.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::OpenBciError::NotStreaming`] if not currently
    /// streaming.
    fn stop_stream(&mut self) -> Result<()>;

    /// Release all hardware resources.
    ///
    /// Stops streaming if active, then closes the serial port, BLE connection,
    /// or UDP socket.  After this call the board object should be discarded or
    /// `prepare()` called again before re-use.
    fn release(&mut self) -> Result<()>;

    /// Send a raw text command to the board and return the board's text response.
    ///
    /// This is a low-level escape hatch for commands not covered by the trait.
    /// The board-specific protocol strings are documented in the
    /// [OpenBCI Cyton SDK](https://docs.openbci.com/Cyton/CytonSDK/) and the
    /// [Ganglion SDK](https://docs.openbci.com/Ganglion/GanglionSDK/).
    ///
    /// **Warning**: sending `"b"` / `"s"` via this method will desynchronise
    /// the driver's internal `streaming` flag.  Prefer [`start_stream`](Board::start_stream)
    /// / [`stop_stream`](Board::stop_stream) instead.
    ///
    /// While streaming, any response from the board is lost (overwritten by
    /// incoming data bytes); in that case an empty string is returned.
    fn send_command(&mut self, cmd: &str) -> Result<String>;

    // ── Electrode layout ──────────────────────────────────────────────────────

    /// Return a reference to the current [`ElectrodeLayout`].
    fn electrode_layout(&self) -> &ElectrodeLayout;

    /// Replace the current [`ElectrodeLayout`].
    ///
    /// Safe to call at any time — the layout is metadata only and does not
    /// affect the board's hardware state.
    fn set_electrode_layout(&mut self, layout: ElectrodeLayout);

    // ── Board metadata ────────────────────────────────────────────────────────

    /// Number of EEG channels produced by this board (4, 8, 16, or 24).
    fn channel_count(&self) -> usize;

    /// Nominal sampling rate in samples per second (e.g. 250 or 200).
    ///
    /// Note: Cyton+Daisy produces 16-channel *merged* samples at ~125 Hz, even
    /// though the underlying hardware runs at 250 Hz.
    fn sampling_rate(&self) -> u32;
}

// ─────────────────────────────────────────────────────────────────────────────

/// Extension trait for boards that support per-channel ADS1299 configuration.
///
/// Implemented by [`cyton::CytonBoard`], [`cyton_daisy::CytonDaisyBoard`],
/// [`cyton_wifi::CytonWifiBoard`], [`cyton_daisy_wifi::CytonDaisyWifiBoard`],
/// and [`galea::GaleaBoard`].
///
/// # ADS1299 channel command format
///
/// Commands take the form `x(CH)(PWR)(GAIN)(INPUT)(BIAS)(SRB2)(SRB1)X`, e.g.:
/// ```text
/// x 1 0 6 0 1 1 0 X
/// │ │ │ │ │ │ │ │ └ end marker
/// │ │ │ │ │ │ │ └── SRB1 (0=off)
/// │ │ │ │ │ │ └──── SRB2 (1=on)
/// │ │ │ │ │ └────── BIAS (1=included)
/// │ │ │ │ └──────── INPUT TYPE (0=normal)
/// │ │ │ └────────── GAIN (6=24×)
/// │ │ └──────────── POWER DOWN (0=on)
/// │ └────────────── CHANNEL ('1' = channel 1)
/// └────────────────  start marker
/// ```
///
/// The [`ChannelConfig`] builder constructs these strings for you.
pub trait ConfigurableBoard: Board {
    /// Apply a [`ChannelConfig`] to one channel (0-based index) and transmit
    /// the resulting command string to the board.
    ///
    /// The [`GainTracker`](crate::channel_config::GainTracker) is updated
    /// immediately so subsequent µV calculations use the correct scale.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::OpenBciError::ChannelOutOfRange`] when
    /// `channel >= board.channel_count()`.
    fn apply_channel_config(&mut self, channel: usize, config: &ChannelConfig) -> Result<()>;

    /// Apply configs to every channel in one call.
    ///
    /// `configs` must have at least `channel_count()` elements; excess entries
    /// are silently ignored.
    fn apply_all_channel_configs(&mut self, configs: &[ChannelConfig]) -> Result<()> {
        for (i, cfg) in configs.iter().enumerate().take(self.channel_count()) {
            self.apply_channel_config(i, cfg)?;
        }
        Ok(())
    }

    /// Send the `"d"` command to restore factory default channel settings.
    ///
    /// This resets all channels to: powered on, 24× gain, normal input,
    /// bias enabled, SRB2 enabled, SRB1 disabled.
    fn reset_to_defaults(&mut self) -> Result<()> {
        self.send_command("d")?;
        Ok(())
    }
}

// ── Sub-modules ───────────────────────────────────────────────────────────────

pub mod cyton;
pub mod cyton_daisy;
pub mod cyton_wifi;
pub mod cyton_daisy_wifi;
pub mod ganglion;
pub mod ganglion_wifi;
pub mod galea;
pub(crate) mod wifi_shield;
