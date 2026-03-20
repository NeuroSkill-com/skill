### i18n

- **Migrate hardcoded English strings to i18n**: Replaced 12 raw English `title` attributes with `t()` calls across 8 components (ChatMessageList, CustomTitleBar, DevicesTab, TimeSeriesChart, dashboard, compare, api). Added 13 new i18n keys (`common.copy`, `common.copyToClipboard`, `common.newer`, `common.older`, `common.clickToHide`, `common.clickToReveal`, `common.goalReached`, `common.resetZoom`, `common.openComparison`, `common.error`, `error.description`, `error.goHome`, `error.reload`) for all 5 languages.

### UI

- **Add SvelteKit root error boundary**: New `+error.svelte` at the route root catches unhandled errors across all 16 routes with a friendly message, "Go to Dashboard" link, and "Reload Page" button — prevents blank white screens on `invoke()` failures.

### Refactor

- **Extract `load_and_apply_settings` from `setup_app`**: Moved the 80-line settings hydration block into its own `#[inline(never)]` function, reducing `setup_app` from 574 to ~490 lines.

### Features

- **Add tests for `skill-gpu`**: 3 tests covering struct construction, JSON serialization, and `read()` no-panic guarantee.
