### Build

- **Faster local hooks**: enabled automatic `sccache` usage in `.githooks/pre-commit` and `.githooks/pre-push` when available, including C/C++ launcher integration for cmake-based crates.
- **Docs-only fast path**: both local hooks now skip expensive frontend/Rust checks for docs/changelog-only changes.
