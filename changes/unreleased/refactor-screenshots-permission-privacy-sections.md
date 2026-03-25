### UI

- **Extracted screenshots permission notice into a dedicated component**: moved macOS screen-recording warning/success UI into `src/lib/screenshots/ScreenshotPermissionNotice.svelte`.
- **Extracted screenshots privacy note into a dedicated component**: moved the storage/privacy footer block into `src/lib/screenshots/ScreenshotPrivacyNote.svelte`.

### Refactor

- **Further streamlined `ScreenshotsTab` composition**: `ScreenshotsTab` now delegates permission and privacy presentation to focused child components, keeping the tab file centered on state orchestration.
