### Build

- **Decouple skill-settings from skill-screenshots**: Moved `ScreenshotConfig` struct and its pure helper methods from `skill-screenshots` into `skill-settings`, breaking the transitive `skill-settings` → `skill-screenshots` → `xcap` → `pipewire` dependency chain. Crates like `skill-router`, `skill-history`, and `skill-settings` no longer require `libpipewire-0.3` to compile. The `fastembed_model_enum()` helper stays in `skill-screenshots` as a standalone function since it depends on the `fastembed` crate. `skill-screenshots` now imports `ScreenshotConfig` from `skill-settings` instead of owning it.

### Bugfixes

- **Fix missing `storage_format` in `UserSettings::default()`**: Added the missing field initializer in the `Default` impl for `UserSettings` in `skill-settings`.
