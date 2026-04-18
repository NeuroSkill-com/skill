### Bug Fixes

- Fix daemon binary missing from DMG when prebuilt llama retry triggers on CI. The failed link left partial build state that cargo considered "fresh", so the retry silently skipped rebuilding the daemon.

### Build

- Add `mtmd` (multimodal) to llama-cpp-4 target-specific dependency features in skill-llm, ensuring prebuilt archives include multimodal symbols.
- Copy Frameworks directory (dynamic llama dylibs) into daemon `.app` bundle during assembly.
- Fail app bundle assembly if daemon binary is missing instead of silently continuing.
- Clean daemon build artifacts during prebuilt llama retry to force a full re-link.
