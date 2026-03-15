# Changelog

## 2026-03-15

### Refactor: rename apple-ocr → skill-vision

- Renamed `crates/apple-ocr/` to `crates/skill-vision/` for naming consistency with the rest of the workspace (`skill-*` convention)
- Updated `Cargo.toml` package name from `apple-ocr` to `skill-vision`
- Updated `src-tauri/Cargo.toml` dependency path
- Updated `skill-screenshots/src/capture.rs` to reference `skill_vision::` instead of `apple_ocr::`
- Added `skill-vision` as a macOS-only dependency in `skill-screenshots/Cargo.toml` (was previously missing — the `#[cfg(target_os = "macos")]` gate masked the missing dep on non-macOS builds)
- All 2 unit tests pass; full workspace builds cleanly

### Refactor: extract activity_store, label_index, autostart, session_csv into workspace crates

- **`skill-data` crate extended** with three new modules:
  - `active_window` — `ActiveWindowInfo` data type (shared across workspace)
  - `activity_store` — `ActivityStore` SQLite persistence (active windows, input activity, per-minute input buckets); includes 8 unit tests
  - `session_csv` — `CsvState` multiplexed CSV writer for EEG/PPG/metrics recording, path utilities (`ppg_csv_path`, `metrics_csv_path`), sample-rate constants, `METRICS_CSV_HEADER`
  - Added `skill-eeg` and `csv` as dependencies

- **New `skill-label-index` crate** (`crates/skill-label-index/`) — cross-modal label HNSW indices (text, context, EEG):
  - `LabelIndexState`, `rebuild`, `insert_label`, `search_by_text_vec`, `search_by_context_vec`, `search_by_eeg_vec`, `mean_eeg_for_window`
  - `LabelNeighbor`, `RebuildStats` types
  - 532 lines extracted; depends on `skill-commands`, `skill-data`, `skill-constants`, `fast-hnsw`, `rusqlite`

- **New `skill-autostart` crate** (`crates/skill-autostart/`) — platform-specific launch-at-login:
  - macOS: LaunchAgent plist in `~/Library/LaunchAgents/`
  - Linux: XDG `.desktop` file in `~/.config/autostart/`
  - Windows: `HKCU\...\Run` registry key
  - 217 lines extracted; depends only on `skill-constants`

- **`src-tauri/src/` shims** — `activity_store.rs`, `label_index.rs`, `autostart.rs` replaced with re-export shims; `session_csv.rs` retains only Tauri-coupled functions (`new_csv_path`, `write_session_meta`) and re-exports the pure CSV writer from `skill-data`; `active_window.rs` re-exports `ActiveWindowInfo` from `skill-data` instead of defining it locally

- All existing `crate::*` import paths continue to work unchanged
