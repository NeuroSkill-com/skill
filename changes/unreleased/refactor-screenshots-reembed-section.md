### UI

- **Extracted screenshots re-embed/status block into a dedicated component**: moved model-change warning, re-embed controls, progress bar, and stats from `src/lib/ScreenshotsTab.svelte` into `src/lib/screenshots/ScreenshotReembedSection.svelte`.

### Refactor

- **Further reduced `ScreenshotsTab` orchestration scope**: removed local ETA formatting/render logic from `ScreenshotsTab` and delegated re-embed presentation details to `ScreenshotReembedSection`.
