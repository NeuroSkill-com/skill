### Build

- **Bump aborts on warnings**: `npm run bump` now captures stdout and stderr from every preflight check step and scans for warning lines. If any warnings are detected, the bump is aborted before any files are modified. Additionally, `cargo clippy` is now invoked with `-D warnings` to promote Rust warnings to errors.
