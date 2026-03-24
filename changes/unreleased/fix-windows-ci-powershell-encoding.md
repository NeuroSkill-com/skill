### Bugfixes

- **Fix Windows CI PowerShell parse error**: Replaced literal em dash characters in PowerShell `run:` blocks in `release-windows.yml`. Non-ASCII in CI scripts can cause encoding corruption and parser failures on Windows runners.

### Performance

- **Fix sccache on Windows CI (57 min -> ~20 min builds)**: The `sccache-action` v0.0.9 on Windows fails to add the sccache directory to the Windows `Path` variable. Git Bash can find sccache (Unix PATH), but native processes like `cargo.exe` cannot, so `RUSTC_WRAPPER=sccache` silently fails and every build compiles 840+ crates from scratch with zero caching. Fixed by resolving sccache to its full Windows path and exporting `RUSTC_WRAPPER` as an absolute path via `GITHUB_ENV`. Also adds the sccache directory to `GITHUB_PATH` so cmake compiler launchers can find it.
