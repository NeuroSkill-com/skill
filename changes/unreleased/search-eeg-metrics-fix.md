### Bug Fixes

- **Fixed EEG metrics not appearing in search results.** The `MetricsBlob` deserializer in `skill-history/cache.rs` used `#[serde(default)]` on `f64` fields, which caused the entire JSON deserialization to fail silently when any field was `null` (e.g., `cognitive_load: null`, `drowsiness: null`). All metrics defaulted to zero, causing every epoch to show "(no EEG metrics stored)" in the AI summary prompt. Fixed by adding a `null_as_zero` custom deserializer that treats JSON `null` as `0.0` without failing the parse.

- **Fixed search cache serving stale results after daemon restart.** Added `cache_clear()` to the search module and call it after metrics backfill completes, ensuring enriched metrics appear immediately.

### Features

- **Per-epoch EEG metrics in AI summary prompt.** The interactive search AI summary now includes per-epoch metrics (engagement, relaxation, SNR, α/β/θ band powers, FAA, mood, TAR, meditation, cognitive load, drowsiness, heart rate) instead of just "(no EEG metrics stored)". Each 5-second epoch shows its own brain state data.

- **On-the-fly CSV metrics fallback.** When embeddings lack `metrics_json`, the interactive search now lazily loads session `_metrics.csv` files and matches epochs by timestamp (±3s tolerance) to populate metrics from CSV data. This is transparent to the user — metrics appear without needing a backfill.

- **Background metrics backfill from CSV.** New `POST /v1/analysis/backfill-metrics` endpoint patches `metrics_json` in the SQLite embeddings table for rows where it is NULL, matching against `_metrics.csv` by timestamp. Fires automatically on search page load (fire-and-forget). After first run, all future searches find metrics directly in the DB.

- **Recomputation of derived EEG metrics.** When `meditation`, `cognitive_load`, `drowsiness`, or `stress_index` are null in stored `metrics_json` but band powers (α, β, θ, δ) are present, they are recomputed on-the-fly using the same formulas as the live pipeline:
  - **Meditation:** `α×200 − β×100 + stillness + HRV` (0–100)
  - **Cognitive load:** sigmoid of frontal θ / parietal α ratio (0–100)
  - **Drowsiness:** TAR/3 × 80 + α spindle component (0–100)
  - **Stress index:** Baevsky approximation from HR, RMSSD, SDNN

- **Shared EEG score formulas (`skill-data/eeg_scores.rs`).** New module with pure-math functions for computing derived EEG metrics from averaged band powers. Used by the history cache for recomputation; the live pipeline in `skill-devices` retains its per-channel implementation for higher accuracy.

### Technical

- **`skill-history/cache.rs`:** Added `backfill_eeg_metrics()`, `lookup_csv_metrics_for_range()`, `find_closest_csv_epoch()`, `epoch_row_to_metrics_json()`, `MetricsBlobOut` struct, and `BackfillResult`.
- **`skill-daemon/routes/search.rs`:** Added `cache_clear()`, lazy CSV fallback in `interactive_search_impl`, populated `meditation`/`cognitive_load`/`drowsiness` in `metrics_from_epoch`.
- **`skill-daemon/routes/analysis.rs`:** Added `/analysis/backfill-metrics` route.
- **`skill-data/eeg_scores.rs`:** New module with `meditation()`, `cognitive_load()`, `drowsiness()`, `stress_index()` and unit tests.
- **Frontend (`+page.svelte`):** Added FAA, mood, TAR, meditation, cognitive load, drowsiness to the AI summary prompt builder. Fire-and-forget backfill call on search page mount.
- **Frontend (`invoke-proxy.ts`):** Registered `backfill_eeg_metrics` command.

### Tests

- **Rust (skill-history):** `timeseries_returns_nonzero_engagement_from_metrics_json`, `timeseries_returns_zero_engagement_from_empty_metrics_json`, `timeseries_null_metrics_json_gives_zero_engagement`, `timeseries_14digit_timestamps_return_metrics`, `epoch_row_to_metrics_json_roundtrip`, `find_closest_csv_epoch_exact_match`, `find_closest_csv_epoch_within_tolerance`, `find_closest_csv_epoch_outside_tolerance`, `find_closest_csv_epoch_empty_slice`, `backfill_skips_dir_without_csvs`, `backfill_empty_dir_is_noop`, `real_data_timeseries_has_metrics` (ignored, requires live data).
- **Rust (skill-daemon):** `metrics_from_epoch_returns_some_for_nonzero_metrics`, `metrics_from_epoch_returns_none_for_all_zero`, `timeseries_to_metrics_pipeline_with_valid_json`, `timeseries_to_metrics_pipeline_with_null_json`, `cache_clear_removes_all_entries`.
- **Rust (skill-data):** `meditation_basic`, `meditation_high_alpha_high_score`, `cognitive_load_low_ratio`, `cognitive_load_high_ratio`, `drowsiness_low_tar`, `drowsiness_high_tar`, `stress_zero_inputs`, `stress_reasonable_range`.
- **Vitest:** AI Summary EEG metrics display — 5 tests covering metrics populated, null, empty, all-zero, and HR display.
