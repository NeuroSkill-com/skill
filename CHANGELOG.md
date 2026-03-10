# Changelog

All notable changes to NeuroSkill™ are documented here.

---

## [0.0.11] — 2026-03-10

### LLM / Chat

- **LLM engine** — full on-device inference via `llama-cpp-4` (llama.cpp
  bindings). Runs text and multimodal (vision) models locally with no cloud
  dependency
- **Model catalog** (`llm_catalog.json`) — curated list of GGUF models
  (Qwen3.5 4B/27B, Llama-3.2-Vision, Gemma3, etc.) with per-entry metadata:
  repo, filename, quantisation, size, family description, tags, recommended
  flag. Bundled into the app at compile time
- **Tauri commands**: `get_llm_catalog`, `set_llm_active_model`,
  `set_llm_active_mmproj`, `download_llm_model`, `cancel_llm_download`,
  `delete_llm_model`, `refresh_llm_catalog`, `get_llm_logs`,
  `start_llm_server`, `stop_llm_server`, `get_llm_server_status`,
  `open_chat_window`
- **HTTP inference server** (`axum` router) — OpenAI-compatible endpoints
  (`/v1/chat/completions`, `/v1/completions`, `/v1/embeddings`) served
  locally so third-party tools can connect to the on-device model
- **Vision / multimodal** — image inputs decoded from data-URL or base64
  and fed through a clip mmproj; `autoload_mmproj` setting automatically
  selects the best downloaded projector for the active model
- **Thinking-model support** — forced `</think>` injection after a budget
  cap; orphaned tail tokens are discarded (decoded into KV cache for
  coherence, suppressed from output) until the next clean line boundary
- **File upload** in chat — images attachable to messages; previewed in
  the UI before sending
- **Markdown renderer** (`MarkdownRenderer.svelte`) — renders streamed
  assistant output with code blocks, tables, and inline formatting
- **Chat window** (`src/routes/chat/+page.svelte`) — full chat UI with
  message history, streaming tokens, stop button, model/mmproj selectors,
  generation parameter controls
- **Global chat shortcut** — configurable keyboard shortcut (stored in
  settings) focuses the existing chat window or opens a new one
- **i18n** — `llm.*` keys added to all five language files (en, de, fr,
  he, uk)

### Build / CI

- **Bypass Tauri's built-in signing pipeline** in both `release.yml` and
  `pr-build.yml` — Tauri's `create-dmg` subprocess crashes with `SIGILL`
  on macOS 26 (hdiutil API change); replaced with explicit steps:
  1. `npx tauri build --bundles app --no-sign` — compile only
  2. `codesign` — deep-sign with `--options runtime` + `--entitlements`
  3. `xcrun notarytool submit … --wait` — notarize
  4. `xcrun stapler staple` — staple ticket to bundle
  5. Recreate `.app.tar.gz` from the signed bundle, then
     `npx tauri signer sign` — re-sign the updater artifact with Ed25519
- `release.sh` — minor fix to `TAURI_TARGET` default propagation

---

## [0.0.9] — 2026-03-10

### Dependencies

- Migrated `llama-cpp-4` and `llama-cpp-sys-4` to local path via
  `[patch.crates-io]` (`../../../llama-cpp-rs/llama-cpp-4` and
  `../../../llama-cpp-rs/llama-cpp-sys-4`) — ensures the SIGILL fix
  (correct `CMAKE_OSX_ARCHITECTURES` / `CMAKE_CROSSCOMPILING` for Apple
  cross-arch builds) is always active; both the `llm` feature and neutts's
  backbone resolve to the same local crate, preserving the `links = "llama"`
  deduplication

### Build / CI

- macOS builds now target `aarch64-apple-darwin` (arm64) only — x86_64
  is no longer compiled
  - `tauri:build:mac` npm script passes `--target aarch64-apple-darwin`
  - `release.sh` defaults `TAURI_TARGET` to `aarch64-apple-darwin` (still
    overridable via env var for universal or x86_64 builds)
  - `build-espeak-static.sh` defaults `CMAKE_OSX_ARCHITECTURES` to `arm64`
    instead of the host architecture (still overridable via `ESPEAK_ARCHS`)
  - `.cargo/config.toml` sets `[build] target = "aarch64-apple-darwin"` so
    plain `cargo build` / `cargo check` / `npx tauri build` all default to
    arm64 without requiring an explicit `--target` flag
  - `ci.yml` Linux `cargo check` / `cargo clippy` steps now pass
    `--target x86_64-unknown-linux-gnu` to override the config.toml default;
    espeak build step passes `ESPEAK_ARCHS=x86_64` explicitly
  - `pr-build.yml` and `release.yml` were already correct (`--target
    aarch64-apple-darwin`, `ESPEAK_ARCHS=arm64`)
- Fixed SIGILL crash after successful compile on macOS 26.3 in both local
  and CI builds; root cause traced via lldb + macOS crash report:
  - Tauri's bundled `create-dmg` script spawns `bundle_dmg.sh` as a child
    process which fails on macOS 26 (hdiutil API change); Node.js propagates
    the child's fatal exit as `process.kill(pid, SIGILL)` via
    `ProcessWrap::OnExit` → promise rejection chain
  - Local dev (`tauri:build:mac`): added `--no-sign` — no certificate on dev
    machines, codesign would have failed at the same stage
  - CI (`release.yml`, `pr-build.yml`): replaced `--bundles app,dmg` with
    `--bundles app`; added an explicit "Create DMG" step that uses `hdiutil`
    directly, stamps the version badge, then signs and notarizes — identical
    end result with no dependency on Tauri's create-dmg script
- Fixed pre-commit hook failing on macOS when CUDA Toolkit is absent
  - `cargo clippy --all-features` activated `llm-cuda` and `llm-vulkan`,
    causing `llama-cpp-sys` to pass `-DGGML_CUDA=ON -DGGML_VULKAN=ON` to
    CMake, which hard-errors if no CUDA Toolkit is found
  - Hook now selects platform-appropriate features: `--features llm-metal`
    on macOS, default features on Linux/Windows — CUDA/Vulkan features are
    never activated where their native toolkits are unavailable

---

## [0.0.6] — 2026-03-06

### Do Not Disturb / Focus Mode

- Replaced hand-rolled ObjC FFI + XPC DND implementation with the
  [`macos-focus`](https://crates.io/crates/macos-focus) crate — pure Rust,
  no private frameworks, no special entitlements
- DND now works reliably on macOS 12–15; the XPC path that consistently
  returned "operation failed" errors has been removed
- Added **Focus mode picker** in Settings → Goals: choose any Focus mode
  configured in System Settings (Do Not Disturb, Work, Personal, Sleep,
  Driving, …) rather than always activating Do Not Disturb
- `focus_mode_identifier` persisted in settings; defaults to Do Not Disturb
  for backwards compatibility with existing configs
- Added `list_focus_modes` Tauri command backed by
  `FocusManager::available_modes()`; falls back to the full first-party mode
  list if `ModeConfigurations.json` is unreadable
- Added TODO stubs for Linux (D-Bus / xdg-portal) and Windows
  (WinRT / IQuietHoursSettings) DND support

### Quit Dialog

- macOS quit confirmation dialog now uses `NSAlert` via `objc2-app-kit`
  dispatched through `dispatch2::DispatchQueue::main().exec_sync()`,
  eliminating the `CFUserNotificationDisplayAlert: called from main
  application thread` log warning that `rfd` triggered

### Bug fixes and warnings

- Fixed `CFStringCreateWithCString` / `CFRelease` clashing `extern "C"`
  signatures between `dnd.rs` and `gpu_stats.rs`
- Removed three unnecessary `unsafe {}` blocks around safe `iimp()` closure
  calls in the (now-deleted) ObjC FFI path
- Removed unused `vm_deallocate` extern declaration in `gpu_stats.rs`
- Removed unnecessary `unsafe {}` block wrapping safe `NSAlert` method calls
- Fixed unescaped ASCII `"` inside German DND strings in `de.ts` that caused
  587 cascading TypeScript parse errors
- Replaced `map_or(false, |v| v == 1)` with `== Some(1)` in `gpu_stats.rs`
- Replaced manual `div_ceil` closure with `u64::div_ceil` in `job_queue.rs`
- Replaced `&&` range assertions with `.contains()` in `ppg_analysis.rs`
- Replaced `vec![…]` with array literals in test push calls
- Replaced `for ch in 0..N` index loops with `enumerate()` iterators in
  `eeg_bands.rs` and `eeg_filter.rs`
- Moved constant-value `assert!` calls into `const { }` blocks in
  `constants.rs`
- Fixed doc comment continuation indent warnings in `gpu_stats.rs`

### i18n

- Added `dnd.focusMode`, `dnd.focusModeDesc`, `dnd.focusModeLoading` keys
  to all five language files (en, de, fr, he, uk)

---

## [0.0.3] — 2026-03-06

- Added NeuTTS engine support alongside KittenTTS, with seamless switching between engines
- TTS engine switching now works reliably in both directions
- Graceful shutdown for NeuTTS on engine change or app exit
- TTS caching and quality improvements
- UI updates for TTS tab including progress/error state display
- Fixed TypeScript type for TTS error phase
- Added translations
- Better updater configuration

---

## [0.0.2] — 2026-03-04

- Improved EEG, Band, and GPU charts
- UI polish for main page
- Dependency and version bumps

---

## [0.0.1] — 2026-03-01

- Initial release
- CI/CD pipeline with signing, notarization, and auto-updater
- EEG visualisation, metrics, and GPU monitoring
- TTS foundation with KittenTTS
