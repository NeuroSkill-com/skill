//! The [`Sample`] type and the [`StreamHandle`] streaming interface.

use std::time::{SystemTime, UNIX_EPOCH};

/// A single decoded data sample returned by any OpenBCI board.
///
/// All voltage values are in **microvolts (µV)**, accelerometer values in
/// **g**, and timestamps as **seconds since the UNIX epoch** (host clock).
///
/// The `eeg` vector length always equals the board's channel count:
/// 4 (Ganglion), 8 (Cyton), 16 (Cyton+Daisy), or 24 (Galea).
///
/// # Which aux fields are populated?
///
/// The Cyton's 33-byte packet includes 6 auxiliary bytes whose meaning
/// depends on the end byte:
///
/// | `end_byte` | `accel` | `analog` |
/// |---|---|---|
/// | `0xC0` | ✅ 3-axis accelerometer | — |
/// | `0xC1` | — | ✅ 3 analog pin readings |
/// | `0xC2`–`0xC6` | — | — |
///
/// The Ganglion always populates `accel`.  Galea populates neither.
#[derive(Debug, Clone)]
pub struct Sample {
    /// Board-assigned rolling counter (0–255), wrapping after 255.
    ///
    /// Gaps indicate dropped packets.
    pub sample_num: u8,

    /// EEG/EMG channel values in **microvolts (µV)**.
    ///
    /// Length equals the board's [`channel_count()`](crate::Board::channel_count).
    pub eeg: Vec<f64>,

    /// On-board accelerometer reading in **g** (X, Y, Z).
    ///
    /// `Some` when the packet carries accelerometer data
    /// (`end_byte == 0xC0` on Cyton, or always on Ganglion).
    pub accel: Option<[f64; 3]>,

    /// Analog pin readings (raw ADC counts).
    ///
    /// `Some` only when the board is in analog mode (`end_byte == 0xC1`).
    pub analog: Option<[f64; 3]>,

    /// Per-channel impedance in **ohms** from the Ganglion's z-check mode.
    /// Order: `[ch1, ch2, ch3, ch4, ref]`.
    pub resistance: Option<Vec<f64>>,

    /// Host-side UNIX timestamp in seconds (assigned when the sample is decoded,
    /// not when the board captured it).
    pub timestamp: f64,

    /// The board's end byte for this packet.
    ///
    /// - `0xC0` — standard (accelerometer in aux bytes)
    /// - `0xC1` — analog pin mode
    /// - `0xC2`–`0xC6` — board-defined extended modes
    pub end_byte: u8,

    /// The 6 raw auxiliary bytes from the Cyton packet, as received over
    /// the wire.  Interpretation depends on `end_byte`.
    pub aux_bytes: [u8; 6],
}

impl Sample {
    /// Construct a zeroed `Sample` for a board with `num_eeg_channels` channels.
    ///
    /// Used internally by packet decoders before filling in real values.
    pub fn zeroed(num_eeg_channels: usize) -> Self {
        Self {
            sample_num: 0,
            eeg:        vec![0.0; num_eeg_channels],
            accel:      None,
            analog:     None,
            resistance: None,
            timestamp:  0.0,
            end_byte:   0xC0,
            aux_bytes:  [0u8; 6],
        }
    }
}

/// Return the current UNIX timestamp in seconds with sub-second precision.
///
/// Used internally to timestamp samples as they are decoded.
pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

// ─────────────────────────────────────────────────────────────────────────────

/// Owned handle to a running data stream returned by [`crate::Board::start_stream`].
///
/// Internally this wraps an `mpsc` channel fed by a background reader thread.
/// The stream **stops automatically** when the handle is dropped (via the `Drop`
/// impl), so you never need to call anything to clean up.
///
/// # Usage patterns
///
/// **Blocking iterator** — simplest, processes every sample in order:
/// ```rust,no_run
/// # use openbci::sample::{Sample, StreamHandle};
/// # fn get_stream() -> StreamHandle { unimplemented!() }
/// let stream: StreamHandle = get_stream();
/// for sample in stream.into_iter().take(250) {
///     println!("{:?}", sample.eeg);
/// }
/// ```
///
/// **Non-blocking polling** — useful when you need to do other work between samples:
/// ```rust,no_run
/// # use openbci::sample::{Sample, StreamHandle};
/// # fn get_stream() -> StreamHandle { unimplemented!() }
/// let stream: StreamHandle = get_stream();
/// loop {
///     if let Some(s) = stream.try_recv() {
///         println!("{:?}", s.eeg);
///     }
///     // ... other work ...
/// }
/// ```
///
/// **Explicit stop** (before drop):
/// ```rust,no_run
/// # use openbci::sample::{Sample, StreamHandle};
/// # fn get_stream() -> StreamHandle { unimplemented!() }
/// let stream: StreamHandle = get_stream();
/// std::thread::sleep(std::time::Duration::from_secs(5));
/// stream.stop(); // sends the stop signal immediately; `drop` is a no-op after this
/// ```
pub struct StreamHandle {
    pub(crate) receiver: std::sync::mpsc::Receiver<Sample>,
    pub(crate) stop_tx:  Option<std::sync::mpsc::SyncSender<()>>,
}

impl StreamHandle {
    /// Try to receive the next [`Sample`] without blocking.
    ///
    /// Returns `None` if no sample is available yet or the stream has ended.
    pub fn try_recv(&self) -> Option<Sample> {
        self.receiver.try_recv().ok()
    }

    /// Block until the next [`Sample`] arrives.
    ///
    /// Returns `None` if the stream has ended (background thread exited).
    pub fn recv(&self) -> Option<Sample> {
        self.receiver.recv().ok()
    }

    /// Access the raw `Receiver` for use in `select!` macros or custom polling.
    pub fn receiver(&self) -> &std::sync::mpsc::Receiver<Sample> {
        &self.receiver
    }

    /// Explicitly send the stop signal to the background reader thread.
    ///
    /// Consumes `self`; any subsequent `drop` is a no-op.  The board method
    /// [`stop_stream`](crate::Board::stop_stream) also sends `"s"` to the
    /// board hardware; calling this method does **not** do that — it only
    /// asks the reader thread to exit.
    pub fn stop(mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for StreamHandle {
    /// Sends the stop signal when the handle goes out of scope.
    fn drop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// Consuming iterator over a [`StreamHandle`].
///
/// Created by `stream_handle.into_iter()`.  Blocks on each call to `next()`
/// until a sample arrives or the stream ends.
impl IntoIterator for StreamHandle {
    type Item = Sample;
    type IntoIter = StreamIter;

    fn into_iter(self) -> Self::IntoIter {
        StreamIter(self)
    }
}

/// Iterator produced by [`StreamHandle::into_iter`].
pub struct StreamIter(StreamHandle);

impl Iterator for StreamIter {
    type Item = Sample;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.receiver.recv().ok()
    }
}
