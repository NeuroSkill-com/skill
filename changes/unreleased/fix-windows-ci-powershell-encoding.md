### Bugfixes

- **Fix Windows CI PowerShell parse error**: Replaced literal em dash characters (`—`) with ASCII `--` inside PowerShell `run:` blocks in `release-windows.yml`. Non-ASCII in CI scripts can cause encoding corruption and parser failures on Windows runners.
