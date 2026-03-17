### Bugfixes

- **Device API save error handling**: `saveEmotivApi` and `saveIdunApi` now wrap the Tauri invoke in `try/catch`; failed saves surface an inline error message instead of silently flashing "Saved".
- **IDUN token env var safety**: replaced `std::env::set_var("IDUN_API_TOKEN", …)` (process-wide mutation, unsafe in multi-threaded Rust) with passing the token via `GuardianClientConfig::api_token`.

### Features

- **RE-AK device image**: `deviceImage()` in both DevicesTab and SettingsTab now maps Nucleus-Hermès BLE names (`hermes`, `nucleus`, `re-ak`, `reak`) to the correct device image.

### i18n

- **Device API section fully translated**: all hardcoded strings in the Device API card (section title, provider titles, descriptions, field labels, show/hide, save/saved) are now driven by `t()` keys under `settings.deviceApi.*`.
