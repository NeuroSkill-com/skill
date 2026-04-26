# Personal Flow Triggers Dashboard

A sidebar section showing the developer's personal flow patterns mined from their EEG + activity history.

## What you see

In the NeuroSkill sidebar panel, a collapsible "Your Flow Recipe" section shows:

- **Best language** — "Focus best on Rust (82)" — from `/brain/code-eeg`
- **Peak hours** — "Peak hours: 9:00, 10:00, 14:00" — from `/brain/optimal-hours`
- **Natural cycle** — "Natural cycle: 42m" — from `/brain/break-timing`
- **Flow killer** — "Flow killer: Slack (focus 38 at switch)" — from `/brain/context-cost`

## Data sources

| Insight | API Endpoint | Time Range |
|---------|-------------|------------|
| Best languages | `/brain/code-eeg` | Last 7 days |
| Peak hours | `/brain/optimal-hours` | Last 7 days |
| Natural cycle | `/brain/break-timing` | Last 7 days |
| Flow killers | `/brain/context-cost` | Last 7 days |

## Settings

`neuroskill.flowTriggers` (default: `true`) — Show/hide the flow triggers section in the sidebar.

## Files

- `src/sidebar.ts` — `_fetchFlowTriggers()` and `_renderFlowTriggers()` methods
