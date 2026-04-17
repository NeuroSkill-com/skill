### Features

- **Interactive search: AI summary with streaming.** "Generate" button in the Insights panel sends search context (labels, EEG metrics, sessions, screenshots) to the local LLM via `chat_completions_ipc` with a channel callback. Response streams token-by-token with live markdown rendering via `MarkdownRenderer`. Thinking blocks are shown in a collapsible `<details>` element.

- **Interactive search: rich AI prompt with full EEG context.** The LLM prompt now includes:
  - Time range and epoch count
  - Labels with timestamps and similarity distances
  - EEG epoch metrics: engagement, relaxation, SNR, α/β/θ band powers, heart rate, relevance score, session ID
  - On-demand timeseries fetch when graph nodes lack stored metrics (computes averages from raw data)
  - Session metrics with stddev, best-session flag
  - Screenshot context: app name, window title, timestamp, OCR similarity
  - Fallback session derivation from node session_ids when backend sessions are empty

- **Interactive search: prompt visibility.** Collapsible "Prompt" section shown immediately when "Generate" is clicked (open by default during generation, collapses after). Shows the exact data sent to the LLM in monospace font.

- **Interactive search: LLM auto-start.** When the LLM is not running, shows "Start LLM and try again" button that calls `start_llm_server`, waits for initialization, and retries the summary automatically.

- **Interactive search: Continue in Chat.** "Continue in Chat →" button after the AI summary creates a new chat session named "Search: {query}", saves the prompt and response as messages, and opens the chat window. Users can ask follow-up questions about their search results.

- **Interactive search: auto-save to chat history.** Every AI summary is automatically saved as a chat session, making all LLM conversations from search discoverable in the chat history regardless of whether the user clicks "Continue in Chat".

- **Interactive search: insights panel.** Collapsible "Insights & Patterns" card with:
  - Optimal conditions: auto-detected peak engagement hour and best app
  - App × Engagement correlation: bar chart showing avg engagement per app (uses `window_title` fallback)
  - Hour-of-day engagement pattern: bar chart across 24 hours
  - AI summary with streaming, thinking blocks, markdown rendering, and chat integration

- **Interactive search: bookmark/findings system.** Star button in node detail panel saves nodes to localStorage (max 50). Saved findings appear in the empty state with quick re-search and delete controls.

- **Interactive search: graph color mode switcher.** Dropdown to recolor EEG nodes by timestamp (turbo), engagement level, SNR quality, or session (hash-based). Graph reactivity fixed with explicit dependency tracking.

- **Interactive search: timeline scrubber.** Horizontal SVG timeline below the graph showing all timestamped nodes as color-coded clickable dots. Clicking selects the node in the detail panel.

- **Interactive search: EEG sparkline.** "Load EEG bands ±60s" button on EEG point nodes fetches actual timeseries data and renders α (blue), β (amber), θ (green) band power chart with a red marker at the epoch timestamp.

- **Interactive search: screenshot preview.** Inline screenshot thumbnail in the detail panel when a screenshot node is selected.

- **Interactive search: compare mode.** Select two EEG points for side-by-side metrics comparison with green/red diff highlighting and time gap display.

- **Interactive search: node detail panel improvements.** Moved to a separate card below the graph with colored left border, generous spacing, text-sm font sizes, responsive metrics grid, breadcrumb trail, and "More like this" / bookmark buttons.

- **Interactive search: 3D graph enhancements.** Double-click zoom with smooth camera tween, minimap, node kind filtering, subtle grid floor, reset view button, enhanced tooltips with metrics/session/relevance.

### i18n

- Added 80+ translation keys across all 9 locales (en, de, es, fr, he, ja, ko, uk, zh) for AI summary, insights, bookmarks, color modes, timeline, compare, LLM controls, and chat integration.
