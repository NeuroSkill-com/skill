// btleplug's API, implemented over webbluetooth.
//
// The `api` and `common` modules are verbatim copies of upstream btleplug
// 0.11.8 — they are platform-independent (types, traits, a peripheral registry
// and a broadcast helper), and copying rather than rewriting them is what
// guarantees the five consuming crates see identical types.  Content-identical,
// not byte-identical: upstream ships CRLF and this repo normalises `*.rs` to
// LF, so compare with `diff --strip-trailing-cr`.
//
// `platform` is the part that is ours: upstream has one backend per OS
// (corebluetooth/, winrtble/, bluez/, droidplug/), and this has one backend for
// all of them, because webbluetooth already abstracts the platforms.  See
// Cargo.toml for why.
//
// Upstream copyright, for the vendored modules:
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
// Licensed under the BSD 3-Clause license. See LICENSE.md.

use crate::api::ParseBDAddrError;
use std::result;
use std::time::Duration;

pub mod api;
// Verbatim upstream, byte-for-byte: the `allow` lives here rather than in the
// module so a re-sync stays a straight copy.  Upstream's elided-lifetime style
// trips a newer lint; that is not a reason to diverge from it.
#[allow(mismatched_lifetime_syntaxes)]
mod common;
pub mod platform;

/// The main error type returned by most methods in btleplug.
///
/// Verbatim from upstream: the consuming crates match on these variants.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Permission denied")]
    PermissionDenied,

    #[error("Device not found")]
    DeviceNotFound,

    #[error("Not connected")]
    NotConnected,

    #[error("Unexpected callback")]
    UnexpectedCallback,

    #[error("Unexpected characteristic")]
    UnexpectedCharacteristic,

    #[error("No such characteristic")]
    NoSuchCharacteristic,

    #[error("The operation is not supported: {}", _0)]
    NotSupported(String),

    #[error("Timed out after {:?}", _0)]
    TimedOut(Duration),

    #[error("Error parsing UUID: {0}")]
    Uuid(#[from] uuid::Error),

    #[error("Invalid Bluetooth address: {0}")]
    InvalidBDAddr(#[from] ParseBDAddrError),

    #[error("Runtime Error: {}", _0)]
    RuntimeError(String),

    #[error("{}", _0)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

/// Convert [`PoisonError`] to [`Error`] for replace `unwrap` to `map_err`
impl<T: std::fmt::Debug> From<std::sync::PoisonError<T>> for Error {
    fn from(e: std::sync::PoisonError<T>) -> Self {
        Self::Other(format!("{:?}", e).into())
    }
}

/// webbluetooth's errors, mapped onto btleplug's.
///
/// Only the variants a caller can usefully distinguish are mapped; the rest
/// become `RuntimeError`, which is what upstream's backends do with
/// platform-specific failures too.
impl From<webbluetooth::Error> for Error {
    fn from(e: webbluetooth::Error) -> Self {
        use webbluetooth::Error as W;
        match e {
            W::NotFound(_) => Error::DeviceNotFound,
            // "the device disconnected, or handles into the old service set
            // were invalidated" — btleplug expresses both as NotConnected.
            W::InvalidState(_) => Error::NotConnected,
            W::Security(_) => Error::PermissionDenied,
            // Bluetooth off, or this process not allowed to use it.  Upstream's
            // CoreBluetooth backend returns PermissionDenied for exactly this.
            W::NotAvailable(_) => Error::PermissionDenied,
            W::NotSupported(m) => Error::NotSupported(m),
            // btleplug's TimedOut carries the deadline, which webbluetooth does
            // not report, so keep the message rather than invent a Duration.
            W::Timeout(m) => Error::RuntimeError(format!("timed out: {m}")),
            other => Error::RuntimeError(other.to_string()),
        }
    }
}

/// Convenience type for a result using the btleplug [`Error`] type.
pub type Result<T> = result::Result<T, Error>;
