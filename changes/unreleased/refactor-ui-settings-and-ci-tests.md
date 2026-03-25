### UI

- **Split large settings tabs into sub-components**: extracted reusable sections from `LlmTab`, `ToolsTab`, and `ScreenshotsTab` into `LlmServerSection`, `SkillsRefreshSection`, `SuggestSkillCta`, and `ScreenshotToggleCard` to reduce template size and improve maintainability.

### i18n

- **Stronger translation drift guard in CI**: added `npm run audit:i18n:check` to frontend CI so missing locale coverage is caught alongside sync checks.

### Build

- **Expanded integration-test crate detection**: CI now includes `skill-history` and `skill-settings` in integration-test crate selection.

### Refactor

- **Split `skill-settings` internal tests into a dedicated module file**: moved in-file tests from `src/lib.rs` into `src/tests.rs` to keep the primary module focused.

### Bugfixes

- **Added integration tests for settings/history contracts**: introduced new integration tests for `skill-settings` defaults/path behavior and `skill-history` metrics/PPG sidecar resolution (`exg_` and legacy `muse_` paths).

### Docs

- **Updated `TODO.md`**: removed completed item for decomposing `LlmTab`, `ToolsTab`, and `ScreenshotsTab` templates into sub-components.
