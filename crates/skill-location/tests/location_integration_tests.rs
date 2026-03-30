// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Integration tests for skill-location.
//!
//! Tests marked `#[ignore]` require network access or macOS location services
//! and are not run in CI by default.  Run them with:
//!
//! ```sh
//! cargo test -p skill-location -- --ignored
//! ```

use skill_location::*;

// ── fetch_location (network-dependent) ───────────────────────────────────────

#[test]
#[ignore = "requires network access"]
fn fetch_location_returns_valid_fix() {
    let fix = fetch_location(10.0).expect("fetch_location should succeed");
    assert!(
        (-90.0..=90.0).contains(&fix.latitude),
        "latitude out of range: {}",
        fix.latitude
    );
    assert!(
        (-180.0..=180.0).contains(&fix.longitude),
        "longitude out of range: {}",
        fix.longitude
    );
    assert!(fix.timestamp > 0.0, "timestamp should be positive");
}

#[test]
#[ignore = "requires network access"]
fn fetch_location_timestamp_is_recent() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    let fix = fetch_location(10.0).expect("fetch_location should succeed");
    // Timestamp should be within the last 60 seconds
    assert!(
        (now - fix.timestamp).abs() < 60.0,
        "timestamp {:.0} too far from now {:.0}",
        fix.timestamp,
        now
    );
}

// ── fetch_ip_location (network-dependent) ────────────────────────────────────

#[test]
#[ignore = "requires network access"]
fn fetch_ip_location_returns_valid_fix() {
    let fix = fetch_ip_location().expect("IP geolocation should succeed");
    assert!((-90.0..=90.0).contains(&fix.latitude));
    assert!((-180.0..=180.0).contains(&fix.longitude));
    assert_eq!(fix.source, LocationSource::IpGeolocation);
}

#[test]
#[ignore = "requires network access"]
fn fetch_ip_location_has_geo_metadata() {
    let fix = fetch_ip_location().expect("IP geolocation should succeed");
    assert!(fix.country.is_some(), "country should be present");
    assert!(fix.timezone.is_some(), "timezone should be present");
}

#[test]
#[ignore = "requires network access"]
fn fetch_ip_location_has_no_gps_fields() {
    let fix = fetch_ip_location().expect("IP geolocation should succeed");
    assert!(fix.altitude.is_none());
    assert!(fix.horizontal_accuracy.is_none());
    assert!(fix.vertical_accuracy.is_none());
    assert!(fix.speed.is_none());
    assert!(fix.course.is_none());
}

#[test]
#[ignore = "requires network access"]
fn fetch_ip_location_country_is_nonempty() {
    let fix = fetch_ip_location().expect("IP geolocation should succeed");
    if let Some(c) = &fix.country {
        assert!(!c.is_empty(), "country should not be empty string");
    }
}

#[test]
#[ignore = "requires network access"]
fn fetch_ip_location_timezone_is_iana() {
    let fix = fetch_ip_location().expect("IP geolocation should succeed");
    if let Some(tz) = &fix.timezone {
        // IANA timezone IDs contain a slash (e.g. "America/New_York")
        assert!(tz.contains('/'), "timezone should be IANA format, got: {tz}");
    }
}

#[test]
#[ignore = "requires network access"]
fn fetch_ip_location_serde_round_trip() {
    let fix = fetch_ip_location().expect("IP geolocation should succeed");
    let json = serde_json::to_string(&fix).expect("serialize");
    let back: LocationFix = serde_json::from_str(&json).expect("deserialize");
    assert!((back.latitude - fix.latitude).abs() < 1e-10);
    assert!((back.longitude - fix.longitude).abs() < 1e-10);
    assert_eq!(back.source, fix.source);
    assert_eq!(back.country, fix.country);
    assert_eq!(back.timezone, fix.timezone);
}

// ── macOS-specific CoreLocation tests ────────────────────────────────────────

#[cfg(target_os = "macos")]
mod macos_tests {
    use super::*;

    #[test]
    #[ignore = "requires macOS main-thread runloop"]
    fn auth_status_returns_known_variant() {
        let status = auth_status();
        matches!(
            status,
            LocationAuthStatus::NotDetermined
                | LocationAuthStatus::Restricted
                | LocationAuthStatus::Denied
                | LocationAuthStatus::Authorized
        );
    }

    #[test]
    #[ignore = "requires macOS location permission + main-thread runloop"]
    fn fetch_location_corelocation_source() {
        let status = auth_status();
        if status != LocationAuthStatus::Authorized {
            eprintln!("skipping: location not authorized (status={status:?})");
            return;
        }

        let fix = fetch_location(10.0).expect("fetch should succeed when authorized");
        assert_eq!(fix.source, LocationSource::CoreLocation);
        assert!(
            fix.horizontal_accuracy.is_some(),
            "CoreLocation should provide accuracy"
        );
    }

    #[test]
    #[ignore = "requires macOS location permission + main-thread runloop"]
    fn fetch_location_corelocation_accuracy_positive() {
        let status = auth_status();
        if status != LocationAuthStatus::Authorized {
            return;
        }

        let fix = fetch_location(10.0).expect("fetch should succeed");
        if let Some(h) = fix.horizontal_accuracy {
            assert!(h > 0.0, "horizontal accuracy should be positive, got {h}");
        }
    }

    #[test]
    #[ignore = "requires macOS location permission + main-thread runloop"]
    fn fetch_location_corelocation_has_altitude() {
        let status = auth_status();
        if status != LocationAuthStatus::Authorized {
            return;
        }

        let fix = fetch_location(10.0).expect("fetch should succeed");
        assert!(fix.altitude.is_some(), "CoreLocation should provide altitude");
    }
}

// ── Non-macOS tests ──────────────────────────────────────────────────────────

#[cfg(not(target_os = "macos"))]
mod non_macos_tests {
    use super::*;

    #[test]
    fn auth_status_always_authorized() {
        assert_eq!(auth_status(), LocationAuthStatus::Authorized);
    }

    #[test]
    fn request_access_always_true() {
        assert!(request_access(0.0));
    }

    #[test]
    #[ignore = "requires network access"]
    fn fetch_location_uses_ip_source() {
        let fix = fetch_location(10.0).expect("fetch should succeed");
        assert_eq!(fix.source, LocationSource::IpGeolocation);
        assert!(fix.country.is_some());
    }
}

// ── Health store compatibility ───────────────────────────────────────────────

#[test]
fn fix_fields_compatible_with_health_location_sample() {
    let fix = LocationFix {
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
    };

    let source_id: String = format!("{:?}", fix.source);
    let timestamp: i64 = fix.timestamp as i64;
    let latitude: f64 = fix.latitude;
    let longitude: f64 = fix.longitude;
    let altitude: Option<f64> = fix.altitude;
    let horizontal_accuracy: Option<f64> = fix.horizontal_accuracy;
    let vertical_accuracy: Option<f64> = fix.vertical_accuracy;
    let speed: Option<f64> = fix.speed;
    let course: Option<f64> = fix.course;

    assert_eq!(source_id, "CoreLocation");
    assert_eq!(timestamp, 1700000000);
    assert!((latitude - 37.3317).abs() < 1e-6);
    assert!((longitude - -122.0307).abs() < 1e-6);
    assert_eq!(altitude, Some(30.5));
    assert_eq!(horizontal_accuracy, Some(10.0));
    assert_eq!(vertical_accuracy, Some(4.0));
    assert_eq!(speed, Some(1.2));
    assert_eq!(course, Some(90.0));
}

#[test]
fn ip_fix_fields_compatible_with_health_location_sample() {
    let fix = LocationFix {
        latitude: 48.8566,
        longitude: 2.3522,
        altitude: None,
        horizontal_accuracy: None,
        vertical_accuracy: None,
        speed: None,
        course: None,
        timestamp: 1700000000.0,
        country: Some("France".into()),
        region: None,
        city: Some("Paris".into()),
        timezone: Some("Europe/Paris".into()),
        source: LocationSource::IpGeolocation,
    };

    let source_id: String = format!("{:?}", fix.source);
    let timestamp: i64 = fix.timestamp as i64;

    assert_eq!(source_id, "IpGeolocation");
    assert_eq!(timestamp, 1700000000);
    assert!(fix.altitude.is_none());
    assert!(fix.horizontal_accuracy.is_none());
}
