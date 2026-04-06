//! Error types for BrainMaster devices.

/// All errors that can occur when communicating with a BrainMaster device.
#[derive(Debug, thiserror::Error)]
pub enum BrainMasterError {
    #[error("serial port error: {0}")]
    Serial(#[from] serialport::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("no BrainMaster device found")]
    NoDevice,

    #[error("device not streaming")]
    NotStreaming,

    #[error("sync lost — no valid packet header within timeout")]
    SyncLost,

    #[error("checksum mismatch (expected {expected:#04x}, got {got:#04x})")]
    Checksum { expected: u8, got: u8 },

    #[error("timeout waiting for data")]
    Timeout,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, BrainMasterError>;
