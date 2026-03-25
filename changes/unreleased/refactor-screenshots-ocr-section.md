### UI

- **Extracted screenshots OCR panel into a dedicated component**: moved OCR engine selection, model info, and search-hint UI from `src/lib/ScreenshotsTab.svelte` into `src/lib/screenshots/ScreenshotOcrSection.svelte`.

### Refactor

- **Further slimmed `ScreenshotsTab` composition**: `ScreenshotsTab` now delegates toggle, OCR, and performance rendering to focused child components while keeping shared state and persistence logic in one place.
