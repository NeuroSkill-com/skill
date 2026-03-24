### Bugfixes

- **CI: remove skill-screenshots from cargo test step**: `skill-screenshots` has no `#[test]` items but pulling it into the test step forced a link of `ort-sys`/`libonnxruntime`, which fails on Linux because `cargo clippy` type-checks without final linking while `cargo test` must produce a real binary. Removing it from the `-p` list eliminates the spurious ORT link failure with zero loss of test coverage; `cargo clippy --workspace` continues to verify the crate compiles correctly.
