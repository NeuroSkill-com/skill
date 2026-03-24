### Bugfixes

- **Fix Windows NSIS packaging strict-mode crash**: normalize the `$missingCrt` result to an array before reading `.Count` in `scripts/create-windows-nsis.ps1`. This prevents a PowerShell strict-mode failure when `Where-Object` returns `$null` (no missing CRT DLLs).
