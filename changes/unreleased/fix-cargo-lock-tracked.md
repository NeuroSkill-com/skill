### Build

- **Commit Cargo.lock**: Removed `Cargo.lock` from `.gitignore` and committed it so that `cargo clippy --locked` and CI builds succeed without needing to regenerate the lockfile.
