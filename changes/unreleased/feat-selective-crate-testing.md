### Performance

- **Selective crate testing in CI**: `cargo test` now only runs for crates affected by the current changeset. A new `scripts/changed-crates.sh` script computes the transitive closure of reverse dependencies from changed files, so PRs that touch a single crate skip unrelated test suites. Both unit tests (`--lib`) and integration tests (`--test '*'`) are selectively run for affected crates. Workspace-wide changes (Cargo.lock, .cargo/config.toml, ci.yml) still trigger a full test run as a safety net.
