### Refactor

- **Canvas chart dedup**: migrated ImuChart, PpgChart, GpuChart, and BandChart to use the shared `animatedCanvas` Svelte action from `use-canvas.ts`, eliminating duplicated ResizeObserver + requestAnimationFrame + DPR scaling boilerplate. EegChart is intentionally left as-is due to its spectrogram tape + MutationObserver + frame-skip complexity.

- **HuggingFace cache path consolidation**: added `hf_cache_root()`, `hf_model_dir()`, and `hf_ensure_dirs()` helpers to `skill-data::util`. Replaced the manual env-var resolution in `skill-exg::resolve_hf_weights` and the duplicated `Cache::from_env().path()` + folder construction pattern in both `skill-exg` and `skill-llm::catalog`. Removed the now-unused `dirs` crate dependency from `skill-exg`.

- **Titlebar store factory**: added `createTitlebarState()` and `createTitlebarCallbacks()` in `titlebar-state.svelte.ts`. Refactored `chat-titlebar`, `history-titlebar`, and `label-titlebar` stores to use the shared factory instead of raw `$state()` calls.
