### Bugfixes

- **Fix Windows CI zip assembly loading**: Load `System.IO.Compression` assembly explicitly before `System.IO.Compression.FileSystem` in the "Sign installer + create updater artifacts" step. On some PowerShell runtimes (notably PowerShell Core on GitHub Actions), loading only `FileSystem` does not implicitly load the base `System.IO.Compression` assembly, causing `Unable to find type [System.IO.Compression.ZipArchiveMode]` errors during NSIS updater zip creation.
