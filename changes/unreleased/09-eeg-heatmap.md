### UI

- **EEG focus timeline heatmap**: collapsible "Focus Timeline" sidebar section renders a ~280×36 px SVG sparkline of today's EEG focus (`/brain/eeg-range`, max 120 points), color-graded green/yellow/red, with hour labels and file-name annotations at peaks/valleys (merged from `/activity/timeline`). Toggle via `neuroskill.eegHeatmap`.

## What you see

In the NeuroSkill sidebar, a collapsible "Focus Timeline" section shows:

- A ~280px wide, ~36px tall SVG sparkline
- Color gradient: green (>70 focus), yellow (40-70), red (<40)
- Hour labels along the bottom (0:00, 3:00, 6:00, ...)
- File names annotated at focus peaks and valleys

## Data sources

| Data | API Endpoint |
|------|-------------|
| EEG time-series | `/brain/eeg-range` (today, max 120 points) |
| File context | `/activity/timeline` (today, last 200 events) |

The heatmap merges EEG data points with the closest timeline events to show which files correspond to focus peaks and dips.

## Settings

`neuroskill.eegHeatmap` (default: `true`) — Show/hide the heatmap in the sidebar.

## Files

- `src/sidebar.ts` — `_fetchHeatmap()` and `_renderHeatmap()` methods
