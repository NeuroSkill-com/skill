### Dependencies

- **jiff 1.0, rusqlite 0.43, jsonschema 0.61**: `skill-time` moves to jiff 1.0 (the `Zoned` → `ZonedDateTime` rename stays inside that crate); rusqlite 0.43 brings a newer bundled SQLite for the legacy-store readers; jsonschema 0.61 for tool-argument validation. jiff 0.2 stays in the lock through `env_logger`.
- **Frontend**: `@sveltejs/kit` 3.3.1, `@tauri-apps/api` and `@tauri-apps/cli` 2.12.5, `maplibre-gl` 6.19, `marked` 18.2.1.
