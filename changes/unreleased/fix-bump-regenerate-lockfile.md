### Bugfixes

- **Regenerate Cargo.lock during bump**: `npm run bump` now runs `cargo generate-lockfile` after updating version in `src-tauri/Cargo.toml`, preventing CI `--locked` build failures due to stale lockfile.
