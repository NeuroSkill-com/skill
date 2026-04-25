### Features

- **Design tokens** (`webview-ui/src/lib/tokens.css`): 30+ CSS custom properties organized into core palette (5 colors), surfaces (4 levels), backgrounds (14 translucent tints), borders (3 colors), semantic aliases (7), and typography (3). Zero inline `rgba()` in App.svelte.
- **Reusable Svelte components** (`webview-ui/src/lib/`):
  - `Card` — wrapper with title, info button, info panel, header-right slot, variant borders (warn/danger/info)
  - `MetricRow` — label + value pair with variant colors (warn/accent/dim/good/bad)
  - `Chevron` — collapsible section with chevron toggle, count badge, slot content
  - `ProgressBar` — horizontal bar with value/max, 5 color variants, optional label
  - `Gauge` — circular SVG ring with animated fill, value, label
  - `Badge` — text badge with 7 variants (default/good/warn/bad/blue/live/score-circle/si)
  - `Callout` — alert box with 3 variants (warn/danger/info)
- **Component index** (`webview-ui/src/lib/index.ts`): barrel export for all components.
- **Timestamp compliance**: 25 automated tests (`src/test-timestamps.ts`) verifying:
  - No `toLocaleTimeString`/`toLocaleDateString` in data layer (activity-tracker, events, brain, config, vt-parser)
  - `toLocaleTimeString` used in UI layer (App.svelte) for display
  - `Date.now()` returns UTC milliseconds
  - ISO 8601 strings parsed to UTC millis
  - No hardcoded timezone offsets in data layer
  - All stored timestamps are UTC; local conversion only at UI boundary
