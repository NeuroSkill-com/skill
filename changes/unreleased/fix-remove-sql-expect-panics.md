### Bugfixes

- **Remove SQL expect() panics in screenshot and health stores**: Replaced 26 `.expect("static SQL")` / `.expect("SQL query")` calls in `skill-data` crate (`screenshot_store.rs`, `health_store.rs`) with graceful fallbacks (`let-else` returning empty vectors, `if-let` skipping failed prepare). The app no longer panics on database corruption, disk-full, or other runtime SQLite errors in these code paths.
