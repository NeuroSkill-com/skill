# TODO

- [ ] split `skill-history/src/lib.rs` (1753 lines) into focused sub-modules (types, listing, metrics, embedding) — needs careful dependency untangling between `patch_session_timestamps` and cross-module references.

- [ ] split `skill-settings/src/lib.rs` (1124 lines) into domain-specific modules (eeg, llm, screenshot, ui settings).

- [ ] add `criterion` benchmark suite for DSP/FFT hot paths to catch performance regressions.

- [ ] break up large Svelte components (UmapViewer3D 1962, DevicesTab 1358, LlmTab 1243) into composable sub-components.

- [ ] complete i18n translations for de/ (8 missing keys) and he/ (1 missing key).
