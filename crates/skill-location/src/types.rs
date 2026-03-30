// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Shared types for the location crate.

use serde::{Deserialize, Serialize};

/// Authorization status for location services.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocationAuthStatus {
    /// The user has not yet been asked for permission.
    NotDetermined,
    /// Location services are restricted by parental controls / MDM.
    Restricted,
    /// The user explicitly denied permission.
    Denied,
    /// Location access has been granted.
    Authorized,
}

/// A single location fix.
///
/// On macOS this comes from CoreLocation and includes full GPS accuracy data.
/// On other platforms it is derived from IP geolocation (lower accuracy).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocationFix {
    /// WGS-84 latitude in degrees.
    pub latitude: f64,
    /// WGS-84 longitude in degrees.
    pub longitude: f64,
    /// Altitude in metres above sea level (`None` if unavailable).
    pub altitude: Option<f64>,
    /// Horizontal accuracy in metres (`None` if unavailable).
    pub horizontal_accuracy: Option<f64>,
    /// Vertical accuracy in metres (`None` if unavailable).
    pub vertical_accuracy: Option<f64>,
    /// Speed in m/s (`None` if unavailable).
    pub speed: Option<f64>,
    /// Course/heading in degrees (`None` if unavailable).
    pub course: Option<f64>,
    /// UTC unix timestamp (seconds, fractional).
    pub timestamp: f64,
    /// Human-readable country name (IP geolocation only).
    pub country: Option<String>,
    /// Region / state (IP geolocation only).
    pub region: Option<String>,
    /// City (IP geolocation only).
    pub city: Option<String>,
    /// IANA timezone ID (IP geolocation only).
    pub timezone: Option<String>,
    /// The source of this fix.
    pub source: LocationSource,
}

/// Where the location fix came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocationSource {
    /// macOS CoreLocation (GPS / Wi-Fi / cell triangulation).
    CoreLocation,
    /// IP-based geolocation API.
    IpGeolocation,
}

/// Errors returned by the location crate.
#[derive(Debug, thiserror::Error)]
pub enum LocationError {
    #[error("location not authorized: {0}")]
    NotAuthorized(String),
    #[error("location request timed out")]
    Timeout,
    #[error("location request failed: {0}")]
    Failed(String),
    #[error("network error: {0}")]
    Network(String),
}
