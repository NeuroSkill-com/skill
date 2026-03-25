### Build

- **CI lint gate enabled**: made Biome lint blocking in CI by removing advisory `continue-on-error` behavior.
- **Pinned apt-cache action version**: replaced floating `awalsh128/cache-apt-pkgs-action@latest` with `@v1` in CI/release workflows.
- **Reduced CI duplication for ONNX setup**: extracted Linux ONNX Runtime installation/export logic into `scripts/install-onnxruntime-linux.sh` and wrapped it in reusable action `.github/actions/setup-onnxruntime-linux`, now used by both Linux CI jobs.
- **Reduced CI duplication for Vulkan setup**: added reusable action `.github/actions/setup-vulkan-linux` (cache + install) and adopted it in Linux CI and release workflows.
- **Reduced CI duplication for Rust bootstrap**: added reusable action `.github/actions/setup-rust-bootstrap-linux` and adopted it in Linux CI/release jobs for Rust toolchain + sccache setup.
- **Engineering health metrics in CI/release**: added per-job metrics artifacts and step summaries for Rust/frontend test durations, sccache stats, cargo compile timings upload, and Linux release binary/package sizes.
- **Automated dependency updates**: added `.github/dependabot.yml` to schedule weekly npm and Cargo dependency update PRs.
- **Repository ownership map**: added `.github/CODEOWNERS` assigning crates, frontend, CI workflows, and release scripts to `@eugenehp`.
- **Rust lint hardening (phase 1)**: promoted critical clippy lints in workspace config (`unwrap_used`, `panic`, `undocumented_unsafe_blocks`) from `warn` to `deny`.

### i18n

- **All-locale i18n checks**: updated `scripts/sync-i18n.ts` and `scripts/audit-i18n.ts` to discover non-English locales dynamically from `src/lib/i18n`, so CI checks automatically cover every locale instead of a hardcoded subset.

### Refactor

- **Parse module test split**: moved large inline tests from `crates/skill-tools/src/parse/mod.rs` into dedicated `crates/skill-tools/src/parse/tests.rs` and kept `mod.rs` focused on module exports.
