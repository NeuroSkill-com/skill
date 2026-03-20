### Features

- **API bearer token authentication**: Added optional bearer token authentication for the HTTP/WS API. When `api_token` is set in settings, all requests must include `Authorization: Bearer <token>`. Empty token (default) disables auth — suitable for localhost-only binds. Configurable via Settings UI and `get_api_token`/`set_api_token` Tauri commands. i18n keys added for all 5 languages.

### Refactor

- **Extract DND/sleep from ws_commands**: Moved DND status/set and sleep schedule get/set into dedicated `dnd_sleep.rs` sub-module. `ws_commands/mod.rs` reduced from 873 to 695 lines.
- **Delete orphaned `bt_monitor.rs`**: Removed 71-line dead file (Bluetooth radio check) that was never imported by any module.

### Features

- **Add tests for `skill-constants` and `skill-router`**: 18 tests for skill-constants (filter math, band continuity, channel counts, Emotiv sample rate derivation, MutexExt poison recovery) and 11 tests for skill-router (rounding precision, NaN/infinity handling, command list integrity).
