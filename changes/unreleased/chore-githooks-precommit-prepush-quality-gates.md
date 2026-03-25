### Build

- **Standardized local quality gates**: expanded `.githooks/pre-commit` to run i18n checks, frontend checks (`svelte-check` + unit tests), and targeted Rust clippy/tests for touched crates.
- **Added heavy pre-push gate**: new `.githooks/pre-push` runs full frontend checks, full Rust clippy (workspace + app features), and workspace Rust library tests before push.

### Bugfixes

- **Latest CI failures resolved locally**: fixed frontend Biome formatting drift and removed a Windows-only unnecessary raw-pointer cast in `src-tauri/src/skill_log.rs`.
- **Hook shell compatibility**: replaced `mapfile` usage in `.githooks/pre-commit` with a bash-3-compatible dedupe loop so commits work on macOS default bash.
