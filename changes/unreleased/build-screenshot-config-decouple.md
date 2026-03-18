### Build

- **Decouple skill-settings from skill-screenshots**: Moved `ScreenshotConfig` struct and its pure helper methods from `skill-screenshots` into `skill-settings`, breaking the transitive `skill-settings` → `skill-screenshots` → `xcap` → `pipewire` dependency chain. Crates like `skill-router`, `skill-history`, and `skill-settings` no longer require `libpipewire-0.3` to compile. The `fastembed_model_enum()` helper stays in `skill-screenshots` as a standalone function since it depends on the `fastembed` crate. `skill-screenshots` now imports `ScreenshotConfig` from `skill-settings` instead of owning it.

- **Feature-gate `xcap` in skill-screenshots**: Added a `capture` feature (default on) that gates the `xcap` dependency. The top-level binary sets `default-features = false` so `cargo clippy` works without `libpipewire-0.3` on dev machines. Screen capture gracefully returns `None` when the feature is disabled. CI and release builds work unchanged since pipewire is installed there.

### Bugfixes

- **Fix missing `storage_format` in `UserSettings::default()`**: Added the missing field initializer in the `Default` impl for `UserSettings` in `skill-settings`.

- **Fix duplicate `MUSE_SAMPLE_RATE` import**: Removed redundant import in `eeg_embeddings/mod.rs` and added missing `CHANNEL_NAMES` import.

- **Fix renamed IDUN field**: Updated `use_60hz` → `mains_freq_60hz` in `session_connect.rs` to match upstream `idun` crate API change.
