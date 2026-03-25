### UI

- **Extracted screenshot pipeline performance UI into a dedicated component**: moved the large performance charts and breakdown panel from `src/lib/ScreenshotsTab.svelte` into `src/lib/screenshots/ScreenshotPerformanceSection.svelte`.

### Refactor

- **Reduced `ScreenshotsTab` template complexity**: removed in-file chart formatting/render helpers from `ScreenshotsTab` and delegated rendering to `ScreenshotPerformanceSection`, keeping tab state/update logic focused.
