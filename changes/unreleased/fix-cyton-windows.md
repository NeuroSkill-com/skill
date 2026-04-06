### Bugfixes

- **Windows COM port ≥ 10**: added `\\.\COMxx` prefix normalization for COM10+ ports that otherwise fail with "file not found".
- **FTDI dongle auto-detection**: 3-pass serial port detection (VID/PID → product/manufacturer string → heuristic fallback) catches dongles that Windows reports without USB metadata.
- **Hot-plug retry**: 3 attempts with 1.5s/3s backoff delays when the serial port is temporarily locked after USB replug.
- **Device ID → session start**: `start_session` now accepts `usb:COM3`, `cgx:*`, `neurofield:*`, `brainbit:*`, `gtec:*`, `lsl:*` device IDs and routes to the correct connect function.
- **LLM inference disabled by default**: daemon `Cargo.toml` had `default = []` — all `#[cfg(feature = "llm")]` code was dead. Fixed: `default = ["llm", "embed-exg"]`.
- **LSL sessions broken**: `lsl:` targets were not handled in `connect_device`. Now wired through `connect_lsl()` with stream resolution and `LslAdapter`.
