### Dependencies

- **Fix rand 0.7.3 vulnerability**: vendored `phf_generator 0.8.0` with rand bumped from 0.7 to 0.8, eliminating the vulnerable transitive dependency.
- **Remove atty**: migrated `iroh_test_client` from `structopt` (clap v2) to `clap v4` derive, dropping the unmaintained `atty` crate.
- **Update llama-cpp-4 to 0.2.50**: upstream llama.cpp fixes including `common_*` symbol renames.
- **Update kittentts to 0.4.1**: TTS engine update.
- **Add grayscale 0.0.1**: macOS grayscale display control for DND mode.

### Bugfixes

- **Dismiss stale Dependabot alerts**: crossbeam-deque, crossbeam-utils, crossbeam-queue, memoffset, and glib alerts dismissed (already at patched versions or fixed in fork).
