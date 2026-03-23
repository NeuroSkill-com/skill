### Bugfixes

- **Fix Windows auto-update "unsupported compression method"**: Replaced PowerShell `Compress-Archive` with .NET `ZipFile` using explicit Deflate compression (method 8) when creating updater `.nsis.zip` archives. `Compress-Archive` on newer Windows versions uses Deflate64 (method 9), which the Tauri updater's zip crate does not support.
