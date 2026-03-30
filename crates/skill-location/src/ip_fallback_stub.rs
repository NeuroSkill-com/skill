// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! IP-geolocation fallback for macOS.
//!
//! On macOS we don't pull in `ureq` as a dependency — instead we shell out to
//! `curl` which is always available on macOS.  This avoids doubling the TLS
//! dependency surface on the platform where CoreLocation is the primary path.

use crate::types::{LocationError, LocationFix, LocationSource};
use serde_json::Value;

pub fn fetch_ip_location() -> Result<LocationFix, LocationError> {
    let output = std::process::Command::new("curl")
        .args(["-sS", "--max-time", "5", "https://ipwho.is/"])
        .output()
        .map_err(|e| LocationError::Network(format!("curl failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(LocationError::Network(format!("curl error: {stderr}")));
    }

    let body = String::from_utf8_lossy(&output.stdout);
    let v: Value = serde_json::from_str(&body)
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
