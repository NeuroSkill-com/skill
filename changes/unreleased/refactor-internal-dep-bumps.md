### Dependencies

- **`llmfit-core` 0.9.38 → 1.1.17**, which replaces its `serde_yml` YAML stack with `yaml_serde`. That takes `serde_yml` and `noyalib` out of the build and with them two security advisories — a YAML resource-budget DoS (`RUSTSEC-2026-0333`) and `serde_yml`'s own unsound/unmaintained notice (`RUSTSEC-2025-0068`) — which were otherwise unfixable by upgrade, since the patched `noyalib` is semver-incompatible with the one `serde_yml` pins. skill uses the crate for hardware detection and model-fit scoring, and the API it depends on is unchanged: `FitLevel` and `RunMode` still have exactly the variants the UI maps, so fit verdicts keep their meaning. The structs gained one field each (`LlmModel.languages`, `SystemSpecs.gpu_available_gb`).
- **`turbovec` 0.4.1 → 1.1.2, which now has no dependencies at all.** It does its own SIMD instead of a BLAS GEMM, so the 42 MB OpenBLAS build dependency goes away along with `faer`, `gemm`, `nano-gemm`, `pulp`, `blas-src` and a dozen of their companions. Index construction and vector insertion became fallible (a bit width outside 2–4, a dim that is not a multiple of 8, a dim mismatch), and both are now handled by falling back to HNSW for that index rather than by panicking.
- **Security updates**: `rustls` 0.23.45 → 0.23.46 (four advisories: ECH data leakage, brotli pre-auth allocation, `Debug` impls exposing session secrets, and an unauthenticated ServerHello downgrade), `event-listener` 5.4.1 → 5.4.2, `lru` 0.18.1 → 0.18.5.
- Net effect on the lockfile against the last tagged release: **36 crates leave, 3 arrive.** `btleplug` and `dashmap` are among the departures, from the Bluetooth work in this same release.

### Server

- **Existing label indices rebuild themselves once after this update.** turbovec 0.4 wrote format v2 and 1.1 reads only v7/v8, so the first load of an old `.tv` file fails with an error naming the version, the index falls back to HNSW for that run, and the next rebuild writes the new format. Nothing is lost — every embedding lives in `labels.sqlite` — and no action is required.
- **TurboVec encodings are now platform-independent.** In 0.4 the rotation ran through whichever BLAS the OS supplied, so the same vectors encoded to different bytes on macOS and Linux; 1.1 computes it the same way everywhere, which is also why the on-disk format moved.

### Bugfixes

- **TurboVec node counts no longer drift.** A label was counted before its vector was handed to the index, so any rejected insert left the reported node total one ahead of what the index actually held — visible in the backend footprint shown in settings. The count now follows the insert.
