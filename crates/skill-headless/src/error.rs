// SPDX-License-Identifier: GPL-3.0-only
//! Error types for the headless browser engine.

use thiserror::Error;

/// All errors that can arise from headless browser operations.
#[derive(Debug, Error)]
pub enum HeadlessError {
    /// The event-loop thread has exited or the channel was closed.
    #[error("channel closed (event-loop exited)")]
    ChannelClosed,
    /// Timed out waiting for a response.
    #[error("operation timed out")]
    Timeout,
    /// The webview returned a JS evaluation error.
    #[error("JS error: {0}")]
    JsError(String),
    /// Navigation to an invalid or unreachable URL.
    #[error("navigation failed: {0}")]
    NavigationFailed(String),
    /// wry / tao initialization failure.
    #[error("init failed: {0}")]
    InitFailed(String),
    /// Screenshot capture failed.
    #[error("screenshot failed: {0}")]
    ScreenshotFailed(String),
    /// The browser session is already closed.
    #[error("browser session is closed")]
    SessionClosed,
    /// A generic wrapped error.
    #[error("{0}")]
    Other(String),
}

impl From<crossbeam_channel::RecvTimeoutError> for HeadlessError {
    fn from(e: crossbeam_channel::RecvTimeoutError) -> Self {
        match e {
            crossbeam_channel::RecvTimeoutError::Timeout => Self::Timeout,
            crossbeam_channel::RecvTimeoutError::Disconnected => Self::ChannelClosed,
        }
    }
}

impl<T> From<crossbeam_channel::SendError<T>> for HeadlessError {
    fn from(_: crossbeam_channel::SendError<T>) -> Self {
        Self::ChannelClosed
    }
}
