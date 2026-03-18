### Bugfixes

- **Rust `DeviceKind` enum missing Ganglion, MW75, Hermes**: Added `Ganglion`, `Mw75`, and `Hermes` variants with correct capabilities (channel count, sample rate, IMU flags). Previously Ganglion was lumped into `OpenBci` and MW75/Hermes had no representation.
- **`DeviceKind::from_name` missing prefixes**: Added `simblee`, `mn8`, and `guardian` prefix detection; split Ganglion from OpenBCI; added MW75 (substring) and Hermes detection.
- **Frontend `deviceCapabilities()` incomplete**: Added `GANGLION_CAPS` (4ch/200Hz), `MW75_CAPS` (12ch/500Hz), and `HERMES_CAPS` (8ch/250Hz) with correct electrode names. Ganglion was previously detected as OpenBCI (8ch/250Hz).
- **Frontend Ganglion detection wrong**: `"simblee"` prefix now correctly returns Ganglion caps instead of falling through to unknown.
