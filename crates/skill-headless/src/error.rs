// SPDX-License-Identifier: GPL-3.0-only
//! Error types for the headless browser engine.

use std::fmt;

/// All errors that can arise from headless browser operations.
#[derive(Debug)]
pub enum HeadlessError {
    /// The event-loop thread has exited or the channel was closed.
    ChannelClosed,
    /// Timed out waiting for a response.
    Timeout,
    /// The webview returned a JS evaluation error.
    JsError(String),
    /// Navigation to an invalid or unreachable URL.
    NavigationFailed(String),
    /// wry / tao initialization failure.
    InitFailed(String),
    /// Screenshot capture failed.
    ScreenshotFailed(String),
    /// The browser session is already closed.
    SessionClosed,
    /// A generic wrapped error.
    Other(String),
}

impl fmt::Display for HeadlessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ChannelClosed => write!(f, "channel closed (event-loop exited)"),
            Self::Timeout => write!(f, "operation timed out"),
            Self::JsError(e) => write!(f, "JS error: {e}"),
            Self::NavigationFailed(u) => write!(f, "navigation failed: {u}"),
            Self::InitFailed(e) => write!(f, "init failed: {e}"),
            Self::ScreenshotFailed(e) => write!(f, "screenshot failed: {e}"),
            Self::SessionClosed => write!(f, "browser session is closed"),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for HeadlessError {}

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
