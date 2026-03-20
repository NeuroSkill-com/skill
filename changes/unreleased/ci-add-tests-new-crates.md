### CI

- **Run Rust tests in CI**: Added `cargo test` step to the CI pipeline covering 13 testable crates (486 tests). Previously CI only ran `cargo check` and `clippy` — tests were never executed.
- **Add new crates to CI clippy**: Added `skill-health`, `skill-gpu`, `skill-screenshots`, and `skill-llm` to the workspace clippy check (were missing since extraction).
