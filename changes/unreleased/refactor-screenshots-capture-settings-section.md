### UI

- **Extracted screenshots capture settings panel into a dedicated component**: moved interval/image-size/quality controls and embedding backend/model selectors from `src/lib/ScreenshotsTab.svelte` into `src/lib/screenshots/ScreenshotCaptureSettingsSection.svelte`.

### Refactor

- **Simplified capture-setting updates in `ScreenshotsTab`**: centralized capture field patching via a small `onUpdate` bridge passed to `ScreenshotCaptureSettingsSection`, including recommended-size adoption on backend/model changes.
