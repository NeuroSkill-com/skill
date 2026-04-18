### Bugfixes

- **Fixed EEG metrics not appearing in search results.** The `MetricsBlob` deserializer in `skill-history/cache.rs` used `#[serde(default)]` on `f64` fields, which caused the entire JSON deserialization to fail silently when any field was `null` (e.g., `cognitive_load: null`, `drowsiness: null`). All metrics defaulted to zero, causing every epoch to show "(no EEG metrics stored)" in the AI summary prompt. Fixed by adding a `null_as_zero` custom deserializer that treats JSON `null` as `0.0` without failing the parse.

- **Fixed search cache serving stale results after daemon restart.** Added `cache_clear()` to the search module and call it after metrics backfill completes, ensuring enriched metrics appear immediately.

### Features

- **Per-epoch EEG metrics in AI summary prompt.** The interactive search AI summary now includes per-epoch metrics (engagement, relaxation, SNR, α/β/θ band powers, FAA, mood, TAR, meditation, cognitive load, drowsiness, heart rate) instead of just "(no EEG metrics stored)". Each 5-second epoch shows its own brain state data.

- **On-the-fly CSV metrics fallback.** When embeddings lack `metrics_json`, the interactive search now lazily loads session `_metrics.csv` files and matches epochs by timestamp (±3s tolerance) to populate metrics from CSV data. This is transparent to the user — metrics appear without needing a backfill.

- **Background metrics backfill from CSV.** New `POST /v1/analysis/backfill-metrics` endpoint patches `metrics_json` in the SQLite embeddings table for rows where it is NULL, matching against `_metrics.csv` by timestamp. Fires automatically on search page load (fire-and-forget). After first run, all future searches find metrics directly in the DB.

- **Recomputation of derived EEG metrics.** When `meditation`, `cognitive_load`, `drowsiness`, or `stress_index` are null in stored `metrics_json` but band powers (α, β, θ, δ) are present, they are recomputed on-the-fly using the same formulas as the live pipeline.

- **Shared EEG score formulas (`skill-data/eeg_scores.rs`).** New module with pure-math functions for computing derived EEG metrics from averaged band powers.
