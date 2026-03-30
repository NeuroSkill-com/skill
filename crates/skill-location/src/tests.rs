// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Unit tests for skill-location types and logic.

use crate::types::*;

// ── LocationAuthStatus ───────────────────────────────────────────────────────

#[test]
fn auth_status_variants_are_distinct() {
    let statuses = [
        LocationAuthStatus::NotDetermined,
        LocationAuthStatus::Restricted,
        LocationAuthStatus::Denied,
        LocationAuthStatus::Authorized,
    ];
    for (i, a) in statuses.iter().enumerate() {
        for (j, b) in statuses.iter().enumerate() {
            if i == j {
                assert_eq!(a, b);
            } else {
                assert_ne!(a, b);
            }
        }
    }
}

#[test]
fn auth_status_clone_eq() {
    let s = LocationAuthStatus::Authorized;
    let s2 = s;
    assert_eq!(s, s2);
}

#[test]
fn auth_status_debug_not_empty() {
    let s = LocationAuthStatus::NotDetermined;
    let dbg = format!("{s:?}");
    assert!(!dbg.is_empty());
    assert!(dbg.contains("NotDetermined"));
}

// ── LocationSource ───────────────────────────────────────────────────────────

#[test]
fn location_source_variants_are_distinct() {
    assert_ne!(LocationSource::CoreLocation, LocationSource::IpGeolocation);
}

#[test]
fn location_source_debug() {
    assert!(format!("{:?}", LocationSource::CoreLocation).contains("CoreLocation"));
    assert!(format!("{:?}", LocationSource::IpGeolocation).contains("IpGeolocation"));
}

#[test]
fn location_source_clone_eq() {
    let s = LocationSource::CoreLocation;
    let s2 = s;
    assert_eq!(s, s2);
}

// ── LocationFix construction & field access ──────────────────────────────────

fn sample_corelocation_fix() -> LocationFix {
    LocationFix {
        latitude: 37.3317,
        longitude: -122.0307,
        altitude: Some(30.5),
        horizontal_accuracy: Some(10.0),
        vertical_accuracy: Some(4.0),
        speed: Some(1.2),
        course: Some(90.0),
        timestamp: 1700000000.123,
        country: None,
        region: None,
        city: None,
        timezone: None,
        source: LocationSource::CoreLocation,
    }
}

fn sample_ip_fix() -> LocationFix {
    LocationFix {
        latitude: 48.8566,
        longitude: 2.3522,
        altitude: None,
        horizontal_accuracy: None,
        vertical_accuracy: None,
        speed: None,
        course: None,
        timestamp: 1700000000.0,
        country: Some("France".into()),
        region: Some("Île-de-France".into()),
        city: Some("Paris".into()),
        timezone: Some("Europe/Paris".into()),
        source: LocationSource::IpGeolocation,
    }
}

#[test]
fn corelocation_fix_has_accuracy_fields() {
    let fix = sample_corelocation_fix();
    assert!(fix.horizontal_accuracy.is_some());
    assert!(fix.vertical_accuracy.is_some());
    assert!(fix.altitude.is_some());
    assert!(fix.speed.is_some());
    assert!(fix.course.is_some());
    assert_eq!(fix.source, LocationSource::CoreLocation);
}

#[test]
fn ip_fix_has_geo_metadata() {
    let fix = sample_ip_fix();
    assert_eq!(fix.country.as_deref(), Some("France"));
    assert_eq!(fix.region.as_deref(), Some("Île-de-France"));
    assert_eq!(fix.city.as_deref(), Some("Paris"));
    assert_eq!(fix.timezone.as_deref(), Some("Europe/Paris"));
    assert_eq!(fix.source, LocationSource::IpGeolocation);
}

#[test]
fn ip_fix_has_no_accuracy_fields() {
    let fix = sample_ip_fix();
    assert!(fix.horizontal_accuracy.is_none());
    assert!(fix.vertical_accuracy.is_none());
    assert!(fix.altitude.is_none());
    assert!(fix.speed.is_none());
    assert!(fix.course.is_none());
}

#[test]
fn corelocation_fix_has_no_geo_metadata() {
    let fix = sample_corelocation_fix();
    assert!(fix.country.is_none());
    assert!(fix.region.is_none());
    assert!(fix.city.is_none());
    assert!(fix.timezone.is_none());
}

#[test]
fn fix_latitude_range() {
    let fix = sample_corelocation_fix();
    assert!((-90.0..=90.0).contains(&fix.latitude));
}

#[test]
fn fix_longitude_range() {
    let fix = sample_corelocation_fix();
    assert!((-180.0..=180.0).contains(&fix.longitude));
}

#[test]
fn fix_timestamp_positive() {
    let fix = sample_corelocation_fix();
    assert!(fix.timestamp > 0.0);
}

// ── LocationFix serde round-trip ─────────────────────────────────────────────

#[test]
fn fix_serde_round_trip_corelocation() {
    let fix = sample_corelocation_fix();
    let json = serde_json::to_string(&fix).expect("serialize");
    let back: LocationFix = serde_json::from_str(&json).expect("deserialize");
    assert!((back.latitude - fix.latitude).abs() < 1e-10);
    assert!((back.longitude - fix.longitude).abs() < 1e-10);
    assert_eq!(back.altitude, fix.altitude);
    assert_eq!(back.horizontal_accuracy, fix.horizontal_accuracy);
    assert_eq!(back.vertical_accuracy, fix.vertical_accuracy);
    assert_eq!(back.speed, fix.speed);
    assert_eq!(back.course, fix.course);
    assert!((back.timestamp - fix.timestamp).abs() < 1e-6);
    assert_eq!(back.source, LocationSource::CoreLocation);
    assert_eq!(back.country, None);
}

#[test]
fn fix_serde_round_trip_ip() {
    let fix = sample_ip_fix();
    let json = serde_json::to_string(&fix).expect("serialize");
    let back: LocationFix = serde_json::from_str(&json).expect("deserialize");
    assert!((back.latitude - fix.latitude).abs() < 1e-10);
    assert!((back.longitude - fix.longitude).abs() < 1e-10);
    assert_eq!(back.country.as_deref(), Some("France"));
    assert_eq!(back.city.as_deref(), Some("Paris"));
    assert_eq!(back.timezone.as_deref(), Some("Europe/Paris"));
    assert_eq!(back.source, LocationSource::IpGeolocation);
}

#[test]
fn fix_deserialize_from_json_literal() {
    let json = r#"{
        "latitude": 51.5074,
        "longitude": -0.1278,
        "altitude": null,
        "horizontal_accuracy": null,
        "vertical_accuracy": null,
        "speed": null,
        "course": null,
        "timestamp": 1700000000.0,
        "country": "United Kingdom",
        "region": "England",
        "city": "London",
        "timezone": "Europe/London",
        "source": "IpGeolocation"
    }"#;
    let fix: LocationFix = serde_json::from_str(json).expect("deserialize");
    assert!((fix.latitude - 51.5074).abs() < 1e-6);
    assert!((fix.longitude - -0.1278).abs() < 1e-6);
    assert_eq!(fix.city.as_deref(), Some("London"));
    assert_eq!(fix.source, LocationSource::IpGeolocation);
}

#[test]
fn fix_serialize_optional_fields_present() {
    let fix = sample_corelocation_fix();
    let v: serde_json::Value = serde_json::to_value(&fix).expect("to_value");
    assert!(v.get("altitude").is_some());
    assert!(v["altitude"].is_f64());
    assert!(v.get("horizontal_accuracy").is_some());
    assert!(v["horizontal_accuracy"].is_f64());
}

#[test]
fn fix_serialize_optional_fields_null() {
    let fix = sample_ip_fix();
    let v: serde_json::Value = serde_json::to_value(&fix).expect("to_value");
    assert!(v["altitude"].is_null());
    assert!(v["horizontal_accuracy"].is_null());
    assert!(v["speed"].is_null());
}

// ── LocationAuthStatus serde ─────────────────────────────────────────────────

#[test]
fn auth_status_serde_round_trip() {
    for status in &[
        LocationAuthStatus::NotDetermined,
        LocationAuthStatus::Restricted,
        LocationAuthStatus::Denied,
        LocationAuthStatus::Authorized,
    ] {
        let json = serde_json::to_string(status).expect("serialize");
        let back: LocationAuthStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*status, back);
    }
}

#[test]
fn auth_status_deserialize_from_string() {
    let s: LocationAuthStatus = serde_json::from_str("\"Authorized\"").expect("deser");
    assert_eq!(s, LocationAuthStatus::Authorized);
}

// ── LocationSource serde ─────────────────────────────────────────────────────

#[test]
fn source_serde_round_trip() {
    for src in &[LocationSource::CoreLocation, LocationSource::IpGeolocation] {
        let json = serde_json::to_string(src).expect("serialize");
        let back: LocationSource = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*src, back);
    }
}

// ── LocationError ────────────────────────────────────────────────────────────

#[test]
fn error_display_not_authorized() {
    let e = LocationError::NotAuthorized("denied by user".into());
    let msg = e.to_string();
    assert!(msg.contains("not authorized"), "got: {msg}");
    assert!(msg.contains("denied by user"), "got: {msg}");
}

#[test]
fn error_display_timeout() {
    let e = LocationError::Timeout;
    assert!(e.to_string().contains("timed out"));
}

#[test]
fn error_display_failed() {
    let e = LocationError::Failed("something broke".into());
    let msg = e.to_string();
    assert!(msg.contains("something broke"));
}

#[test]
fn error_display_network() {
    let e = LocationError::Network("connection refused".into());
    let msg = e.to_string();
    assert!(msg.contains("connection refused"));
}

#[test]
fn error_is_send_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<LocationError>();
    assert_sync::<LocationError>();
}

// ── LocationFix Clone ────────────────────────────────────────────────────────

#[test]
fn fix_clone_is_independent() {
    let fix = sample_ip_fix();
    let mut cloned = fix.clone();
    cloned.latitude = 0.0;
    cloned.city = Some("Berlin".into());
    // Original unchanged
    assert!((fix.latitude - 48.8566).abs() < 1e-6);
    assert_eq!(fix.city.as_deref(), Some("Paris"));
}

// ── Edge cases ───────────────────────────────────────────────────────────────

#[test]
fn fix_with_zero_coordinates() {
    let fix = LocationFix {
        latitude: 0.0,
        longitude: 0.0,
        altitude: None,
        horizontal_accuracy: None,
        vertical_accuracy: None,
        speed: None,
        course: None,
        timestamp: 1.0,
        country: None,
        region: None,
        city: None,
        timezone: None,
        source: LocationSource::IpGeolocation,
    };
    assert!((fix.latitude).abs() < f64::EPSILON);
    assert!((fix.longitude).abs() < f64::EPSILON);
}

#[test]
fn fix_with_extreme_coordinates() {
    // North pole
    let fix = LocationFix {
        latitude: 90.0,
        longitude: 180.0,
        altitude: Some(-400.0), // Dead Sea
        horizontal_accuracy: Some(0.5),
        vertical_accuracy: Some(0.1),
        speed: Some(0.0),
        course: Some(359.9),
        timestamp: 1.0,
        country: None,
        region: None,
        city: None,
        timezone: None,
        source: LocationSource::CoreLocation,
    };
    assert!((fix.latitude - 90.0).abs() < f64::EPSILON);
    assert!((fix.longitude - 180.0).abs() < f64::EPSILON);
    // Negative altitude is valid
    assert_eq!(fix.altitude, Some(-400.0));
}

#[test]
fn fix_with_negative_coordinates() {
    // South America
    let fix = LocationFix {
        latitude: -33.4489,
        longitude: -70.6693,
        altitude: Some(520.0),
        horizontal_accuracy: None,
        vertical_accuracy: None,
        speed: None,
        course: None,
        timestamp: 1700000000.0,
        country: Some("Chile".into()),
        region: None,
        city: Some("Santiago".into()),
        timezone: Some("America/Santiago".into()),
        source: LocationSource::IpGeolocation,
    };
    assert!(fix.latitude < 0.0);
    assert!(fix.longitude < 0.0);
}

#[test]
fn fix_debug_format_includes_coords() {
    let fix = sample_corelocation_fix();
    let dbg = format!("{fix:?}");
    assert!(dbg.contains("37.3317"), "debug should include latitude");
    assert!(dbg.contains("-122.0307"), "debug should include longitude");
    assert!(dbg.contains("CoreLocation"), "debug should include source");
}

// ── Public API surface ───────────────────────────────────────────────────────
//
// CoreLocation calls `dispatch_sync(dispatch_get_main_queue(), …)` which will
// deadlock in a test runner that has no active NSRunLoop on the main thread.
// These are marked `#[ignore]` and only run with `cargo test -- --ignored`.

#[test]
#[ignore = "calls CoreLocation FFI which requires macOS main-thread runloop"]
fn auth_status_returns_a_valid_variant() {
    let status = crate::auth_status();
    let _ = format!("{status:?}");
}

#[test]
#[ignore = "calls CoreLocation FFI which requires macOS main-thread runloop"]
fn request_access_does_not_panic() {
    let granted = crate::request_access(0.1);
    #[cfg(not(target_os = "macos"))]
    assert!(granted);
    #[cfg(target_os = "macos")]
    let _ = granted;
}

// ── Non-macOS public API (safe to call in test runner) ───────────────────────

#[cfg(not(target_os = "macos"))]
#[test]
fn non_macos_auth_status_always_authorized() {
    assert_eq!(crate::auth_status(), LocationAuthStatus::Authorized);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn non_macos_request_access_always_true() {
    assert!(crate::request_access(0.0));
}
