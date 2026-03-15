### Bugfixes

- **Fix Windows CI PowerShell parse errors**: Added UTF-8 BOM to `create-windows-nsis.ps1`, `release-windows.ps1`, and `setup-build-cache.ps1` so Windows PowerShell 5.1 correctly reads non-ASCII characters (™, —). Replaced `?.` null-conditional operator (PowerShell 7+ only) in `release-windows.ps1` with a PS 5.1-compatible alternative.
