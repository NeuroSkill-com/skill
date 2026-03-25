# TODO

- [x] Raise Rust quality gates gradually: move key clippy lints from `warn` to `deny` for non-test code (start with `unwrap_used`, `panic`, and unsafe docs).
- [x] Reduce CI duplication by extracting shared setup (ONNX Runtime, Vulkan SDK, sccache/toolchain bootstrap) into reusable scripts or composite actions.
- [x] Harden CI supply chain by replacing floating action versions (e.g. `@latest`) with pinned versions/SHAs.
- [x] Make frontend lint blocking in CI after baseline cleanup (remove `continue-on-error` for Biome lint).
- [x] Add automated dependency update workflow (Dependabot or Renovate) for both npm and Cargo.
- [x] Add `CODEOWNERS` to define clear review ownership for crates, frontend, CI, and release scripts.
- [x] Refactor large mixed modules into smaller focused files; move large inline test blocks into dedicated test files where appropriate.
- [x] Add engineering health metrics/reporting (test duration + flakiness trends, compile-time hotspots, binary-size trends, optional coverage trend for critical crates).
- [x] Improve i18n completeness and consistency across all locales (not just specific languages): enforce missing-key checks and translation quality gates in CI.
