### Features

- **EmotiBit support**: connect an EmotiBit board over Bluetooth (`emotibit-ble` firmware) or WiFi (stock firmware). Heart rate, inter-beat intervals, SpO2 and skin temperature record as device vitals, accelerometer and gyroscope as motion, and battery as battery. A BLE board is found by its GATT service; add a WiFi board as `emotibit:<id or ip>`.
- **Electrodermal activity and other scalar sensor streams are recorded**: a new `_sensors` sidecar (CSV or Parquet) stores one `timestamp_s, channel, unit, value` row per sample, so EDA, the SCR metrics, thermopile, humidity, magnetometer and raw PPG share one file regardless of rate. Subscribers also receive a live `SensorSample` event. Other devices with a scalar stream and no dedicated event can use the same `DeviceEvent::Sensor`.
- **EDA analysis**: skin conductance is analysed as it is recorded. Every five seconds you get the tonic level (SCL), skin conductance responses per minute and their size, and arousal as a z-score against the session's own baseline. Artifacts (lost contact, impossible jumps) are rejected rather than counted. The results are stored beside the raw samples as `eda_*` channels.
- **Live EDA card**: the dashboard shows the skin-conductance trace with the derived metrics, for an EmotiBit or any device that reports an `eda` channel. It says when the baseline is still being learned and warns about poor skin contact.

### Bugfixes

- **Sidecar files were miscounted and left behind**: `_vitals` and `_fnirs` files were treated as sessions by history listing, loaded as EEG by the EEG-file filter, and not removed when a session was deleted. One shared list of sidecar suffixes now drives all three.

### Build

- **`npm run release -- --rc --skip-checks`** (and `npm run bump -- --skip-checks`): cut a release candidate without any local testing or compilation. `bump` skips its preflight (frontend checks, the i18n check, vitest, clippy and cargo test), and `npm run tag` pushes the tag with `--no-verify` so the pre-push hook does not re-run them either. The version and tag safety checks and the pre-commit hook still run, and CI is the first to see the commit.
- **Heavy integration tests are opt-in**: nine model-, GPU- and measurement-dependent test targets (`fft_*`, `qwen3_tts_e2e`, `tts_validate_e2e`, `asr_e2e`, `asr_funasr_nemotron_smoke`, `rlx_image_embed`, `rlx_semantic`) sit behind a per-crate `slow-tests` feature, so `cargo test --workspace` no longer links them. Most were `#[ignore]`d or skip themselves without cached models. `cargo test-slow` runs everything; the documented run commands and the two TTS scripts pass the feature.
- **Smaller dev builds**: dependency crates build with no debug info and DuckDB at `opt-level = 1`, `split-debuginfo = "off"` stops kache writing `.dSYM` bundles, the `skill` library is built as an `rlib` only, benchmark binaries sit behind a `bench-tools` feature, and seven crates link their fast integration tests into one `it` binary each. A full workspace test build is about 6 GB instead of over 49 GB.
- **Linux release and CI builds no longer fail on a stale apt index**: the runner image's package index can be days old and the mirror drops superseded versions, so `cache-apt-pkgs-action` asked for files that were gone, the whole install failed while the step still reported success, and the build died later on `glib-2.0 was not found` (v0.1.1, v0.1.1-rc.3 and v0.1.2-rc.1) or `libsoup-3.0 not found` (PR CI). The apt index is now refreshed first, with retries, and a check right after the install fails immediately with a clear message if glib, GTK, libsoup or WebKit is missing.

### i18n

- **EDA card strings** in all nine languages.
