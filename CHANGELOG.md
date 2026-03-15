# Changelog

## 2026-03-15

### Refactor: deduplicate codebase (types, dashboard components, window helpers)

**Frontend — TypeScript/Svelte:**
- Consolidated 5 duplicate `UmapPoint`/`UmapResult`/`UmapProgress` interface definitions into shared exports in `src/lib/types.ts`; updated `UmapViewer3D.svelte`, `UmapScene.svelte`, `UmapCanvas.svelte`, `compare/+page.svelte`, `search/+page.svelte` to import from the shared module
- Extracted `CollapsibleSection.svelte` — reusable collapsible card header with chevron toggle, title, and live-blink dot; replaces ~12 lines of repeated markup in 7 dashboard cards (`ArtifactEvents`, `BrainStateScores`, `CompositeScores`, `ConsciousnessMetrics`, `EegIndices`, `HeadPoseCard`, `PpgMetrics`)
- Extracted `MetricBar.svelte` — reusable thin progress bar with configurable height, Tailwind bg class, and CSS gradient; replaces repeated `h-1 rounded-full bg-black/8 dark:bg-white/10` markup in dashboard metric cards
- Added `fmtDateShort(unix)` to `format.ts` for month+day formatting without year; replaced inline `toLocaleDateString` in `ChatSidebar.svelte`
- Replaced inline date formatting in `CalibrationTab.svelte` with existing `fmtDateTimeLocale()` from `format.ts`
- Added `thresholdColor(value, thresholds, fallback)` utility to `format.ts` for declarative metric color threshold logic
- Exported new dashboard components from `src/lib/dashboard/index.ts`

**Backend — Rust:**
- Created `WindowSpec` struct and `focus_or_create()` helper in `window_cmds.rs` — deduplicates the "check existing → unminimize/show/focus → or build new" pattern
- Created `focus_or_create_with_emit()` variant for windows that emit tab-switch events on focus
- Refactored 15 window-open functions to use the shared helpers: `open_settings_window`, `open_help_window`, `open_search_window`, `open_labels_window`, `open_label_window`, `open_api_window`, `open_whats_new_window`, `open_onboarding_window`, `open_model_tab`, `open_updates_window`, `open_history_window`, `open_downloads_window`, `open_chat_window`, `open_about_window`, `open_compare_window`
- Removed unused `Manager` imports from `history_cmds.rs` and `about.rs`

### UI: history view — rainbow label circles with hover interaction

- Replaced text-based label display in session summary rows, expanded session details, epoch dot timeline legend, and canvas `renderDayDots` with tiny colored circles
- Labels are colored with a rainbow hue distribution (0°–300° HSL) based on temporal order (`eeg_start`), so nearby labels share similar colors and distant ones are visually distinct
- Hovering a label circle highlights all exact-match labels (same `.text`) across sessions with a glowing ring + scale-up effect (`box-shadow` glow, `scale-[1.7]`)
- Temporally close labels (within 5 minutes) get a subtler brightness/glow effect on hover (`scale-[1.4]`, `brightness-130`)
- Each circle shows a popover tooltip on hover with the label text and timestamp
- All highlights clear when hover ends, returning to the default rainbow distribution
- Canvas labels rendered as colored circles with white border instead of triangle markers + text
- Replaced `Badge` component in session summary row with inline rainbow dot strip

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
