### Dependencies

- **Fix rand 0.7.3 vulnerability**: vendored `phf_generator 0.8.0` locally with `rand` bumped from 0.7 to 0.8, eliminating the vulnerable `rand 0.7.3` transitive dependency pulled in by `selectors 0.24 → phf_codegen 0.8`.
- **Remove atty**: migrated `iroh_test_client` from `structopt` (clap v2) to `clap v4` derive, dropping the unmaintained `atty` crate.
- **Dismiss stale Dependabot alerts**: dismissed alerts for `crossbeam-deque`, `crossbeam-utils`, `crossbeam-queue`, `memoffset`, and `glib` — all already at patched versions or fixed in fork.
