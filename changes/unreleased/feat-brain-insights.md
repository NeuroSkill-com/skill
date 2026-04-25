### Features

- **Developer insights endpoint**: `POST /v1/brain/developer-insights` returns 7 actionable EEG-fused insights in one call.
- **Test failure by focus level**: correlates build/test exit codes with EEG focus (high/mid/low) — shows how focus level predicts test outcomes.
- **Hourly productivity**: churn volume + undo rate + avg EEG focus by hour of day — identifies peak and trough hours.
- **Context switch recovery**: EEG focus level at each editor/terminal/panel transition — measures the cognitive cost of switching.
- **AI tool impact on focus**: per-app (Claude, Pi) focus delta vs baseline — quantifies whether AI tools help or hurt flow state.
- **Focus by language**: avg EEG focus + undo rate per programming language — identifies which languages are cognitively easiest/hardest.
- **Dev loop efficiency by hour**: build/test cycle time + pass rate by time of day — shows when edit-test loops are fastest.
- **Tool focus impact**: avg EEG focus per command category (docker, git, deploy, etc.) — reveals which tools correlate with lowest focus.
- **EEG timeseries table**: periodic JSON snapshots of all brain metrics (every 5s during recording). Extensible — new metrics without schema changes. Any event correlatable by timestamp join.
- **EEG timeseries worker**: background thread writes full band powers to `eeg_timeseries` every 5s when an EEG session is active.
- **`POST /v1/brain/eeg-at`**: get EEG metrics at a specific timestamp (nearest sample).
- **`POST /v1/brain/eeg-range`**: get EEG time-series in a range for charts and correlation analysis.
- **Window focus tracking**: `window_focus` events sent when VS Code gains/loses focus. EEG not attributed to coding when VS Code is in background.
- **Live EEG attachment**: `latest_bands` focus/mood injected at event storage time for terminal commands, zone switches, and conversations.

### Tauri UI

- **Brain Insights section** in Activity tab: test failure rates by focus level, focus by language bars, AI impact delta, tool focus grid, hourly productivity chart.
- **EEG focus inline**: terminal commands and conversation messages show EEG focus score (green/yellow/red) at the moment they occurred.

### VS Code Sidebar

- **Brain Insights card**: test failure by focus chips, focus by language bars, tool impact chips. Only shown when EEG data is available.
- **Struggle card**: hidden when no EEG recording (focus is EEG-only, not activity-based).
- **Flow gauge label**: shows "focus" with EEG, "activity" without — no false claim of brain measurement.
