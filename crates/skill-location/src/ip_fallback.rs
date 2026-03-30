// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! IP-geolocation fallback used on Linux and Windows (and as a macOS fallback
//! when CoreLocation is unavailable/denied).

use crate::types::{LocationError, LocationFix, LocationSource};
use serde_json::Value;

/// Fetch an approximate location from the `ipwho.is` IP geolocation service.
///
/// This mirrors the existing `exec_location` logic in `skill-tools` but returns
/// a structured [`LocationFix`] instead of raw JSON.
pub fn fetch_ip_location() -> Result<LocationFix, LocationError> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(3)))
        .timeout_recv_body(Some(std::time::Duration::from_secs(5)))
        .build()
        .into();

    let resp = agent
        .get("https://ipwho.is/")
        .call()
        .map_err(|e| LocationError::Network(e.to_string()))?;

    let v: Value = resp
        .into_body()
        .read_json()
        .map_err(|e| LocationError::Failed(format!("invalid JSON: {e}")))?;

    let ok = v.get("success").and_then(Value::as_bool).unwrap_or(true);
    if !ok {
        let msg = v
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        return Err(LocationError::Failed(msg.to_string()));
    }

    let lat = v
        .get("latitude")
        .and_then(Value::as_f64)
        .ok_or_else(|| LocationError::Failed("missing latitude".into()))?;
    let lon = v
        .get("longitude")
        .and_then(Value::as_f64)
        .ok_or_else(|| LocationError::Failed("missing longitude".into()))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    Ok(LocationFix {
        latitude: lat,
        longitude: lon,
        altitude: None,
        horizontal_accuracy: None,
        vertical_accuracy: None,
        speed: None,
        course: None,
        timestamp: now,
        country: v.get("country").and_then(Value::as_str).map(String::from),
        region: v.get("region").and_then(Value::as_str).map(String::from),
        city: v.get("city").and_then(Value::as_str).map(String::from),
        timezone: v
            .get("timezone")
            .and_then(|z| z.get("id"))
            .and_then(Value::as_str)
            .map(String::from),
        source: LocationSource::IpGeolocation,
    })
}
