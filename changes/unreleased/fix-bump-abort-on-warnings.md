### Build

- **Bump aborts on warnings**: `npm run bump` now captures stdout and stderr from every preflight check step and scans for warning lines. If any warnings are detected, the bump is aborted before any files are modified. `cargo clippy` is invoked with `-D warnings` to promote Rust warnings to errors.
- **Bump mirrors CI checks**: preflight now runs all CI-equivalent steps — `npm test` (vitest), `cargo clippy` on all workspace crates (not just the app crate), and `cargo test --lib` on the same crate subset as CI — so issues are caught locally before pushing.
