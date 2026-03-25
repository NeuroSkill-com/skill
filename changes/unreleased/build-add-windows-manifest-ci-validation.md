### Build

- **Validate Windows manifest in CI**: added `scripts/check_windows_manifest.py` and wired it into `ci.yml` and `release-windows.yml` so malformed `src-tauri/manifest.xml` fails fast before Windows build/release.
