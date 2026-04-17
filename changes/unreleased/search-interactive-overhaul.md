### Features

- **Interactive search: EEG metrics on graph nodes.** Each EEG epoch node now carries its full metrics (engagement, relaxation, SNR, alpha/beta/theta, FAA, heart rate, etc.) via the `NeighborMetrics` struct, visible in the tooltip and detail panel.

- **Interactive search: session grouping & cross-session comparison.** Every node gets a `session_id` derived from its timestamp. The results include a session summary with averaged metrics, stddev, min/max engagement/SNR, duration, and band power ratios. The best session (highest avg engagement) is auto-flagged with a ★ marker.

- **Interactive search: cross-session trend chart.** SVG sparkline rendered above the session summary, showing engagement trend across sessions. Best session dot highlighted in green.

- **Interactive search: relevance scoring.** Each EEG node receives a composite `relevance_score` combining text similarity (50%), temporal distance (30%), and engagement (20%). Nodes are optionally sortable by relevance in the UI and rendered larger in the 3D graph for high-relevance matches.

- **Interactive search: SNR quality filter.** New `snrPositiveOnly` toggle (step 6) excludes EEG epochs with non-positive SNR (bad signal quality). Backed by server-side filtering before graph construction.

- **Interactive search: device filter.** Device dropdown (step 7) in the interactive pipeline, filtering EEG epochs by device name at the SQL query level via `get_session_timeseries_filtered()`.

- **Interactive search: date-range filter with presets.** Date-range inputs (step 8) constrain results to a time window. Quick preset buttons (24h, 7d, 30d) with a clear button.

- **Interactive search: EEG epoch ranking.** Rank-by dropdown (step 9) sorts EEG epochs by engagement, SNR, or relaxation before selecting top-k, surfacing the most interesting epochs first.

- **Interactive search: collapsible advanced filters.** Steps 6–9 are grouped behind an "Advanced filters" toggle to keep the default view clean. An amber dot indicator shows when any filter is active.

- **Interactive search: performance timing & system load.** Each search returns `embed_ms`, `graph_ms`, `total_ms` timing plus CPU usage and memory stats via `sysinfo`. Displayed in a compact perf bar.

- **Interactive search: search result caching.** LRU-style cache (8 entries, 5-minute TTL) in the daemon. Repeated searches with identical parameters return instantly without re-embedding.

- **Interactive search: CSV export.** "Export CSV" button in the sessions summary panel downloads a CSV with all session metrics.

- **Interactive search: search history.** Recent queries persisted in localStorage (max 10). Displayed as clickable chips in the empty state for quick re-runs. ArrowUp in empty textarea recalls the last query.

- **Interactive search: pipeline settings persistence.** All pipeline parameters (kText, kEeg, kLabels, reachMinutes, SNR, rankBy, advanced toggle) saved to localStorage and restored on page load.

- **Interactive search: 3D graph improvements.**
  - **Double-click to zoom:** smoothly flies the camera to the selected node with easeInOutQuad tweening.
  - **Minimap:** 80×80px SVG overlay in the bottom-right showing all node positions projected to 2D.
  - **Node kind filtering:** toggle checkboxes to hide/show EEG, Labels, and Screenshot nodes.
  - **Color mode switcher:** dropdown to recolor EEG nodes by timestamp, engagement, SNR, or session.
  - **Node sizing by relevance:** higher-relevance EEG nodes render 1.0–1.3× larger.
  - **Enhanced tooltips:** show relevance score, session ID, and full EEG metrics summary on hover.
  - **Edge kind labels:** selected node tooltip shows connected edge types (→ text_sim, ← eeg_bridge).
  - **Subtle grid floor:** transparent grid pattern at y=-15, adapts to dark/light theme.
  - **Reset view button:** "⌂ Reset" button + click-empty-space to fly camera back to default position.

- **Interactive search: node detail panel.**
  - Separate card below the graph with generous spacing and readable typography.
  - Colored left border matching node kind.
  - Breadcrumb trail showing the full path from query → text_label → eeg_point → found_label.
  - "More like this" button to re-search using the selected node's text.
  - Bookmark button to save interesting nodes as findings.
  - EEG sparkline: "Load EEG bands ±60s" fetches actual timeseries and renders α/β/θ band chart.
  - Screenshot preview: inline thumbnail for screenshot nodes.
  - Compare mode: select two EEG points for side-by-side metrics comparison with diff highlighting.

- **Interactive search: timeline scrubber.** Horizontal SVG timeline below the graph showing all timestamped nodes as color-coded dots. Click any dot to select that node.

- **Interactive search: insights & patterns panel.**
  - Auto-computed activity-engagement correlation: groups screenshots by app and shows avg engagement per app as a bar chart.
  - Hour-of-day engagement pattern: bar chart showing which hours have highest engagement.
  - Optimal conditions report: identifies peak engagement time and best app.
  - AI summary: sends search context to the local LLM for a natural-language analysis of patterns and recommendations.
  - Bookmark/findings system: save interesting nodes to localStorage, displayed in the empty state for future reference.

- **Interactive search: loading skeleton.** Graph-shaped SVG skeleton with animated nodes and edges replaces the simple spinner during search.

- **Interactive search: animated session bars.** Engagement and SNR progress bars in the session summary have smooth CSS transitions on load.

- **Interactive search: EEG epoch deduplication.** `seen_eeg_ts` HashSet prevents duplicate EEG nodes when multiple text labels have overlapping time windows.

- **Interactive search: parallel compare_search.** Both range queries in `/search/compare` now run concurrently via `tokio::join!`.

- **Interactive search: `get_session_timeseries_filtered()`.** New function in `skill-history` that filters timeseries by device name at the SQL level, used by the interactive search device filter.

### i18n

- Added 60+ new translation keys across all 9 locales (en, de, es, fr, he, ja, ko, uk, zh) covering all new search features: filters, insights, timeline, compare, bookmarks, color modes, and performance stats.
