### Bugfixes

- **Fix ™ symbol mangled in Windows NSIS installer**: The `.nsi` script was written as UTF-8 without BOM. NSIS with `Unicode True` requires a BOM to detect UTF-8; without it, NSIS falls back to the system ANSI codepage and corrupts non-ASCII characters like ™ in the product display name, version info, registry entries, and shortcuts. Changed to UTF-8 with BOM.
