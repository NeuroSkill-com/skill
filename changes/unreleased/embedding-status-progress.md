### Features

- **Embedding coverage dashboard** in Settings → EXG → Re-embed. Shows a color-coded coverage bar (green ≥95%, amber ≥50%, red <50%), total/embedded/missing epoch counts, and estimated time remaining based on current embed speed.
- **Per-day breakdown table** in the re-embed section. Expandable table showing each day's total epochs, embedded count, missing count, and a mini coverage bar — makes it easy to identify which days have gaps.
- **Idle reembed status indicator**. When background embedding is enabled, the UI now shows whether it's actively processing (with day + progress), or waiting for the idle timeout to elapse, instead of running silently.
- **Re-embed progress improvements**. Progress bar now shows indeterminate animation during encoder loading, percentage + ETA during processing, amber bar when paused (device reconnected), red bar with error detail on failure.
- **EEG embedding coverage in Search page**. Corpus stats banner and empty-state panel now show epoch embedding coverage (e.g. "12,400/14,200 (87%)") with color coding, so you know at a glance whether your data is fully searchable.

### Server

- `GET /v1/models/estimate-reembed` response now includes `embedded`, `missing`, `coverage_pct`, `avg_embed_ms`, `eta_secs`, `per_day` (per-day breakdown array), and `idle_reembed` (background reembed status with active/idle_secs/done/total/current_day).
- `GET /v1/search/stats/stream` slow-tier stats now include `eeg_total_epochs`, `eeg_embedded_epochs`, and `eeg_missing_epochs`.
- New `IdleReembedStatus` struct in daemon state tracks background reembed progress and idle countdown, updated in real-time by the idle reembed loop.
