# skill-location

Cross-platform location provider for NeuroSkill.

| Platform | Backend | Accuracy |
|----------|---------|----------|
| macOS | Apple CoreLocation (GPS / Wi-Fi / cell) | ~10–100 m |
| Linux | IP geolocation (`ipwho.is`) | ~1–25 km (city level) |
| Windows | IP geolocation (`ipwho.is`) | ~1–25 km (city level) |

## Features

- **CoreLocation on macOS** — uses the system's location hardware for precise
  fixes including altitude, speed, course, and horizontal/vertical accuracy.
- **Automatic fallback** — if CoreLocation is denied or fails, transparently
  falls back to IP geolocation.
- **Uniform API** — `fetch_location()` returns the same `LocationFix` struct
  regardless of platform.
- **Permission helpers** — `auth_status()` and `request_access()` map to
  CoreLocation authorization on macOS; always-authorized on other platforms.
- **Health store integration** — `LocationFix` maps directly to
  `skill_health::LocationSample` for storage alongside other health data.

## Usage

```rust
use skill_location::{auth_status, fetch_location, LocationAuthStatus};

if auth_status() == LocationAuthStatus::NotDetermined {
    skill_location::request_access(30.0);
}

match fetch_location(10.0) {
    Ok(fix) => println!("{:.5}, {:.5}", fix.latitude, fix.longitude),
    Err(e) => eprintln!("location error: {e}"),
}
```

## macOS Entitlements

The app's `Info.plist` must include `NSLocationWhenInUseUsageDescription` and
the binary must have the `com.apple.security.personal-information.location`
entitlement for CoreLocation to work.
