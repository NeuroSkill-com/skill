//! Error types returned by every board operation.

use thiserror::Error;

/// All errors that can originate from this crate.
///
/// Most public API functions return `Result<T>`, which is an alias for
/// `std::result::Result<T, OpenBciError>`.
#[derive(Debug, Error)]
pub enum OpenBciError {
    /// Wraps a [`serialport::Error`] — port not found, permission denied,
    /// baud-rate mismatch, etc.
    #[error("Serial port error: {0}")]
    SerialPort(#[from] serialport::Error),

    /// Wraps a standard [`std::io::Error`] from socket or file operations.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// The board did not complete its startup handshake (e.g. `$$$` marker
    /// was never received, or a `Failure` response was returned).
    #[error("Board not ready: {0}")]
    BoardNotReady(String),

    /// A board method was called before [`crate::Board::prepare`].
    #[error("Board not yet prepared — call prepare() first")]
    BoardNotPrepared,

    /// [`crate::Board::start_stream`] was called while already streaming.
    #[error("Already streaming")]
    AlreadyStreaming,

    /// [`crate::Board::stop_stream`] was called when not streaming.
    #[error("Not currently streaming")]
    NotStreaming,

    /// An HTTP or TCP error occurred communicating with a WiFi Shield board.
    #[error("WiFi / HTTP error: {0}")]
    Wifi(String),

    /// A Bluetooth LE error from `webbluetooth` (only present with the `ble`
    /// feature).
    ///
    /// Covers the Web Bluetooth error names as well as transport failures —
    /// notably `SecurityError`, which is what a service missing from
    /// [`crate::board::ganglion::GanglionConfig::services`] produces.
    #[cfg(feature = "ble")]
    #[error("BLE error: {0}")]
    Ble(#[from] webbluetooth::Error),

    /// The supplied configuration is logically invalid (e.g. gain code out
    /// of range, missing required field).
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// A blocking operation (board ready wait, BLE scan) exceeded its timeout.
    #[error("Operation timed out")]
    Timeout,

    /// A channel index was out of range for the board.
    /// Contains `(requested_index, board_channel_count)`.
    #[error("Channel index {0} out of range (board has {1} channels)")]
    ChannelOutOfRange(usize, usize),

    /// The internal `mpsc` channel between the reader thread and the caller
    /// was unexpectedly closed.
    #[error("Stream channel disconnected")]
    StreamDisconnected,

    /// Low-level packet framing or decoding failed.
    #[error("Packet parse error: {0}")]
    PacketParse(String),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, OpenBciError>;
