# Changelog

All notable changes to NeuroSkill™ are documented here.
Pending changes live as fragments in [`changes/unreleased/`](changes/unreleased/).
Past releases are archived in [`changes/releases/`](changes/releases/).

---

## [Unreleased]

## [0.0.61] — 2026-03-24

### CLI

- **Detect competing cargo processes in bump**: `npm run bump` now checks for other running `cargo` processes before starting clippy/test preflight checks and warns that they may cause hangs due to the global cargo package-cache lock. The user is prompted to continue or abort.

## [0.0.60] — 2026-03-24

### Performance

- **LLM E2E reuses clippy build cache**: The `llm-e2e` CI job now runs after `rust-check` (`needs: rust-check`) and shares its Cargo build cache via `Swatinem/rust-cache` (`save-if: false`). This turns the E2E compilation into a cheap incremental build instead of a full rebuild (~3–5 min saved). The linker (mold + clang), target triple, and `CMAKE_*_COMPILER_LAUNCHER: sccache` env vars are aligned with `rust-check` to maximise cache hits. The Vulkan SDK (~200 MB) now uses the same `actions/cache` key as `rust-check`, skipping download on cache hit. System deps (including mold + clang) are merged into one cached `awalsh128/cache-apt-pkgs-action` step, eliminating a separate `apt-get update` round-trip.
- **Halve LLM E2E test runtime (67s → 33s)**: Reduced `ctx_size` from 4096 to 2048 (test prompts are < 600 tokens). Reduced tool-chat `max_tokens` from 128 to 64 — the model was wasting inference echoing full JSON results (117-128 tokens) when it only needs ~25 tokens for the tool call and a short summary. Added `[profile.test.package.llama-cpp-sys-4]` and `llama-cpp-4` with `opt-level = 3` so the C++ inference engine runs optimized even in test builds, boosting tok/s by ~20%.
- **LLM E2E is now opt-in**: The `llm-e2e` job no longer runs automatically on push. It is triggered only via manual `workflow_dispatch` with the `run_llm_e2e` checkbox enabled. The Discord notification shows ⏭️ when skipped.

### Bugfixes

- **Regenerate Cargo.lock during bump**: `npm run bump` now runs `cargo generate-lockfile` after updating version in `src-tauri/Cargo.toml`, preventing CI `--locked` build failures due to stale lockfile.

- **Add missing safety comment for unsafe block**: Added `// SAFETY:` comment on the Linux `RLIMIT_STACK` unsafe block in `main.rs` to satisfy `clippy::undocumented_unsafe_blocks` (Rust 1.94).

- **Fix i18n test import shadowing**: The `extractKeysFromDir` import from `i18n-utils.ts` was shadowing the local test helper of the same name, causing 8 pre-existing key-sync test failures. Renamed to `extractKeysWithValues` in the test import.

### Build

- **CI/Release speed improvements**: Estimated 7-12 min savings per CI run via multiple optimizations:
  - Removed redundant `cargo check` step — `clippy` is a strict superset and already compiles everything.
  - Merged duplicate `cargo-audit` and `audit` jobs into a single security audit job.
  - Moved audit to run only on main/develop pushes (advisory, not PR-blocking).
  - Added concurrency groups to `ci.yml` to cancel superseded runs on the same branch/PR.
  - Replaced manual `--workspace -p crate1 -p crate2 ...` clippy invocations with `--workspace --exclude skill`.
  - Switched Linux CI from manual `actions/cache` to `Swatinem/rust-cache` for smarter per-crate invalidation (matching release workflows).
  - Cached Vulkan SDK on Linux CI (previously downloaded ~200 MB on every run).
  - Added `mold` fast linker + `clang` to Linux CI (previously only in release-linux).
  - Added `fetch-depth: 1` to CI jobs that don't need full git history.
  - Added `--locked` flag to all release cargo build commands for reproducible builds.
  - Added `--timings` flag and cargo-timings artifact upload to macOS and Linux release workflows (previously Windows only) for build profiling.
  - Removed redundant `cargo check` on Windows CI — `clippy` already covers it.
  - Fixed Discord notification to show "skipped" emoji for audit when it doesn't run on PRs.
  - Added `CMAKE_C_COMPILER_LAUNCHER` / `CMAKE_CXX_COMPILER_LAUNCHER` sccache integration to macOS, Linux CI, and preview builds (previously Windows release only) so cmake-based -sys crate compilations (llama-cpp-sys, espeak-ng) are cached across runs.

- **Commit Cargo.lock**: Removed `Cargo.lock` from `.gitignore` and committed it so that `cargo clippy --locked` and CI builds succeed without needing to regenerate the lockfile.

### i18n

- **Untranslated value detection**: Added a vitest check (`i18n untranslated value detection`) that fails when any non-English locale contains values identical to English that are not in the exemption list. The exemption logic (brand names, technical acronyms, formulas, academic citations, etc.) is now shared between `i18n-utils.ts`, the `audit-i18n.ts` script, and the test suite. A new `check:i18n` npm script runs the audit with `--check` for CI gating.

- **Translate all untranslated strings**: Translated 388 strings across 4 locales (de, fr, he, uk) that were still in English. Covers common UI (errors, dismiss, zoom reset, goal reached), supported devices and setup instructions, device API settings, API authentication, screenshot/OCR pipeline labels, screen recording permissions, history streaks, LLM tool settings, onboarding, and search screenshot tab. Added brand/product names and cross-language cognates to the exemption list.

## [0.0.59] — 2026-03-24

### Features

- **Windows NSIS installer: optional runtime components**: The installer now has a Components page with smart auto-detection. Optional sections for Vulkan Runtime (GPU acceleration), VC++ 2015-2022 Redistributable, WebView2 Runtime (required for UI on older Windows 10), and GPU TDR timeout increase (prevents driver resets during long LLM inference). Each component auto-selects only when its prerequisite is missing.
- **Windows NSIS installer: kill running instance**: The installer detects if the app is running before upgrading and offers to close it (WM_CLOSE then taskkill). The uninstaller also force-kills the app before removing files.
- **Windows NSIS installer: long path support**: Enables the `LongPathsEnabled` registry key so HuggingFace model cache paths exceeding 260 characters work correctly.
- **Windows NSIS installer: firewall rule**: Adds a Windows Firewall exception for the local LLM/WebSocket server, preventing the "allow access?" popup on first launch. Cleaned up on uninstall.
- **Windows NSIS installer: launch at login**: A "Launch at login" component is selected by default. Writes the same `HKCU\...\Run\skill` registry key used by the in-app autostart setting. Users can uncheck it during install or toggle it later in Settings.
- **Windows NSIS installer: clean uninstall**: The uninstaller now removes the autostart registry entry (`HKCU\...\Run\skill`), the firewall rule, and optionally offers to delete the user data folder (`%LOCALAPPDATA%\NeuroSkill`) including settings, sessions, and downloaded models.

### Refactor

- **Adopt `anyhow` across all workspace crates**: Replaced `Result<T, String>` error handling with `anyhow::Result<T>` throughout the crate layer. `map_err(|e| format!(...))` chains are now `.context()` / `.with_context()`, and `Err(format!(...))` becomes `anyhow::bail!(...)`. The Tauri command boundary in `src-tauri/` converts back to `String` via `.map_err(|e| e.to_string())`. `skill-jobs` retains `Result<Value, String>` for stored/cloned job results. `skill-headless` retains its `HeadlessError` enum. A migration script (`scripts/adopt_anyhow.py`) automates the bulk of the conversion.

## [0.0.58] — 2026-03-24

### Features

- **Auto-launch after install on Windows**: The NSIS finish page now shows a "Launch NeuroSkill™" checkbox (checked by default). The app is launched as the current user (not elevated) via the Explorer shell trick, so per-user paths, tray registration, and autostart work correctly.

- **Extracted `search-logic.ts` module**: Pulled pure business logic (mode normalization, UMAP label enrichment, time helpers, analysis chips) out of the 2,169-line search page into a testable TypeScript module with 9 unit tests.

- **Extracted `compare-logic.ts` module**: Pulled timeline helpers, session range selection, pointer-to-UTC conversion, and date formatting out of the 1,922-line compare page into a testable module with 14 unit tests.

- **Added DSP pipeline integration tests**: New `dsp_pipeline_test.rs` for `skill-eeg` covering band analysis at multiple sample rates, beta/alpha dominance detection, quality monitoring, and reset behaviour (5 tests).

- **Added `skill-headless` tests**: New `intercept_tests.rs` covering `InterceptStore` push/snapshot/clear, serialization round-trip, and default state (5 tests).

- **Added `skill-screenshots` tests**: New `context_tests.rs` covering mock context, fastembed model resolution, and `ActiveWindowInfo` defaults (5 tests).

- **Added `skill-jobs` integration tests**: New `queue_tests.rs` covering submit-and-poll, sequential execution ordering, error propagation, queue positioning, and unknown-job handling (5 tests).

- **Added `skill-data` tests**: New `hooks_log_tests.rs` (4 tests) and `screenshot_store_tests.rs` (4 tests) covering database creation, insert, count, and pagination.

### Bugfixes

- **Fixed 2 broken tests in `src-tauri/src/constants.rs`**: The `embedding_overlap_samples_correct` and `embedding_hop_samples_correct` tests were asserting stale values (640) after `EMBEDDING_OVERLAP_SECS` changed from 2.5 to 0.0. Tests now derive expected values from the actual constants.

- **Fixed compilation error in `skill-headless`**: Two `eval_fire()` calls passed `reply` by value instead of by reference.

- **Fix ™ symbol mangled in Windows NSIS installer**: The `.nsi` script was written as UTF-8 without BOM. NSIS with `Unicode True` requires a BOM to detect UTF-8; without it, NSIS falls back to the system ANSI codepage and corrupts non-ASCII characters like ™ in the product display name, version info, registry entries, and shortcuts. Changed to UTF-8 with BOM.

- **Fix Windows SxS "side-by-side configuration incorrect" error**: Added `CMAKE_MSVC_RUNTIME_LIBRARY = "MultiThreaded"` to the workspace `.cargo/config.toml` `[env]` section so that cmake-based C/C++ dependencies (llama-cpp-sys, espeak-ng) use static CRT (`/MT`) matching Rust's `+crt-static` target feature. Previously this env var was only set in CI, causing local Windows builds to produce a CRT mismatch that triggered the SxS error on machines without the Visual C++ Redistributable.

### Refactor

- **Eliminated all clippy warnings across the workspace**: Resolved every warning from `cargo clippy --workspace` — from 500+ warnings to zero.

  - Converted 21 `match` → `let...else` patterns across 10 files (api.rs, screenshot_cmds.rs, ws_commands, device_scanner, session_runner, session_connect, eeg_embeddings, label_cmds, skill-skills/sync, skill-llm/actor).
  - Replaced 17 `lock().expect("lock poisoned")` calls with `lock_or_recover()` across skill-llm (handlers, logging, state, tool_orchestration) and src-tauri/api.
  - Applied `cargo clippy --fix` auto-fixes: redundant closures, method call simplifications, let-else conversions.
  - Added `// SAFETY:` comments to remaining undocumented `unsafe` blocks (skill_log, quit, active_window, window_cmds).
  - Added `#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]` to `build.rs` (build-time panics are standard practice).
  - Added `#![allow(clippy::unwrap_used)]` to `image_encode_bench.rs` (benchmark binary).
  - Disabled `needless_pass_by_value` lint (280 false positives from Tauri `#[command]` handlers).
  - Disabled `expect_used` lint (50 legitimate uses in thread spawning, NonZero constructors, Tauri app builder — all unrecoverable).

- **Split `skill-tools/src/parse.rs` into modules**: The 2,441-line monolith is now split into 7 focused sub-modules (`types`, `coerce`, `validate`, `extract`, `strip`, `inject`, `json_scan`) while preserving the full public API. All 161 tests continue to pass.

- **Workspace-wide lint configuration**: Added `[workspace.lints]` in root `Cargo.toml` with consistent Clippy rules across all 22 crates — `unwrap_used`, `expect_used`, `undocumented_unsafe_blocks`, `needless_pass_by_value`, and more. All crates inherit via `[lints] workspace = true`.

- **Eliminated `unwrap()` in library code**: Replaced the sole remaining `unwrap()` in production code (`skill-commands/src/graph.rs`) with safe `let-else`. All other `unwrap()` calls were already in tests/examples/benchmarks.

- **Hardened `InterceptStore` lock handling**: Replaced `lock().expect("lock poisoned")` with graceful `if let Ok(guard)` pattern in `skill-headless` — poisoned locks now degrade gracefully instead of panicking.

### Build

- **Bust Windows CI caches**: Bumped Cargo and sccache cache keys (`windows-release-x86_64-msvc-v2`, `SCCACHE_GHA_VERSION=2`) to invalidate stale cmake artifacts that were compiled with dynamic CRT (`/MD`).
- **Verify static CRT in Windows CI**: Added a post-compile `dumpbin /dependents` check that fails the build if `vcruntime140.dll`, `ucrtbase.dll`, or `msvcp*.dll` appear as dependencies, catching dynamic CRT leaks before packaging.

- **Added `cargo audit` to CI**: New `cargo-audit` job in the CI pipeline scans for known dependency vulnerabilities on every push and PR.

### Docs

- **Added SAFETY comments to all `unsafe` blocks**: Documented invariants for every `unsafe` block in `skill-vision` (FFI OCR), `skill-gpu` (IOKit/sysctl), `skill-llm` (llama.cpp backend), and `skill-screenshots` (CoreFoundation/AppKit FFI).

## [0.0.57] — 2026-03-24

### Bugfixes

- **Fix skill-headless build with wry 0.54**: Enable `rwh_06` feature on `tao` so `Window` implements `HasWindowHandle` required by wry 0.54.

- **Fix mDNS discovery in smoke-test.sh**: Keep a single `dns-sd -B` process running continuously and poll its output every second, instead of spawning and killing a new process every 3 seconds (which could miss service announcements).

- **Fix smoke-test false-positive mDNS match**: The `dns-sd -B` header line already contains "skill", so the grep matched immediately before any service was actually discovered. Changed pattern to `Add.*_skill._tcp` which only matches a real service registration event.

- **Smoke test mDNS retry**: Moved mDNS discovery from `smoke-test.sh` (bash `dns-sd`) into `test.ts` (bonjour-service). Discovery now retries indefinitely with a 3-second backoff until the Skill server appears or the user presses Ctrl-C, fixing the "could not resolve port" failure on slow startups.

- **Smoke test port discovery**: Fixed `smoke-test.sh` failing because `test.ts` tried its own 5-second mDNS browse which raced and failed. The script now resolves the port via `dns-sd -L` after the browse succeeds and passes it explicitly to `test.ts`, eliminating the double-discovery race condition.

- **Fix smoke test on macOS**: Replace GNU `timeout` command (not available by default on macOS) with a portable Perl-based timeout in `smoke-test.sh`.

- **Fix smoke-test unbound variable**: `scripts/smoke-test.sh` failed with `unbound variable` when invoked without arguments due to `set -u` and bare `${*}`. Changed to `${*:-}` to default to empty string.

### CLI

- **Move smoke-test to scripts/ and add npm script**: Moved `smoke-test.sh` into `scripts/` and added `npm run test:smoke` shortcut.

## [0.0.56] — 2026-03-24

### Features

- **Smoke test script**: Added `smoke-test.sh` and `npm run test:smoke` to launch the app and run `test.ts` end-to-end inside a tmux session, waiting for mDNS service discovery before starting tests.

- **Full WS command test coverage**: Added smoke tests in `test.ts` for all previously untested WebSocket commands: `session_metrics`, calibration CRUD (`get_calibration`, `create_calibration`, `update_calibration`, `delete_calibration`), `sleep_schedule` / `sleep_schedule_set`, health commands (`health_summary`, `health_metric_types`, `health_query`, `health_sync`), and extended LLM management commands (`llm_downloads`, `llm_refresh_catalog`, `llm_hardware_fit`, `llm_select_model`, `llm_select_mmproj`, `llm_pause_download`, `llm_resume_download`, `llm_set_autoload_mmproj`, `llm_add_model`). All 58 dispatch table commands now have corresponding tests.

### Performance

- **Faster Windows release builds**: Added `CMAKE_C_COMPILER_LAUNCHER` and `CMAKE_CXX_COMPILER_LAUNCHER` env vars so sccache caches C/C++ compilations from cmake-based -sys crates (llama-cpp-sys-4, etc.), not just rustc. Added `[profile.release]` with `lto = "thin"` and `codegen-units = 8` for faster linking. Moved frontend build before SDK installs to reduce idle time. Combined NSIS and Vulkan SDK installs into a single parallel step. Added sccache GHA cache versioning and `--timings` cargo flag with artifact upload for build profiling.

### Bugfixes

- **Fix Windows CI zip assembly loading**: Load `System.IO.Compression` assembly explicitly before `System.IO.Compression.FileSystem` in the "Sign installer + create updater artifacts" step. On some PowerShell runtimes (notably PowerShell Core on GitHub Actions), loading only `FileSystem` does not implicitly load the base `System.IO.Compression` assembly, causing `Unable to find type [System.IO.Compression.ZipArchiveMode]` errors during NSIS updater zip creation.

## [0.0.55] — 2026-03-24

### Bugfixes

- **Fix Windows "side-by-side configuration" launch error**: Statically link the MSVC C/C++ runtime (`+crt-static`) into the Windows binary so the app no longer requires the Visual C++ Redistributable to be pre-installed. The espeak-ng static build and CI workflow also set `CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded` to ensure all C/C++ dependencies use the matching static CRT.

- **Fix Windows auto-update "unsupported compression method"**: Replaced PowerShell `Compress-Archive` with .NET `ZipFile` using explicit Deflate compression (method 8) when creating updater `.nsis.zip` archives. `Compress-Archive` on newer Windows versions uses Deflate64 (method 9), which the Tauri updater's zip crate does not support.

## [0.0.54] — 2026-03-23

### Bugfixes

- **Fix clean:rust ENOTEMPTY on large target dirs**: Added retry options and `rm -rf` fallback to `scripts/clean-rust.js` so the Rust build artifact cleanup no longer fails with ENOTEMPTY on very large directory trees.

- **Windows app manifest for BLE access**: Added a custom Windows application manifest (`manifest.xml`) declaring Windows 10/11 compatibility via `supportedOS` and `maxversiontested`. Without this, Windows 11 may deny WinRT Bluetooth Low Energy API access to unpackaged desktop apps. Also includes Common Controls v6 and per-monitor DPI awareness v2.

- **Windows 11 Bluetooth permissions guidance**: Updated BLE error messages and the "Bluetooth is off" UI state to include Windows 11-specific instructions (Settings → Privacy & Security → Bluetooth). The "Open Settings" button now opens both the Bluetooth devices page and the Bluetooth privacy page on Windows. Added a Windows-specific adapter state check in `bluetooth_ok()` to detect powered-off adapters. Updated all locales (en, de, fr, he, uk).

### Build

- **Stop tracking Cargo.lock**: Removed all `Cargo.lock` files from version control and added `Cargo.lock` to `.gitignore`.

- **Add libgbm-dev to Linux CI**: Added missing `libgbm-dev` package to CI and release workflows to fix linker error (`library not found: gbm`).

## [0.0.53] — 2026-03-22

### Features

- **Auto-pair first discovered device**: When the paired devices list is empty (fresh install or all devices unpaired), the first discovered device is automatically added to paired devices and auto-connected. This provides a seamless first-run experience without requiring manual pairing. Only triggers when no paired devices exist.

- **Cortex WebSocket connection status indicator**: The Emotiv Cortex scanner backend now tracks and emits its WebSocket connection state (`disconnected`, `connecting`, `connected`) to the frontend in real time.

- **Llama XML tool-call parsing**: `extract_tool_calls()` now recognises the `<function=name><parameter=key>value</parameter></function>` XML format emitted by Llama-family models. Both `<parameter>` tag pairs and inline JSON bodies are supported. Stripping and streaming partial-tag handling are also included.

- **Tool-call self-healing**: When the LLM emits a garbled or malformed tool call that cannot be parsed, the orchestrator now detects the failed attempt, injects a corrective message containing the raw output, and asks the model to re-emit in the correct format. Up to 2 retry attempts are made before falling back to normal output. This significantly improves reliability with smaller local models.

### Bugfixes

- **Emotiv credentials not persisting across restarts**: `DeviceApiConfig` fields used `#[serde(skip_serializing)]` to keep secrets out of the JSON settings file, but this also caused `get_device_api_config` to return empty credentials to the frontend via Tauri IPC. The command now returns a `serde_json::Value` that bypasses the skip, so stored keychain credentials are correctly displayed after restart and are no longer accidentally overwritten with empty values on subsequent saves.

- **Keychain credentials not persisted on any platform**: The `keyring` crate v3.x requires explicit platform backend features. Without them, no credential store was compiled in and `set_password`/`get_password` silently failed, causing Emotiv (and IDUN / API token) credentials to be lost on restart. Enabled OS-specific backends via target dependencies (`apple-native` on macOS, `windows-native` on Windows, `linux-native-sync-persistent` + `crypto-rust` on Linux). Also improved error logging so keychain failures are no longer silently swallowed.

- **KittenTTS: re-open audio device on every utterance**: The KittenTTS backend opened the system audio output once at startup and reused the same stream for all subsequent speech. If the device was unplugged, switched, or became unavailable (e.g. Bluetooth disconnect, USB DAC removal), playback failed with "The requested device is no longer available." The worker now re-opens the default audio device before each utterance, matching the NeuTTS backend behaviour. This is cheap (~1 ms) relative to synthesis time and ensures the current default device is always used.

- **Screenshots not captured on Windows and Linux**: The `screenshots` feature (which enables the `xcap` screen-capture backend) was missing from the default Cargo features and from all CI/release build workflows. On macOS this was invisible because capture uses the `screencapture` CLI tool directly, but on Windows and Linux the capture function silently returned `None`. Added `screenshots` to the default feature set and to all build/CI workflows for Windows and Linux.

- **Fix WebView2 window class unregistration error on Windows**: Stopped dropping the WebView early during Close command handling. The `Chrome_WidgetWin_0` class unregistration race (Win32 error 1412) occurred because the WebView was destroyed while the event loop and parent window were still alive. The WebView is now dropped naturally when `run_return` finishes, allowing Chromium's internal child windows to clean up before class unregistration.

- **Fix Windows Vulkan STATUS_ACCESS_VIOLATION crash**: The crash was in the Vulkan validation layer (`VK_LAYER_KHRONOS_validation`) loaded by the Vulkan SDK during llama.cpp model loading on the `llm-actor` thread. In debug builds, the validation layer's `ErrorObject` constructor crashes inside `vkEnumeratePhysicalDevices` on certain Windows GPU drivers. Now disable the validation layer at app startup via `VK_LOADER_LAYERS_DISABLE` and `VK_INSTANCE_LAYERS` env vars (affects llama.cpp / ggml-vulkan) plus `WGPU_VALIDATION=0` (affects wgpu/cubecl). Added a Windows vectored exception handler that prints faulting address, thread name, and full backtrace for any future access violations.

- **Fix window flickering on Windows**: The screenshot capture worker iterated through all non-minimized windows calling `PrintWindow` (via xcap `capture_image()`) on each one until it got a result. `PrintWindow` sends a `WM_PRINT` message forcing each window to repaint, causing constant visible flickering across all open windows every few seconds. Now on Windows only the single foreground window is captured using xcap's `is_focused()` check, with a monitor-capture fallback if that fails.

### Build

- **`clean:rust` script fails on Windows**: Replaced Unix `rm -rf` with a cross-platform Node script (`scripts/clean-rust.js`) that works on Windows, macOS, and Linux. The script now reports the size of build artifacts and how much disk space was freed.

### UI

- **Colored status dots for Emotiv Cortex**: The Scanner Backends section now shows a green dot when connected to the Cortex WebSocket, a blinking yellow dot while connecting, and a red dot when disconnected — replacing the previous static text badges.

- **EMOTIV Launcher connection reminder**: Devices discovered via the Cortex transport (EMOTIV headsets) now show a hint reminding the user to confirm the connection in the EMOTIV Launcher app.

- **Native shortcut rendering in tray menu**: Keyboard shortcuts in the system tray context menu now use the native accelerator parameter on `MenuItem` instead of being appended to the label text. The OS renders them right-aligned in the platform-native style (e.g. ⌘⇧L on macOS, Ctrl+Shift+L on Linux/Windows).

### LLM

- **`detect_garbled_tool_call()`**: New public function that identifies malformed tool-call attempts in assistant output (broken `[TOOL_CALL]` blocks, incomplete XML `<function=` tags, or unbalanced JSON with tool-call keys).
- **`build_self_healing_message()`**: New public helper that constructs the corrective user message for the retry loop.

### i18n

- **New key `settings.scanner.cortexConnecting`**: Added "Connecting…" translations in EN, DE, FR, HE, and UK.

- **New key `settings.emotivLauncherHint`**: Added translations in EN, DE, FR, HE, and UK for the EMOTIV Launcher reminder.

- **Remove hardcoded Muse/OpenBCI two-device-only phrases**: Updated all i18n strings (en, de, fr, he, uk) that mentioned only "Muse or OpenBCI" to use device-agnostic wording like "your BCI device", "any supported BCI device", or "another BCI device". The app now supports Muse, MW75 Neuro, OpenBCI boards, Emotiv, IDUN Guardian, and more — user-facing text no longer implies only two brands are supported. Search keywords in the command palette were expanded to include all supported device names.

## [0.0.52] — 2026-03-21

### Features

- **System keychain for secrets**: API tokens and device credentials (`api_token`, Emotiv client ID/secret, IDUN API token) are now stored in the OS credential store (macOS Keychain, Linux Secret Service, Windows Credential Manager) instead of plaintext in `settings.json`. Secrets survive app reinstalls and build updates. Existing plaintext values are automatically migrated to the keychain on first launch and stripped from the JSON file.

- **Skills sync on launch**: Added a "Sync on launch" toggle in the Skills auto-refresh settings. When enabled, the app forces a community skills sync every time it starts, regardless of the periodic refresh interval. The periodic schedule continues to run normally afterwards.

- **Flamegraph profiling script**: Added `npm run tauri:flamegraph` to profile the Tauri app with `flamegraph` and produce an interactive SVG. Works on Linux (perf), macOS (dtrace), and Windows (dtrace/xperf). Supports optional duration argument (e.g. `npm run tauri:flamegraph -- 60`) and `--release` flag (default: dev profile to match `tauri dev`).

### Performance

- **Session listing 10x faster**: Replaced `serde_json::Value` (BTreeMap-backed) with a typed `SessionJsonMeta` struct for parsing session JSON sidecars, eliminating expensive BTreeMap construction and recursive drop overhead.
- **Metrics timestamp lookup O(1) instead of O(n)**: `read_metrics_csv_time_range` now reads only the first and last 4 KB of the file (via seek) instead of parsing every CSV record. For a 100 MB metrics file this reduces I/O from ~100 MB to ~8 KB.
- **Skip redundant timestamp patching**: `patch_session_timestamps` now skips sessions that already have valid start/end timestamps from their JSON sidecar, avoiding unnecessary metrics file reads on every session listing.
- **ZUNA encoder loads ~60% faster**: Encoder-only weight filter skips all decoder tensors during deserialization (halves bf16-to-f32 conversion work and memory). Weight data is moved instead of cloned via new `WeightMap::take()`. HashMap pre-sized from safetensors tensor count.
- **LUNA encoder loads faster**: Updated luna-rs to 0.0.3 with the same zero-copy weight loading, encoder-only filter, and HashMap pre-sizing optimizations.

### Bugfixes

- **Flamegraph permission errors on macOS**: Separated build (normal user) from profiling (`sudo flamegraph`) so dtrace runs as root and owns its trace files. Fixes "Trace file already exists" (exit 42) and "Permission denied" errors caused by root-owned artifacts from previous runs.
- **Flamegraph builds dev profile by default**: Now matches `tauri dev` behavior. Pass `--release` explicitly for optimized profiling.

- **Flamegraph script launching stale binaries**: Fixed `tauri:flamegraph` profiling old binaries instead of freshly-built ones. Added `-p skill` to target only the skill package, moved build cwd to workspace root, added sccache/mold detection to match `tauri-build.js` environment (prevents fingerprint mismatches), made `forceRemove` fail hard instead of silently continuing, and added post-build mtime verification to catch stale binaries before profiling.

- **Skills sync discovers all community skills**: The skill discovery algorithm stopped recursing into subdirectories when the repository root contained a valid `SKILL.md` with a `description` in frontmatter, causing only one skill (the index) to be loaded. Added support for an `index: true` frontmatter flag that marks a `SKILL.md` as an index file — the skill is loaded but the scanner continues recursing into child directories. The community skills repo root `SKILL.md` now uses this flag. Also fixed Phase 2 to skip re-processing `SKILL.md` files already handled in Phase 1.

### Build

- **Flamegraph script: full clean + WebView cache purge + user directory fixes**: `npm run tauri:flamegraph` now performs a complete clean before profiling: `cargo clean`, SvelteKit/Vite cache removal (`.svelte-kit`, `node_modules/.vite`, `build`), and — critically — **WebKitGTK/WebKit per-app cache clearing** (`~/.local/share/com.neuroskill.skill/` on Linux, `~/Library/WebKit/` and `~/Library/Caches/` on macOS). The stale WebView cache was causing the profiled binary to show old HTML/CSS and the wrong icon even after a fresh build. The sudo `--preserve-env` list is also expanded to include `CARGO_HOME`, `RUSTUP_HOME`, `DISPLAY`, `WAYLAND_DISPLAY`, `DBUS_SESSION_BUS_ADDRESS`, `XDG_RUNTIME_DIR`, and `LOGNAME`, ensuring the profiled app uses current-user directories instead of root's.

## [0.0.51] — 2026-03-21

### Features

- **Cross-modal screenshot ↔ EEG search in CLI**: Added new CLI commands and WS endpoints for bridging screenshots and EEG data:
  - `search-images --by-image <path>` — search screenshots by visual similarity using CLIP vision embeddings (base64 image sent over WS, server-side CLIP embedding + HNSW search).
  - `screenshots-for-eeg [--start --end] [--window N]` — find screenshots captured near EEG recording timestamps ("EEG → screen" bridge). Auto-selects the latest session when no range is given.
  - `eeg-for-screenshots "query" [--k N] [--window N]` — search screenshots by OCR text, then return EEG labels and session info near each match ("screen → EEG" bridge).
  - New WS commands: `search_screenshots_by_image_b64`, `search_screenshots_vision`, `screenshots_for_eeg`, `eeg_for_screenshots`.
  - All commands support `--json`, `--mode`, `--k`, `--window`, and `--limit` flags.

- **Implicit evidence collection for protocols**: Every protocol execution now produces structured measurement data using a standardised `px:start:`/`px:end:` label schema with pipe-separated metrics (bar, stress_index, relaxation, focus, hr, mood, faa, rmssd, deltas, outcome). The LLM captures before/after snapshots automatically and silently for every protocol, determining outcome as positive (≥10% target improvement), neutral, or negative. No user action required — evidence collection is invisible infrastructure.

- **Personal protocol effectiveness ranking**: After 5+ labeled protocol executions, the LLM aggregates outcomes via `search_labels "px:end"` to build a personal ranking by success rate and average metric delta. Surfaces time-of-day patterns, trigger-specific effectiveness, and modality preferences. Presents insights like "Cold water face splash is your most effective stress intervention — 92% success, average stress drop 31%."

- **Evidence-driven protocol selection**: New matching guidance rule — "Evidence first." Before suggesting any protocol, the LLM checks past effectiveness data. Leads with proven personal winners over generic recommendations. Retires consistent failures after 4+ negative outcomes. Explores new protocols occasionally even with strong data. Tracks modality preferences across interventions.

- **Implicit life-event labeling**: The LLM silently labels mentioned life events (caffeine intake, walks, meals, meetings, exercise, app switches, sleep quality) with EEG metric snapshots to build a complete personal effectiveness map beyond formal protocols. Privacy safeguards: full transparency if asked, user owns all local data, no inference of unmentioned events.

- **Interactive graph screenshots**: Added screenshot nodes to the interactive 3D cross-modal search graph. When the "Screenshots" checkbox is enabled, screenshots are discovered via three strategies: semantic text search on the query (attached to the query node), semantic search on each text-label (attached to label nodes), and timestamp proximity ±30 min around EEG points. Screenshots are injected as proper connected graph nodes (cyan spheres with async-loaded thumbnail sprites), connected via `screenshot_link` edges, and fully participate in selection highlighting. Duplicates are suppressed by filename.
- **SVG/DOT export includes screenshots**: When screenshots are visible in the interactive graph, the exported SVG and DOT files now include screenshot nodes. The frontend regenerates the SVG/DOT via new `regenerate_interactive_svg` / `regenerate_interactive_dot` Tauri commands, passing the full augmented graph (including client-side screenshot nodes) to the backend layout engine.

- **Interactive search: screenshot discovery and 3D visualization**: The interactive cross-modal search now discovers screenshots near EEG neighbor timestamps, ranking them by window-title and OCR-text proximity to the query. Screenshot nodes appear as a new layer in the graph with dedicated styling (pink `#ec4899`). Two new edge kinds (`screenshot_prox` for temporal proximity and `ocr_sim` for text-based matches) connect EEG points to relevant screenshots. A new 3D perspective-projected SVG (`svg_3d`) is generated alongside the existing 2D layouts using 3-component PCA across all text embeddings. The `InteractiveGraphNode` struct gains `proj_z`, `filename`, `app_name`, `window_title`, `ocr_text`, and `ocr_similarity` fields. The `InteractiveSearchResult` includes the new `svg_3d` field. Both the Tauri command and WebSocket `interactive_search` handler are updated. DOT and flat SVG exports also render screenshot nodes.

- **Modality Router**: Added a decision table mapping 12 EEG triggers to 7 intervention modalities (Breath, Tactile, Cognitive, Visual, Movement, Auditory, Passive Physiological) so the LLM selects the best modality for each person's circumstances before choosing a specific protocol. Breathing is presented as one equal option among many, never the default. Includes a modality selection guide by context and phrasing examples showing how to offer multi-modal choices.

- **Multi-modal protocol restructuring**: Restructured ~30 protocols that previously defaulted to breathing to present non-breathing alternatives at equal priority using ", OR " choice points. Affected protocols include Focus Reset, Pre-Performance Activation, Extended Exhale, Physiological Sigh, Kapalabhati, 4-Count Energising, Wim Hof, Anger Processing, Grief Holding, Emotion Surfing, Joy Amplification, Emotional Boundaries, Excitement Regulation, all morning routines, all workout protocols, social media protocols, Pre-Meal Pause, Sleep Wind-Down, Co-Regulation, One-Handed Calm, Cortical Quieting, Coherence Building, Break Reset, and Post-Scroll Reset.

- **Matching Guidance updated**: Added "Modality first, protocol second" as the top matching rule and "Always offer at least two modalities" as a new requirement, ensuring the LLM never prescribes breathing without presenting alternatives.

- **neuroskill-evidence standalone skill**: New skill defining the implicit evidence collection and personal effectiveness engine used across all intervention-delivering skills. Includes the standardised `px:` label schema (`px:start`, `px:end`, `px:note`, `px:skip`, `px:auto`) with pipe-separated key=value context format, required fields (8 core EEG metrics), outcome determination rules by trigger type, mandatory before/after measurement flow, life-event implicit labeling (caffeine, meals, walks, meetings, sleep, exercise, app switches), hook trigger tracking, evidence aggregation by data maturity, personal protocol ranking algorithm (success_rate × avg_delta_target), 10 evidence-driven selection rules, evidence surfacing and tone guidelines, privacy safeguards, complete protocol name reference (100+ snake_case names), and a quick-reference integration pattern for other skills.

- **Non-breathing protocol repertoire**: Added 30+ new protocols across 6 categories for users who find breathing exercises inconvenient — Cognitive Reset (micro-tasking, peripheral vision, mental arithmetic, internal narration silence, gratitude, mental time travel, worry parking, category switching), Tactile & Haptic Regulation (texture scanning, pressure points, temperature contrast, bilateral tapping, structured fidget, palm press, hand warming), Oculomotor & Visual (saccade reset, candle gaze, colour hunting, slow tracking, panoramic vision, near-far focus), Micro-Movement / Discreet (isometric squeeze-release, jaw release, micro-walk, seated spinal wave, wrist/ankle circles, shoulder blade pinch, single-leg floor press), Auditory / Non-Music (sound mapping, humming, toning, environmental sound bath, whisper reading, rhythmic tapping), and Passive Physiological (dive reflex, deliberate yawning, chewing, gargling, wrist cooling, ear massage, tongue press, postural gravity drop). Updated matching guidance to prioritise non-breathing protocols when user preference or context indicates.

- **Protocol API Integration Guide**: Added a comprehensive guide teaching the LLM how to use the NeuroSkill API at every phase of protocol delivery — a 4-phase lifecycle (Before: validate trigger and baseline with `status`, `search_labels`, `label`; During: voice-guide with `say`, protect with `dnd_set`, monitor with `status` polling, time with `timer`; After: measure deltas, `label` outcomes, `notify` summaries, `say` results; Longitudinal: `search_labels` to find past protocol instances, `compare` protocol vs non-protocol sessions, `hooks_set` for auto-triggers, `umap` for visual separation, `sleep` for sleep protocol impact). Includes a 16-tool quick reference table, concrete JSON payload examples for each phase, and 10 integration principles.

- **Protocol API annotations (🔧)**: Grounded 17 key protocols with specific API call sequences — Theta-Beta Neurofeedback (tbr live feedback), Box Breathing (full lifecycle with auto-hook setup after 5 sessions), Cardiac Coherence (rmssd as live biofeedback), Loving-Kindness (faa shift tracking), Alpha Induction (real-time alpha feedback loop), Sleep Wind-Down (sleep staging comparison next morning), NSDR/Yoga Nidra (full voice-guided delivery via `say`), Flow State (minimal interruption + neural signature hooks), Micro-Tasking Focus (quick measurement), Study Focus Sprint (Pomodoro + data-driven scheduling), Morning Clarity (sleep summary + streak), Caffeine Timing (personal sensitivity mapping via labels), Digital Sunset (sleep_schedule-driven auto-trigger), Sensory Overload (immediate DND + pattern recognition), Hyperfocus Exit (stillness-based proactive hook), Between-Patient Reset (shift stress trajectory), and Post-Doom-Scrolling (app correlation + proactive nudge hooks).

- **Protocol personalisation engine**: Added a top-level Personalisation Engine to the protocol repertoire with 12 adaptation dimensions (age, physical ability, neurodivergence, emotional state, cultural background, gender/identity, professional context, social context, location/time, language, need urgency, intimacy level), 10 adaptation rules, and 12 concrete example adaptations showing the same EEG trigger delivered differently for an executive, new mother, teenager, construction worker, elder, autistic teen, night-shift nurse, grieving person, person with ADHD, pregnant woman, wheelchair user, and ESL speaker.

- **Context-specific protocol collections**: Added 11 new protocol sections (77 protocols total) for underserved user groups — Parenting & Caregiving, Elderly & Aging, Teens & Students, Neurodivergent-Friendly (ADHD, autism, OCD, RSD), Commuters & Travellers, Manual & Physical Workers, Healthcare & Shift Workers, Intimate & Relational, Accessibility-Adapted (visual/hearing/mobility/cognitive impairment, chronic pain), Culturally Diverse Practices (Chinese Wuqinxi, Mesoamerican Temazcal, Japanese Shinrin-Yoku, Hawaiian Ho'oponopono, African Ubuntu, Indian Bhramari, Aboriginal Dadirri, Tai Chi, Islamic Dhikr, Christian Centering Prayer, Hindu/Buddhist Mala meditation), and Situational Micro-Protocols (11 ultra-short real-moment interventions).

- **Diverse contextual variants across existing protocols**: Enhanced Attention, Stress, Emotional Regulation, Grounding, Sleep, Music, and Dietary protocols with persona-specific variant examples (marked with ◈) covering children, elders, wheelchair users, breath-averse users, public settings, non-English speakers, introverts, athletes, and more. Expanded music and food suggestions to span Western, South Asian, East Asian, Latin, African, K-pop, Arabic, and vegan cultures.

- **Status command enriched with usage analytics**: The `status` command now returns most-used apps (all-time, 24h, 7d), most frequent label texts (all-time, 24h, 7d), screenshot/OCR summary counts (total, with embedding, with OCR, with OCR embedding), top screenshot apps (all-time, 24h), and label text-embedding count. New fields: `apps`, `screenshots`, and expanded `labels` section in the JSON response.

### Performance

- **Eliminate PNG encode/decode round-trip in screenshot embed pipeline**: The capture thread now sends the pre-decoded `DynamicImage` directly to the embed thread instead of encoding it to PNG first. The embed thread calls fastembed's `embed_images()` with the `DynamicImage` directly, avoiding the CPU-intensive PNG encode (capture thread) → PNG decode (fastembed) round-trip. For LLM vision and OCR paths that still need encoded bytes, JPEG is produced lazily (~10× faster than PNG). OCR via `ocrs` now also operates on the decoded image buffer directly (`run_ocr_from_image`) instead of encoding to PNG and decoding again.

### Bugfixes

- **LLM screenshot tool commands**: Added `search_screenshots`, `screenshots_around`, `screenshots_for_eeg`, and `eeg_for_screenshots` to the `skill` tool's command enum, description, and alias resolution. The LLM can now invoke screenshot search commands correctly instead of failing with validation errors. Also maps CLI names (`search-images`, `screenshots-around`, etc.) to their WebSocket API equivalents.

- **CPU FFT fallback for CI**: Added `rustfft`-based CPU fallback in `skill-eeg` behind a feature gate. The `gpu` feature (opt-in) uses `gpu-fft` with wgpu; without it, tests and headless CI environments use pure-Rust `rustfft`. Fixes 33 test failures on GPU-less CI runners.

- **Lazy embedder retry**: When the text embedding model fails to initialise at startup (e.g. missing download, network error), semantic label search, interactive search, and re-embed commands now retry initialisation on demand instead of permanently returning "embedder not initialized".

- **Fix text embedder model resolution**: `build_embedder` used `EmbeddingModel::from_str` which only matches debug variant names (e.g. `BGESmallENV15`), not the `model_code` strings persisted in settings (e.g. `Xenova/bge-small-en-v1.5`). Added `resolve_embedding_model()` that first looks up by `model_code` from the supported-models list, falling back to the variant-name parser. Applied to both model init and `set_embedding_model` validation.

- **LLM skill tool args coercion**: When an LLM flattens command arguments to the top level (e.g. `{"command":"search_screenshots","query":"today"}` instead of `{"command":"search_screenshots","args":{"query":"today"}}`), the coercion layer now automatically wraps stray properties into the `args` object before validation. Also handles the common `"arguments"` misspelling as an alias for `"args"`. This prevents validation errors for all `skill` tool commands.

- **Missing skill commands in tool enum**: Added `sleep_schedule`, `sleep_schedule_set`, `health_summary`, `health_query`, `health_metric_types`, `health_sync`, `search_screenshots_vision`, and `search_screenshots_by_image_b64` to the skill tool command enum, description, and `is_skill_api_command()`. These WS commands were functional but invisible to the LLM.

- **Coerced tool arguments now reach execution**: The tool orchestrator validated and coerced LLM arguments (e.g. wrapping flat `query` into `args`) but discarded the coerced value before calling `execute_builtin_tool_call`. The executor re-parsed the original un-coerced string, so flattened skill args like `{"command":"search_screenshots","query":"today"}` passed validation but still failed at runtime because `args.get("args")` found nothing. Fixed both sequential and parallel execution paths to write coerced arguments back to `tc.function.arguments` before execution.

- **Generic hyphenated CLI name resolution**: Added a generic fallback in `resolve_skill_alias` that converts any hyphenated tool name to underscored form and checks if it's a valid skill API command. This catches LLMs copying CLI names from docs (e.g. `search-labels`, `session-metrics`, `sleep-schedule`, `dnd-set`) without needing to enumerate every variant.

- **Block LLM download/management commands**: Added `llm_download`, `llm_cancel_download`, `llm_pause_download`, `llm_resume_download`, `llm_refresh_catalog`, and `llm_logs` to the BLOCKED list in skill tool execution. These LLM self-management commands should not be callable from the LLM itself.

- **Fix missing description warnings for root skill files**: Added YAML frontmatter with `name` and `description` to `skills/SKILL.md`, `skills/README.md`, and `skills/METRICS.md` so the skill discovery scanner no longer emits "description is required in frontmatter" warnings.

### Refactor

- **3D PCA utility**: Added `pca_3d()` function to `skill-commands` for 3-component power-iteration PCA, complementing the existing `pca_2d()`.
- **SVG 3D generator**: Added `generate_svg_3d()` to `skill-commands::graph` — renders a dark-themed perspective-projected SVG with depth cues (scale, opacity, drop shadows) and a grid floor.

- **Extracted evidence collection from neuroskill-protocols**: Removed the ~190-line inline Evidence Collection section from the protocols skill and replaced it with a reference to the standalone neuroskill-evidence skill. The protocols skill now depends on the evidence skill for all measurement, labeling, and ranking rules, allowing any other skill to also follow the same evidence framework.

- **Split neuroskill-protocols into 11 domain sub-skills**: The monolithic 1866-line protocols skill has been split into a slim 411-line hub (personalisation engine, API integration guide, modality router, matching guidance, sub-skill index) plus 11 contextually-loaded domain sub-skills: `neuroskill-protocols-focus` (136 lines), `neuroskill-protocols-stress` (101 lines), `neuroskill-protocols-emotions` (110 lines), `neuroskill-protocols-sleep` (54 lines), `neuroskill-protocols-body` (106 lines), `neuroskill-protocols-routines` (117 lines), `neuroskill-protocols-nutrition` (129 lines), `neuroskill-protocols-music` (79 lines), `neuroskill-protocols-digital` (71 lines), `neuroskill-protocols-breathfree` (279 lines), and `neuroskill-protocols-life` (537 lines). Each sub-skill loads independently based on the user's message domain, enabling efficient use with small-context-window LLMs. The hub always loads when protocol intent is detected, and sub-skills load contextually alongside the neuroskill-evidence skill.

### Build

- **Bump cleans Rust artifacts**: `npm run bump` now runs `npm run clean:rust` at the end to remove `src-tauri/target`, freeing disk space after the preflight checks.

### CLI

- **CLI v1.2.0**: Bumped version to reflect new cross-modal search capabilities.
- Fixed misplaced shebang line that prevented `npx tsx cli.ts` from running.

### LLM

- **Cross-modal query guide in tool description**: Added a decision guide to the `skill` tool description that maps question patterns to the correct command and direction (e.g. "What was on screen during EEG?" → `screenshots_for_eeg`, "How was my brain when I saw X?" → `eeg_for_screenshots`). This helps the LLM pick the right cross-modal bridging command instead of guessing.

- **Status tool results formatted as readable text**: When the internal LLM calls the `status` command, the result is now converted from raw JSON to a human-readable text block with clear section headers (Device, Session, EEG Embeddings, Labels, Most Used Apps, Screenshots, Signal Quality, Current Scores, Hooks, Sleep, Recording History). This makes status output in the Chat window much easier to read for both the user and the model.

### Docs

- **Cross-modal screenshot↔EEG documentation**: Updated SKILL.md, neuroskill-screenshots skill, and skills index with documentation for `search-images --by-image`, `screenshots-for-eeg`, `eeg-for-screenshots` commands, cross-modal workflows, and all new WS endpoints.

- **SKILL.md updated for new and updated functionality**: Documented the enriched `status` command response with `apps` (top apps by window switches, all-time/24h/7d), `labels.top_all_time`/`top_24h`/`top_7d` (most frequent label texts), `labels.embedded` count, and `screenshots` summary (total, with_embedding, with_ocr, with_ocr_embedding, top_apps). Updated `interactive` search documentation to cover the new 5th screenshot layer, `screenshot` node kind with `filename`, `app_name`, `window_title`, `ocr_text`, `ocr_similarity` fields, `proj_z` 3-D PCA coordinate, new `screenshot_prox` and `ocr_sim` edge kinds, and `svg_3d` pre-rendered 3-D perspective SVG output. Added the `skill` built-in LLM tool to the tool-calling table with supported and blocked command lists.

- **Skills synced with SKILL.md changes**: Updated `neuroskill-labels` skill to document the 5th screenshot layer in interactive search (screenshot nodes with `filename`, `app_name`, `window_title`, `ocr_text`, `ocr_similarity` fields), new `screenshot_prox` and `ocr_sim` edge kinds, 3-D PCA projection fields (`proj_x`/`proj_y`/`proj_z`), and three SVG outputs (`svg`, `svg_col`, `svg_3d`). Updated `neuroskill-llm` skill with full `skill` tool documentation including supported commands, argument coercion, blocked self-management commands, and status formatting. Updated skill index description from 4-layer to 5-layer graph search.

- **Cross-modal workflow examples in skills**: Replaced the bash-only Cross-Modal Workflows section in the screenshots SKILL.md with a direction-based query guide table and multi-step LLM tool-call examples (JSON). Added Cross-Modal Follow-Ups sections to the search and labels SKILL.md files showing how to chain commands across modalities.

- **Screenshot & skills search tests**: Added comprehensive smoke tests for all 6 screenshot WS commands (`search_screenshots`, `screenshots_around`, `search_screenshots_vision`, `search_screenshots_by_image_b64`, `screenshots_for_eeg`, `eeg_for_screenshots`) covering semantic/substring modes, cross-modal EEG↔screenshot bridging, CLIP vision search, base64 image upload, error handling for missing/invalid fields, and result structure validation. Also added skills command rejection tests confirming Tauri-only commands are correctly rejected over WS/HTTP.

- **LLM tool call examples in skills**: Added `## LLM Tool Calls` sections with concrete JSON examples to all skill SKILL.md files (screenshots, labels, search, sessions, sleep, hooks, DND, streaming). This helps the LLM use the correct `{"command": "...", "args": {...}}` format.

- **Improved skill tool description**: Updated the `skill` tool description and `args` field description with explicit examples showing the `command` + `args` nesting pattern. Added SLEEP SCHEDULE and HEALTH command groups to the description.

- **Status SKILL.md**: Updated to document the new `apps` (top apps by window switches), `labels.top_*` (most frequent label texts), and `screenshots` (OCR counts, top apps) fields in the status response. Added LLM Tool Calls section with guidance on using `status` for app usage queries. Fixed JSON response example to show correct field names (`switches`, `last_seen`, `last_used`).

## [0.0.50] — 2026-03-21

### Features

- **Chat model picker in titlebar**: Click the model name in the titlebar to switch between downloaded models without leaving the conversation. The dropdown shows all downloaded models grouped by family with size info, and seamlessly stops the current model and loads the selected one.

- **LFM2.5 1.2B Instruct model**: Added LiquidAI/LFM2.5-1.2B-Instruct-GGUF to the LLM catalog with 7 quant variants (Q4_0, Q4_K_M, Q5_K_M, Q6_K, Q8_0, F16, BF16). Ultra-compact 1.2B-parameter text model that fits in under 2 GB VRAM.

- **LFM2.5 1.2B Thinking model**: Added LiquidAI/LFM2.5-1.2B-Thinking-GGUF to the LLM catalog with 7 quant variants (Q4_0, Q4_K_M, Q5_K_M, Q6_K, Q8_0, F16, BF16). Ultra-compact 1.2B-parameter reasoning model with chain-of-thought capability.

### Performance

- **Screenshot capture pipeline ~3× faster**: Eliminated redundant image encode/decode round-trips in the capture thread. `resize_fit_pad` no longer encodes to PNG (deferred to embed-thread send); `encode_webp` operates on the already-decoded `DynamicImage` instead of re-decoding from bytes; Linux/Windows xcap capture skips the ~500ms PNG encoding entirely by passing the decoded RGBA image directly. Expected improvement: ~2.9s → ~0.8s per iteration on Linux.

### Bugfixes

- **Fix label_store tests failing with "readonly database"**: The `TempDir` was dropped at the end of the helper function, deleting the temporary directory before the SQLite connection could write to it. The `TempDir` handle is now kept alive for the duration of each test.

### Build

- **Bump aborts on warnings**: `npm run bump` now captures stdout and stderr from every preflight check step and scans for warning lines. If any warnings are detected, the bump is aborted before any files are modified. `cargo clippy` is invoked with `-D warnings` to promote Rust warnings to errors.
- **Bump mirrors CI checks**: preflight now runs all CI-equivalent steps — `npm test` (vitest), `cargo clippy` on all workspace crates (not just the app crate), and `cargo test --lib` on the same crate subset as CI — so issues are caught locally before pushing.
- **Bump checks libpipewire-0.3**: added `libpipewire-0.3` to the Linux system dependency preflight check so the missing `-dev` package is caught early with a clear install hint instead of a cryptic cargo build failure.

### UI

- **Language dropdown close on outside click**: The language picker dropdown now stays open after selecting a language (showing the updated checkmark), and reliably closes when clicking anywhere outside the dropdown. Switched to `pointerdown` capture for robust outside-click detection across all window areas including drag regions.

- **Language flag padding**: Added padding around the language flag button in the title bar for better spacing across all windows.

## [0.0.49] — 2026-03-21

### Bugfixes

- **Fix a11y warnings in HistoryCalendar**: Replaced clickable `<div>` with a `<button>` for session bars in the calendar view, resolving svelte-check warnings about missing keyboard handlers and ARIA roles.

### Refactor

- **Fix clippy warnings in main crate**: Replaced `match` with `if let` in `worker.rs`, derived `Default` for `DndRuntimeState`, and used struct initializer for `InputTrackingState` in `state.rs`.

## [0.0.47] — 2026-03-21

### Performance

- **LLM E2E test: reduce CI time from ~15 min to ~2-3 min**: Lowered `max_tokens` from 512 to 128 and `max_rounds` from 2 to 1 for tool-chat test steps. The 1.6B model on CPU was generating max tokens every round at ~1.6 tok/s, and hallucinating calls to disabled tools caused extra rounds. Also added "Do NOT call any other tool" to system prompts to reduce hallucinated tool calls.

### Bugfixes

- **Fix false-positive "kill" safety check on "skill" commands**: `check_bash_safety` now uses word-boundary detection so patterns like `"kill "` no longer match inside words like `"skill"`. Commands such as `skill --help` or `neuroskill-status` no longer trigger the dangerous-command approval dialog. Actual `kill`, `killall`, and `pkill` commands (including after pipes/semicolons) are still correctly flagged.

- **Fix CI compilation and clippy errors**: Added missing `std::io::Cursor` import for Linux in `skill-screenshots`, replaced `.err().expect()` with `.expect_err()` in `skill-llm` tool orchestration, used `.is_multiple_of()` instead of manual modulo check, replaced `.map_or(true, …)` with `.is_none_or(…)`, fixed `needless_range_loop` clippy warnings in generation and actor modules, added `Default` impl for `ScreenshotMetrics`, simplified boolean expression in capture backfill check, and replaced `len() > 0` with `!is_empty()` in HNSW guards.

- **Fix unused `Cursor` import in skill-screenshots**: Tightened the `cfg` gate on `std::io::Cursor` in `platform.rs` so it is only imported when actually used (macOS, or Linux/Windows with the `capture` feature). Fixes a compile error with `-D warnings` on Linux CI without the `capture` feature.

## [0.0.46] — 2026-03-20

### Features

- **Add tests for untested modules**: Added 61 new unit tests across 4 previously untested modules:
  - `skill-eeg/eeg_model_config` (13 tests): config defaults, JSON round-trip, persistence save/load, corrupt file handling.
  - `skill-tools/types` (18 tests): `LlmToolConfig` defaults, dangerous tool safety, serialization, `ToolContextCompression` levels.
  - `skill-tools/defs` (21 tests): builtin tool definitions integrity, `is_builtin_tool_enabled` toggle logic, `resolve_skill_alias` routing.
  - `skill-tools/context` (9 tests): token estimation, context trimming, compression levels, system/user message preservation.

- **API bearer token authentication**: Added optional bearer token authentication for the HTTP/WS API. When `api_token` is set in settings, all requests must include `Authorization: Bearer <token>`. Empty token (default) disables auth — suitable for localhost-only binds. Configurable via Settings UI and `get_api_token`/`set_api_token` Tauri commands. i18n keys added for all 5 languages.

- **Add tests for `skill-constants` and `skill-router`**: 18 tests for skill-constants (filter math, band continuity, channel counts, Emotiv sample rate derivation, MutexExt poison recovery) and 11 tests for skill-router (rounding precision, NaN/infinity handling, command list integrity).

- **In-app device switching**: Cancel connection and switch to a different paired device directly from the main dashboard without using the tray icon menu. When connected with multiple paired devices, a "Switch Device" button appears in the footer. During scanning, a "Cancel" button and alternate device list are shown inline.

- **GPU memory safety guard**: Added pre-decode GPU memory checks to prevent Metal/CUDA `abort()` crashes when GPU memory is exhausted. The LLM engine now verifies sufficient free GPU/unified memory before starting prompt decode, multimodal decode, warmup, and periodically during token generation. When memory is too low, requests are rejected with a recoverable error message instead of crashing the entire application.

- **LLM E2E integration test with benchmarking and mock EEG data**: Full end-to-end Rust integration test (`crates/skill-llm/tests/llm_e2e.rs`) that: downloads a capable model (>=1.5B), starts the LLM server, runs a plain chat, a date tool-calling chat, and a NeuroSkill status tool-calling chat with a mock EEG API server returning realistic brain-state data (device info, signal quality, meditation/focus scores, session history, labels). Every step is benchmarked with timing and throughput (tok/s). All responses and tool events are captured and displayed in a formatted report. Runnable via `npm run test:llm:e2e`.

- **LLM VLM screenshot backend**: Added a new `"llm-vlm"` screenshot embed backend that uses the LLM vision model for both image embeddings (mean-pooled vision tokens via mmproj) and OCR (VLM-based text extraction via chat completion). This allows benchmarking VLM-based OCR against traditional OCR engines (ocrs / Apple Vision). Selectable in Settings → Screenshots → Embed backend. Also added `ocr_via_llm()` to the `ScreenshotContext` trait.

- **VLM image embedding benchmark in E2E test**: Added step 9 to the LLM E2E test that benchmarks VLM image embedding via the `EmbedImage` request path. Reports embedding dimensions and timing. Skipped with a warning when no mmproj is loaded.

- **LUNA model backend for EXG embeddings**: Added support for the [luna-rs](https://crates.io/crates/luna-rs) EEG foundation model as an alternative embedding backend alongside ZUNA. Users can now select between ZUNA (default) and LUNA in the EEG Model settings tab, with LUNA offering `base`, `large`, and `huge` model size variants. The selected backend and model size are persisted in `model_config.json`.

- **Embedding speed tracking**: Every embedding inference now tracks wall-clock time in milliseconds. The last embedding speed and an exponential moving average are displayed in the EEG Model tab and published via the WebSocket status endpoint. SQLite stores per-row `embed_speed_ms` for post-hoc performance analysis.

- **Model provenance in SQLite and HNSW**: Each embedding row now records which model backend (`zuna` or `luna`) produced it via the new `model_backend` TEXT column in the daily `embeddings` table. Historical rows without the column are auto-migrated on open. This enables filtering, auditing, and future re-embedding by model.

- **Per-model HNSW indices**: Each model backend now gets its own HNSW index file per day (`eeg_embeddings.hnsw` for ZUNA, `eeg_embeddings_luna.hnsw` for LUNA) and globally (`eeg_global.hnsw` / `eeg_global_luna.hnsw`). This prevents dimension mismatches when switching backends and allows side-by-side nearest-neighbor search for each model. The daily SQLite remains shared with a `model_backend` column to differentiate rows. Search APIs accept an optional model backend parameter to load the correct index.

- **Re-embed from raw EXG data**: Added `estimate_reembed` and `trigger_reembed` Tauri commands. The re-embed worker reads raw EEG samples from session CSV files (`exg_*.csv` / `muse_*.csv`), reads channel names and sample rate from the JSON sidecar, chunks data into 5-second epochs with 50% overlap, resamples to model input size, runs the selected encoder (ZUNA or LUNA) on the GPU, and writes new embedding rows to SQLite. Per-model HNSW indices are rebuilt per day and globally. Progress is streamed to the frontend via the `reembed-progress` event.

- **No overlap by default**: Changed the default embedding epoch overlap from 2.5 s to 0.0 s, so consecutive epochs no longer overlap. Users can still configure overlap via settings. This doubles the effective epoch interval from 2.5 s to 5.0 s, reducing redundant GPU work.

- **Add tests for `skill-autostart`**: 3 tests covering Linux XDG autostart enable/disable lifecycle, non-existent app safety, and disable idempotency.
- **Add tests for `skill-tts`**: 9 tests across config (defaults, JSON round-trip, empty JSON deserialization) and logging (enable toggle, write without callback, disabled noop).

- **Epoch-aligned screenshot interval**: Screenshot capture interval is now aligned with EEG embedding epochs (5 s). The slider offers multipliers from 1× (every 5 s) to 12× (every 60 s) in 5-second steps, replacing the old 1–30 second free-form slider. Legacy config values are automatically snapped to the nearest epoch boundary.

- **Tool argument type coercion**: `validate_tool_arguments` now coerces argument types before JSON Schema validation, fixing multi-model compatibility. Handles string-to-boolean (`"true"` → `true`), string-to-number (`"3"` → `3`), number-to-string (`42` → `"42"`), integer-to-float rounding, string-encoded JSON objects/arrays, and `"yes"`/`"no"`/`"on"`/`"off"` boolean aliases. Added `coerce_tool_call_arguments` public API for pre-execution coercion. Different LLM backends (Llama, Qwen, Mistral, Gemma, DeepSeek) emit arguments in subtly different type formats — this coercion layer normalises them transparently.

- **LLM helpers integration tests**: Added 65 tests in `llm-helpers.test.ts` covering the full download-progress lifecycle — unit tests for each helper, plus end-to-end scenarios simulating download start, window reopen, completion, failure, pause, multi-family downloads, shard progress, and stale-state recovery.

- **Dynamic context window growth**: When a prompt exceeds the current context window, the LLM engine now automatically attempts to grow the context size (up to the model's max trained context length) if VRAM/memory permits, before falling back to trimming chat history. This avoids unnecessary message loss on systems with enough memory.

- **Add tests for `skill-gpu`**: 3 tests covering struct construction, JSON serialization, and `read()` no-panic guarantee.

- **Add tests for `skill-llm` chat store**: 9 tests covering session CRUD, message save/load, tool call persistence, archive/unarchive, session params roundtrip, and temp directory isolation.

- **Add tests for `skill-history` cache**: 14 tests (was 5) covering downsample edge cases (empty, single element, max=0, max=1, evenly spaced), metrics cache path generation, sleep summary defaults, and sleep stage analysis with epochs.

### Performance

- **Raise minimum auto context size from 2048 to 4096**: The `recommend_ctx_size` heuristic no longer returns 2048 as a minimum, which was too small for most conversations with system prompts and tool results. The new floor is 4096 tokens.

- **Debounce settings persistence**: `save_settings()` is now debounced with a 500ms window. Multiple rapid settings changes (e.g., toggling several options in quick succession) are collapsed into a single disk write, preventing I/O storms. A `save_settings_now()` function ensures settings are flushed during app shutdown.

- **Remove excessive .clone() in hot loops**: Eliminated redundant `String`, `PathBuf`, and `Vec<f32>` allocations in search, index rebuild, and downsampling hot paths. Key changes:
  - `skill-commands`: Search functions now store indices into `date_dirs`/`day_indices` instead of cloning `(String, PathBuf)` per embedding and per HNSW hit; owned copies are only materialized for the final top-k candidates after truncation.
  - `skill-history`: `list_embedding_sessions` interns day-name strings via an index vector instead of cloning per DB row; session-gap loop references interned names by index.
  - `skill-history`: `downsample_timeseries` uses in-place `swap` + `truncate` instead of cloning large `EpochRow` structs into a new `Vec`.
  - `skill-history`: `analyze_search_results` borrows `&str` date references instead of cloning into the frequency map.
  - `skill-label-index`: `rebuild_indices` iterates `rows` by value so `Vec<f32>` embeddings are moved into HNSW insert instead of cloned.
  - `skill-screenshots`: HNSW rebuild loop iterates rows by value to move embeddings instead of cloning.
  - `skill-commands/graph`: Parent-ID dedup uses `as_deref()` + `&str` set instead of cloning `Option<String>` twice.

### Bugfixes

- **LLM crash on Metal buffer allocation failure**: Fixed a crash (`SIGABRT` in `ggml_metal_synchronize` → `ggml_abort`) that occurred when the Metal GPU backend failed to allocate buffers during `llama_decode`. The ggml abort is unrecoverable in-process, so pre-flight memory checks now prevent reaching that code path.
- **Reduced dynamic context growth memory budget**: Lowered the memory headroom multiplier for dynamic context window resizing from 85% to 70% of available GPU memory, reducing the risk of Metal OOM during large context operations.

- **Screenshot LLM token receiver**: Fixed `blocking_recv()` return type mismatch (`Ok(tok)` → `Some(tok)`) in `screenshot.rs` that prevented compilation with newer tokio mpsc channel API.

- **Test coverage expansion**: Added 164 new tests across frontend and Rust crates. Frontend: 3 new test files — `format.test.ts` (47 tests covering 31 formatting functions), `history-helpers.test.ts` (53 tests covering day math, label colors, duration formatting), `umap-helpers.test.ts` (32 tests covering easing, color conversion, geometry normalisation, gradient ticks). Rust: `skill-tools/exec.rs` (24 tests for truncation, path resolution, bash/path safety, UTC offset formatting), `skill-label-index` (8 tests for HNSW insert, search, rebuild, empty-state handling). Total test count: frontend 280→412, Rust skill-tools 58→82, skill-label-index 0→8.

- **Fix all 21 TypeScript type errors**: Resolved every `svelte-check` error across the codebase (was 21, now 0):
  - `ChatToolCard`: Added typed `arg()` helper for `Record<string, unknown>` tool args, typed `SourceEntry` interface for web search sources, cast `tu.result` through `Record` instead of direct `unknown` access.
  - `compare/+page.svelte`: Fixed `UmapProgress` cast through `unknown`.
  - `history-helpers.test.ts`: Fixed `LabelRow` field names (`wall_start` → `label_start`).
  - `umap-helpers.test.ts`: Fixed `UmapPoint` construction to match current interface.

- **Calibration timer drift**: Replaced sequential `sleep(1000)` countdown with wall-clock-based timing (`Date.now()`) to prevent cumulative drift over long calibration phases.
- **Calibration TTS desynchronization**: The break-phase "Next: …" announcement was fire-and-forget, causing it to queue behind the next action's TTS cue and delay the countdown start. Both break announcements now await completion before the countdown begins, ensuring audio and visual phases stay in sync.

- **Fix cubecl GlobalConfig panic loop on embedder respawn**: When the EEG embedder worker was respawned (e.g. after switching models), `configure_cubecl_cache` called `GlobalConfig::set()` a second time, causing a panic. Replaced the `Once` guard (which itself gets poisoned after a panic, triggering an infinite respawn loop) with an `AtomicBool` compare-exchange that silently skips the already-configured case.

- **Fix LUNA model download using wrong weights filename**: `download_hf_weights` was hardcoded to download `model-00001-of-00001.safetensors` (ZUNA) even when the LUNA backend was selected, causing repeated download failures because the LUNA repo uses `LUNA_base.safetensors`. Parameterised the function to accept `weights_file` and `config_file`, and updated the worker and settings commands to pass the correct filenames per backend.

- **DSP: use actual device channel count for QualityMonitor**: `SessionDsp::new` and the Emotiv desc-may-change reset path passed the compile-time constant `EEG_CHANNELS` (12) instead of the actual device channel count, producing extra phantom `NoSignal` quality entries for devices with fewer channels.

- **DSP: periodic status emit scales with sample rate**: The `process_eeg` status emit interval was hardcoded to `count % 256`, firing every ~1 s only at 256 Hz. Now uses the device's actual sample rate so the interval is ~1 s at any rate (128 Hz, 250 Hz, 500 Hz, etc.).

- **DSP: AC-coupled clip detection in QualityMonitor**: The clip-count check used absolute sample values, causing DC-coupled devices (e.g. Emotiv with ~4200 µV baseline) to report every sample as a clip and mark all channels as `Poor`. Clip detection now subtracts the window mean first, matching the existing AC-coupled RMS logic.

- **Add keys to critical `{#each}` loops**: Added `(key)` expressions to Svelte `{#each}` loops for session lists, device lists, day lists, settings tabs, search presets, and score keys across dashboard, history, search, compare, and settings pages. Prevents DOM thrashing when lists update.

- **Remove debug console.log**: Replaced 7 `console.log` calls in `UmapScene.svelte` with `console.debug` (filtered in production DevTools by default).

- **EEG embeddings: epochs now fire for all device channel counts**: The `EegAccumulator` used a fixed 12-element buffer array (`EEG_CHANNELS`) but checked *all* 12 buffers to decide when an epoch was ready. Devices with fewer channels (Muse=4, Emotiv Insight=5, Hermes=8) had empty inactive buffers whose `len()==0` prevented the epoch trigger from ever firing — meaning **no EEG embeddings were produced**. Now only active device channels (`0..device_channels`) are checked; inactive channels are zero-filled in the model input tensor.

- **EEG embeddings: correct resampling for non-256 Hz devices**: The accumulator already had `resample_linear()` and `native_epoch_samples` logic, but it was unreachable due to the channel-count bug above. Verified that the full path now works: native-rate samples are accumulated, and when an epoch is complete, each channel is resampled from `native_epoch_samples` to `EMBEDDING_EPOCH_SAMPLES` (1280) for the ZUNA model. Devices at 256 Hz skip resampling (identity path).

- **EEG embeddings: channel name padding for ZUNA preprocessing**: The `load_from_named_tensor` function requires `channel_names.len() == data.nrows()`. With 12-row zero-padded tensors but only 4 device channel names, this assertion would fail. The worker now pads channel names with synthetic `_padN` labels for inactive rows, which don't match any 10-20 electrode position and get default spatial coordinates.

- **Add diagnostic logging for Emotiv Cortex session creation**: When connecting to an Emotiv headset, the session creation wait loop silently discarded all non-SessionCreated events. Now logs each event type (Connected, Authorized, Warning, HeadsetsQueried, etc.) so connection issues can be diagnosed from the log output.

- **Move disk I/O outside AppState lock in set_eeg_model_config**: `save_model_config` (disk write) was called while holding the AppState mutex, which could block other subsystems (including the async Cortex connection) from acquiring the lock. Now the config is persisted after the lock is released.

- **LLM download progress lost on window reopen**: When starting a model download in LLM settings, closing the window, and reopening it, the download progress bar was not shown. The poll timer only refreshed the catalog when it already knew about an active download, creating a chicken-and-egg problem on fresh mounts. Fixed by always polling the catalog (a cheap in-memory read) so in-flight downloads are detected regardless of initial component state. Also added the missing `"paused"` variant to the frontend `DownloadState` type.

- **Fix LUNA crash on channels with mixed-case names**: LUNA's channel vocabulary uses uppercase names (e.g. `PZ`) but some devices like EMOTIV INSIGHT send mixed-case (e.g. `Pz`), causing a panic in `channel_indices_unwrap`. The embed worker now normalises channel names to uppercase and filters out any channels not in the LUNA vocabulary instead of panicking.

- **Fix screenshot HNSW panic on vision model change**: Switching the screenshot embedding backend (e.g. fastembed 512-dim → mmproj 2048-dim) caused a panic when inserting into the existing HNSW index built with a different dimension. Both vision and OCR HNSW indices now detect dimension mismatches and reset to a fresh index, with a log message suggesting re-embed to backfill.

- **Fix metrics-only SQLite insert failing with NOT NULL constraint**: When the GPU device was poisoned and the embedder fell back to metrics-only mode, `insert_metrics_only` tried to insert NULL for `eeg_embedding` which has a NOT NULL constraint in existing databases. Now inserts an empty blob for backward compatibility, and new databases use a nullable column.

- **Fix LUNA huge/large variant crash (GroupNorm dimension mismatch)**: The HuggingFace `config.json` for LUNA lacks per-variant hyperparameters, so all variants loaded with the `base` config (embed_dim=64, depth=8). The `huge` variant (embed_dim=128, depth=24) crashed with "Expected 16, got 32" in GroupNorm. Added `LUNA_VARIANT_CONFIGS` constants with correct dimensions for each variant and a `luna_variant_config_path` helper that generates a variant-specific config file before loading the encoder.

- **Fix mmproj "Use" button deadlock**: `set_llm_active_mmproj` and `set_llm_active_model` called `save_catalog()` which re-acquires the LLM mutex while it was already held, causing a deadlock that froze the UI. Switched to `save_catalog_locked()` which operates on the already-held lock guard.

- **Fix "prompt too long" error when prompt exceeds n_ctx**: The LLM engine now automatically trims older chat history messages (keeping system prompt and latest user turn) when the tokenized prompt exceeds the context window budget and the context cannot be grown further. Previously, conversations would fail with "prompt too long (N >= n_ctx M)" once history grew past the context limit.

- **Remove SQL expect() panics in screenshot and health stores**: Replaced 26 `.expect("static SQL")` / `.expect("SQL query")` calls in `skill-data` crate (`screenshot_store.rs`, `health_store.rs`) with graceful fallbacks (`let-else` returning empty vectors, `if-let` skipping failed prepare). The app no longer panics on database corruption, disk-full, or other runtime SQLite errors in these code paths.

- **Replace silent catch blocks with console.warn logging**: Added descriptive `console.warn` messages to ~80 silent `catch {}` / `.catch(() => {})` blocks across all frontend source files. Each warning includes a bracketed module tag (e.g. `[chat]`, `[tts]`, `[goals]`) and the failed operation name, making previously invisible failures easy to diagnose in the browser console. Affected files span routes (+page.svelte for home, chat, search, history, calibration, onboarding, settings, session, labels, api, compare, focus-timer), layout, and shared lib components (TtsTab, LlmTab, GoalsTab, SleepTab, ToolsTab, ScreenshotsTab, UpdatesTab, ChatSidebar, ChatMessageList, MarkdownRenderer, TtsTestWidget, HelpElectrodes, theme-store, window-title, i18n).

- **Replace `any` types with proper TypeScript types**: Replaced `any` with `unknown` in error catch callbacks (`LlmTab`, `SettingsTab`), added `instanceof Error` checks for error message extraction, typed Tauri invoke results with `Record<string, unknown>` instead of `any[]`, and replaced `as any` event payload casts with typed `Record` casts.

- **Remove `.unwrap()` from production Rust code**: Replaced all 137 `.unwrap()` calls in production (non-test) code with safe alternatives — `expect("reason")` for provably-safe invariants, `unwrap_or`/`unwrap_or_default` for fallible values, `let Some(...) = ... else { ... }` for early returns, and `?` for propagation. Prevents potential panics in session CSV/Parquet writers, PPG analysis, screenshot store SQL queries, health store, GPU stats, history cache, tool call parsing, TTS engine, LLM init, headless browser, autostart, job queue, device session adapters, tray icons, settings commands, and all Mutex lock sites. Test code retains `.unwrap()` as is standard practice.

- **Replace `.lock().expect("lock poisoned")` with `.lock_or_recover()`**: Converted all 37 occurrences of panicking mutex locks across 6 files (`api.rs`, `lib.rs`, `settings_cmds`, `ws_commands`, `llm/cmds/server.rs`, `llm/cmds/streaming.rs`) to use the poison-recovering `MutexExt::lock_or_recover()` trait. The app will now gracefully recover from poisoned locks instead of crashing.

- **Fix missing `format` field in LlmModel**: Added the required `format: ModelFormat::Gguf` field to the `LlmModel` constructor in `hardware_fit.rs`, fixing a compilation error introduced by an upstream `llmfit-core` dependency update.

### Refactor

- **Extract DND/sleep from ws_commands**: Moved DND status/set and sleep schedule get/set into dedicated `dnd_sleep.rs` sub-module. `ws_commands/mod.rs` reduced from 873 to 695 lines.
- **Delete orphaned `bt_monitor.rs`**: Removed 71-line dead file (Bluetooth radio check) that was never imported by any module.

- **skill-headless is now optional in skill-tools**: The `skill-headless` dependency (wry/tao headless browser) is now behind an optional `"headless"` feature (on by default). `skill-llm` depends on `skill-tools` with `default-features = false`, so the LLM E2E test no longer pulls in wry/tao (fixes macOS compile error). When headless is disabled, web_fetch/web_search gracefully fall back to plain HTTP.

- **Deprecate Muse-defaulting DSP constructors**: `BandAnalyzer::new()`, `ArtifactDetector::new()`, and `QualityMonitor::new()` are now `#[deprecated]` with guidance to use the sample-rate-aware variants (`new_with_rate`, `with_channels`, `with_window`). Added `FilterConfig::passthrough_with_rate(sr)` for non-Muse passthrough configs. This prevents accidental use of 256 Hz defaults with non-Muse hardware.

- **`EegAccumulator` tracks `device_channels`**: New field set by `set_device_channels()`, used to scope buffer checks and epoch building to active channels only. Buffers are also cleared on device change to prevent stale data from a prior session.

- **Unit tests for `resample_linear`**: Added 5 tests covering identity, upsample, downsample, empty source, and zero target cases.

- **Extract LLM helpers into testable module**: Moved all pure functions and types from `LlmTab.svelte` into `$lib/llm-helpers.ts` (vendor labels, quant ranking, family grouping, entry sorting, entry group splitting, family auto-selection, download detection). The component now imports from the shared module.

- **Extract `load_and_apply_settings` from `setup_app`**: Moved the 80-line settings hydration block into its own `#[inline(never)]` function, reducing `setup_app` from 574 to ~490 lines.

- **Consolidate frontend stores into `src/lib/stores/`**: Moved 12 scattered `.svelte.ts` store files from `src/lib/` root into a dedicated `src/lib/stores/` directory. Merged 5 tiny titlebar-related files (`titlebar-state`, `chat-titlebar`, `history-titlebar`, `label-titlebar`, `help-search-state`) into a single `stores/titlebar.svelte.ts`. Renamed remaining stores to drop the `-store` suffix (e.g. `theme-store` → `theme`, `toast-store` → `toast`). Updated ~50 import paths across the codebase.

- **Extract `skill-gpu` crate**: Moved GPU utilisation/memory stats from `skill-data::gpu_stats` (698 lines, 19 `unsafe` blocks) into its own standalone `skill-gpu` crate with zero Tauri dependencies. `skill-data` re-exports `skill_gpu::*` for backward compatibility.

- **Extract history page canvas rendering into `history-canvas.ts`**: Moved 3 pure canvas rendering functions (`renderDayDots`, `renderDayGrid`, `renderSparkline`) and the `heatColor` utility from the 2,224-line `history/+page.svelte` into a dedicated `src/lib/history-canvas.ts` module (280 lines). The history page now delegates to these functions via thin wrappers, reducing it to 1,983 lines (-241). All rendering logic is now testable independently of Svelte reactive state.

- **Move hooks CRUD + keyword suggestions into `hook_cmds.rs`**: Extracted `sanitize_hook`, `get_hooks`, `set_hooks`, `get_hook_statuses`, `suggest_hook_keywords` (with helper functions `norm_keyword`, `fuzzy_score`, `merge_suggestion`) from `settings_cmds/mod.rs` into the existing `hook_cmds.rs` sub-module. `mod.rs` reduced from 959 to 761 lines.

- **Remove `any` types from core interfaces**: Replaced all `any` annotations in `chat-types.ts`, `search-types.ts`, and `chat-utils.ts` with proper types (`Record<string, unknown>`, `unknown`, discriminated union `ContentPart`). Added `typeof` guards in `detectToolDanger` for safe property access. Added explicit casts at `JobPollResult` consumption sites in search and compare pages.

- **Extract `skill-health` crate**: Moved HealthKit data store from `skill-data::health_store` into its own standalone `skill-health` crate with zero Tauri dependencies. The new crate has 9 unit tests covering sync idempotency, per-table queries, metric type listing, and aggregate summaries. `skill-data` re-exports `skill_health::*` for backward compatibility — no consumer changes needed.

- **Split AppState into domain sub-states**: Extracted `LlmState` and `DndRuntimeState` behind their own `Arc<Mutex<>>` inside `AppState`, eliminating lock contention between LLM operations (model loading, chat streaming, downloads) and the EEG/device hot path. LLM and DND code now acquires independent locks without blocking device status reads or UI commands. Reduced the `new_boxed()` stack allocation from 32 MB to 8 MB.

- **Split AppState into domain sub-states**: Extracted `ShortcutState`, `UiPrefsState`, `InputTrackingState`, and `EmbeddingModelState` from the monolithic 50+ field `AppState` struct. Fields are now accessed via `state.shortcuts.*`, `state.ui.*`, `state.input.*`, and `state.embedding.*`. This improves code organization and prepares for future independent locking to reduce mutex contention.

- **Split `lib.rs` and `settings_cmds/mod.rs`**: Extracted 5 new modules from the two largest files in `src-tauri`. `lib.rs` reduced from 1,580 to 1,413 lines by moving the macOS external renderer (172 lines) into `external_renderer.rs`. `settings_cmds/mod.rs` reduced from 1,560 to 938 lines by extracting device commands (`device_cmds.rs`), activity tracking (`activity_cmds.rs`), screenshot config/search (`screenshot_cmds.rs`), and skills management (`skills_cmds.rs`). All re-exports preserved — no changes to the public API or `generate_handler!` invocation.

- **Split ws_commands/mod.rs into sub-modules**: Extracted calibration (7 commands), health (4 commands), and screenshot (2 commands) handlers into dedicated `calibration.rs`, `health.rs`, and `screenshots.rs` sub-modules. Reduced `mod.rs` from 1168 to 873 lines while keeping the dispatch table as the single routing point.

- **Clean up TODO.md**: Removed 4 completed items, keeping only active/open tasks.

### Build

- **Centralize workspace dependencies**: Added `[workspace.dependencies]` for `rusqlite`, `serde`, and `serde_json` to the root `Cargo.toml`. Updated 7 crates (`skill-health`, `skill-data`, `skill-commands`, `skill-llm`, `skill-router`, `skill-label-index`, `skill-history`) plus `skill-gpu` to use `{ workspace = true }`, ensuring a single version and feature set across the workspace.

- **Cargo workspace consolidation**: All 21 Rust crates now share a single Cargo workspace defined in the project root `Cargo.toml`. A shared target directory (`src-tauri/target/`) eliminates the ~3.6 GB of duplicate build artifacts that previously accumulated in per-crate `target/` directories under `crates/`. The `.cargo/config.toml` was moved from `src-tauri/` to the project root, and `[patch.crates-io]` / `[profile.dev]` sections were moved to the workspace root as required by Cargo. All CI and release workflows updated accordingly (`Swatinem/rust-cache` workspace paths, `Cargo.lock` hash keys, `cargo build -p skill` from workspace root).

- **LLM E2E test in CI**: Added `llm-e2e` job to the CI workflow that runs the full LLM integration test on main pushes and manual triggers. The HuggingFace model cache is persisted across runs. The full report is saved as a build artifact (`llm-e2e-report`) with 30-day retention, and the formatted report table is rendered in the GitHub Actions Job Summary. The Discord notification now includes the LLM E2E result.

- **Clean all clippy warnings and enforce in CI**: Fixed ~25 clippy warnings across 7 crates (`skill-eeg`, `skill-data`, `skill-tools`, `skill-headless`, `skill-skills`, `skill-history`, `skill-label-index`, `skill-tts`). Fixes include adding `Default` impls, replacing manual modulo checks with `.is_multiple_of()`, fixing doc indentation, removing redundant closures, using `is_some_and`, and properly iterating with `enumerate`. Added a workspace-wide `cargo clippy -- -D warnings` step to CI so new warnings are caught before merge.

- **Update LUNA HuggingFace repo to PulpBio/LUNA**: Changed the default LUNA model repository from `thorir/LUNA` to `PulpBio/LUNA` (`https://huggingface.co/PulpBio/LUNA`). Existing configs with the old repo are auto-migrated on load.

### UI

- **Full context viewer popup**: Added a "View full context" button to the context breakdown chart. Clicking it opens a full-screen modal showing every message in the context window (system prompt, user messages, assistant responses, tool calls/results) with role-colored headers, per-message token estimates, and a copy-all button. Closeable via backdrop click or Escape key.

- **Company logos in Supported Devices**: Each company section in the Devices tab now displays the company's logo (fetched as favicon from the official website) instead of the first device image preview. Logos added for Interaxon (Muse), Neurable, OpenBCI, Emotiv, IDUN Technologies, and RE-AK.

- **Scanning cancel button**: Added an inline Cancel button during the scanning state so users no longer need the tray menu to abort a connection attempt.
- **Device switcher panel**: When connected, a collapsible panel lists other paired devices with one-click "Switch" buttons that disconnect and reconnect automatically.

- **GPU memory threshold settings**: Added configurable GPU memory safety thresholds in Settings → LLM → Inference Settings. Users can set the minimum free GPU memory required before decode (default: 0.5 GB) and during generation (default: 0.3 GB), or disable the checks entirely.

- **Smarter onboarding LLM model selection**: The onboarding model picker now uses a priority chain: already-downloaded model → Qwen3.5 4B Q4_K_M → LFM2.5-VL 1.6B Q8_0 (ultra-compact fallback) → any recommended model (smallest first). This ensures low-memory devices get a working LLM out of the box.

- **Extract `HistoryStatsBar` component**: Moved the 75-line recording streak / stats bar from `history/+page.svelte` into a reusable `$lib/HistoryStatsBar.svelte` component with proper i18n (7 new keys: streak messages, days/hours/sessions labels, week trend). History page reduced from 1985 to 1924 lines.

- **Screenshot interval slider**: Updated to show epoch-aligned steps (5 s, 10 s, …, 60 s) with multiplier badge (e.g. "10s (2× epoch)").

- **Company logo white background**: Company logos in the Devices tab now always have a white background instead of a translucent muted background, ensuring consistent logo visibility in both light and dark modes.

- **Proactive Hooks accent consistency**: Replaced all hardcoded `primary` (neutral black/white) and `ring-ring` references with accent-remapped `violet-500` family in the Hooks tab. Keyword suggestion focus highlight, scenario select focus ring, distance suggestion panel (border, background, threshold text, percentile bar zones and markers), relative-age text, and the enabled checkbox now all honor the user's Appearance accent setting.

- **LLM quick test snippet selectable & copyable**: The curl code snippet in LLM advanced settings is now text-selectable (overriding the global `user-select: none`) and includes a Copy button for one-click clipboard copy.

- **Show correct encoder name and loading state in EEG Model settings**: The encoder status section was hardcoded to show "ZUNA Encoder" regardless of the selected backend. Now dynamically shows the correct name (e.g. "LUNA Encoder (huge)") based on the selected backend and variant. The status indicator dot also pulses blue while the encoder is loading on the GPU.

- **Screenshot settings accent consistency**: Replaced all hardcoded `primary` (neutral black/white) references with accent-remapped `violet-500` family across the entire Screenshots tab. Toggles, range sliders, progress bar, badges, select focus rings, icons, and the privacy note now all honor the user's Appearance accent setting.

- **Add SvelteKit root error boundary**: New `+error.svelte` at the route root catches unhandled errors across all 16 routes with a friendly message, "Go to Dashboard" link, and "Reload Page" button — prevents blank white screens on `invoke()` failures.

- **Improve accessibility with aria-labels**: Added `aria-label` and `aria-expanded` attributes to icon-only and toggle buttons across high-traffic pages (search, history, chat sidebar). Covers interactive search expand/collapse, +/- stepper controls, analysis panel toggle, close/dismiss buttons, chat new/archive/restore/delete buttons.

- **OpenBCI config moved into Device API card**: The standalone OpenBCI configuration section has been folded into the Device API card as a collapsible sub-section, alongside Emotiv Cortex and IDUN Cloud, for a more consistent layout.

- **Remove 2K context size option from LLM settings**: The 2K (2048 tokens) context size option was removed from the LLM inference settings UI. The backend auto-recommend (`recommend_ctx_size`) already treats 2048 as "too small for practical use" and never selects it — the minimum auto-recommended context is 4K. The default remains "auto", which intelligently picks the largest context size that fits in available GPU/unified memory. Users can still manually select 4K, 8K, 16K, 32K, 64K, or 128K.

- **Extract `HistoryCalendar` component**: Moved the 208-line calendar heatmap (year/month/week views) from `history/+page.svelte` into a reusable `$lib/HistoryCalendar.svelte` component. History page reduced from 1924 to 1735 lines.

- **Extract `OnboardingChecklist` component**: Moved the 34-line onboarding checklist from the dashboard `+page.svelte` into `$lib/OnboardingChecklist.svelte`. Fixed hardcoded "Dismiss" button → `t("common.dismiss")`. Dashboard reduced from 1678 to 1644 lines.

### LLM

- **Qwen3 30B-A3B Instruct**: Added `byteshape/Qwen3-30B-A3B-Instruct-2507-GGUF` to the LLM catalog. MoE architecture with 30B total / 3B active parameters — fast inference with strong reasoning. Five quant variants: Q3_K_S 2.70bpw (10.3 GB), Q3_K_S 3.25bpw (12.4 GB), Q4_K_S 3.61bpw (13.8 GB, recommended), Q4_K_S 3.92bpw (15.0 GB), and IQ4_XS 4.67bpw (17.8 GB).

### i18n

- **Context viewer translations**: Added i18n keys for the context viewer (en, de, fr, uk, he).

- **Device switcher translations**: Added new i18n keys for device switching UI in English, German, French, Ukrainian, and Hebrew.

- **GPU memory safety strings**: Added translation keys for the new GPU memory threshold settings (`llm.inference.gpuMemThreshold`, `gpuMemThresholdDesc`, `gpuMemDecode`, `gpuMemGen`) in all languages (en, fr, uk, de, he).

- **Pre-commit hook enforces i18n key synchronisation**: Added `.githooks/pre-commit` that runs `npm run sync:i18n:check` when any file under `src/lib/i18n/` is staged. Blocks commits with missing translation keys and guides the developer to run `npm run sync:i18n:fix`. The hook is automatically activated via `postinstall` (`git config core.hooksPath .githooks`) and skips entirely when no i18n files are changed (~0 ms overhead on normal commits).

- **Screenshot interval strings**: Updated English, German, French, Hebrew, and Ukrainian translations for the epoch-aligned interval description and added `screenshots.intervalEpoch` key.

- **Migrate hardcoded English strings to i18n**: Replaced 12 raw English `title` attributes with `t()` calls across 8 components (ChatMessageList, CustomTitleBar, DevicesTab, TimeSeriesChart, dashboard, compare, api). Added 13 new i18n keys (`common.copy`, `common.copyToClipboard`, `common.newer`, `common.older`, `common.clickToHide`, `common.clickToReveal`, `common.goalReached`, `common.resetZoom`, `common.openComparison`, `common.error`, `error.description`, `error.goHome`, `error.reload`) for all 5 languages.

- **Add `common.dismiss` key**: New i18n key for all 5 languages, replacing the hardcoded "Dismiss" string in the onboarding checklist.

### Docs

- **Add doc comments to `skill-router` public API**: Added `///` documentation to all 6 rounding helper functions (`r1`, `r2`, `r3`, `r1d`, `r2d`, `r2f`).

### CI

- **Run Rust tests in CI**: Added `cargo test` step to the CI pipeline covering 13 testable crates (486 tests). Previously CI only ran `cargo check` and `clippy` — tests were never executed.
- **Add new crates to CI clippy**: Added `skill-health`, `skill-gpu`, `skill-screenshots`, and `skill-llm` to the workspace clippy check (were missing since extraction).

## [0.0.45] — 2026-03-19

### Features

- **Auto-connect to paired devices**: When the BLE scanner discovers a previously paired device while the app is idle (disconnected, no active session or pending reconnect), a session is automatically started. No cooldown is needed — `start_session()` immediately marks the app as connecting, preventing duplicate attempts, and the normal retry backoff handles failures.

- **Emotiv multi-headset selection**: when multiple Emotiv headsets are paired in the EMOTIV Launcher, the scanner now lists each one individually (e.g. `EPOCX-A1B2C3D4`, `INSIGHT-5AF2C39E`) in the discovered devices list instead of a single generic "Emotiv (Cortex)" entry. Users can pair and connect to the specific headset they want. The selected headset ID is passed to the Cortex API so the correct device is targeted.

- **Comprehensive GIF recording for all app UIs**: Added `scripts/screenshots/take-gifs.mjs` — a Playwright-based tool that records 58 animated GIFs covering every page, tab, toggle, expandable section, and hidden parameter panel in the app. Supports `--filter`, `--theme`, and `--list` CLI flags. Covers: dashboard (full scroll, electrode guide expand/collapse, collapsible sections), all 18 settings sub-tabs with toggle-revealed panels (DND automation with threshold/duration/lookback/SNR, OpenBCI config, calibration editor, LLM advanced inference, tool toggles with web search provider, screenshot OCR and metrics, skills), chat (sidebar, settings panel, tools panel), search (EEG/Text/Images modes), history (session expand), session detail, compare, help (all 11 tabs with scroll), calibration electrode tabs, onboarding wizard, labels search modes, focus timer config, downloads, API code examples, about, and what's new.

- **IMU data recording**: Devices with IMU sensors (Muse, Hermes, Emotiv, IDUN) now record accelerometer, gyroscope, and magnetometer data to `exg_<ts>_imu.csv` (or `.parquet`). Data includes `timestamp_s`, `accel_x/y/z`, `gyro_x/y/z`, `mag_x/y/z` columns.
- **Storage format selector in Settings**: Added a Recording Format picker (CSV / Parquet / Both) to the Settings tab. The "Both" option writes CSV and Parquet files simultaneously for every data stream (EEG, PPG, IMU, metrics).
- **GPU and memory stats moved to Settings**: The GPU / Unified Memory (RAM) panel is now shown in the Settings tab instead of the EXG tab, where it logically belongs alongside other system configuration.

- **Scanner backend settings**: New "Scanner Backends" section in the Devices tab with toggles for each transport (BLE, USB Serial, Emotiv Cortex). Changes are persisted in `settings.json` and take effect on next app restart.

- **Emotiv Cortex connection indicator**: The Cortex scanner toggle shows a live "Connected to Cortex" / "Not connected" badge based on whether Emotiv devices have been discovered via the Cortex WebSocket API.

- **Device log viewer**: New collapsible "Device Log" panel in the Devices tab showing a live, color-coded log of scanner and session events (BLE discovery, USB detection, Cortex polling, connect/disconnect, watchdog). Auto-refreshes every 3 seconds. Entries are kept in a 200-entry ring buffer.

- **Transport badge on devices**: Discovered devices now show a transport badge (USB, Cortex, WiFi) next to their name when they were found via a non-BLE transport.

- **Animated GIF capture for scrolling/animated windows**: The screenshot capture worker now detects motion between consecutive frames using pixel-difference scoring. When the change exceeds a configurable threshold (default 5%), a rapid burst of frames is captured and encoded as an animated GIF alongside the still WebP screenshot. New config fields: `gif_enabled`, `gif_frame_count`, `gif_frame_delay_ms`, `gif_motion_threshold`, `gif_max_size_kb`. The middle frame of the burst is used as the representative image for CLIP embedding and OCR. GIFs exceeding the size limit are automatically discarded.

- **Emotiv adapter test coverage**: Added tests for EEG translation, headset disconnect on stop-all-streams/close-session warnings, error-to-disconnect mapping, and warning filtering.

- **Tool thinking budget**: Added a configurable thinking budget override for tool-calling rounds in the Tools settings panel. Options: Chat (use chat-level setting), None (0), 256, 1K, 4K tokens. Lower values make the model respond faster after tool results. Stored in `LlmToolConfig.thinking_budget`.

### Bugfixes

- **Emotiv auto-connect no longer hijacks first headset**: Cortex devices are no longer blindly auto-connected as "trusted transport". Only explicitly paired headsets trigger auto-connect, preventing the first headset from being grabbed when multiple are available. Legacy `cortex:emotiv` paired entries are still honored for backward compatibility.
- **Emotiv scanner discovers headsets on first tick**: the Cortex scanner runs its first probe immediately at startup (before the 900 ms auto-connect fires) so all headsets are discovered and visible in the device list. Subsequent probes are skipped while a session is active to avoid invalidating the session's cortex token.

- **Data watchdog for silent BLE disconnects**: Added a 15-second data watchdog to the session event loop. If no device event arrives within the timeout, the connection is treated as silently lost and auto-reconnect is triggered. This catches scenarios where the BLE link stays alive but GATT notifications stop flowing (radio interference, device sleep, firmware hang).

- **Reconnect retry limit**: Auto-reconnect now gives up after 12 consecutive failed attempts (~51 seconds of total backoff) instead of retrying indefinitely. A toast notification informs the user to reconnect manually when the limit is reached. This prevents draining battery on a device that was intentionally turned off or moved out of range.

- **Disable GIF burst capture by default**: Changed `gif_enabled` default from `true` to `false` so the app only takes still screenshots during normal operation. GIF burst capture (motion detection + multi-frame capture) is intended for use in scripts only and can be explicitly enabled when needed.

- **Disable GIF capture in periodic screenshot loop**: The normal app screenshot worker no longer produces animated GIFs via motion detection. GIF burst capture is now reserved exclusively for scripts. The `gif_encode` module and config fields are preserved for the script-level API.

- **Emotiv disconnect cleanup**: when an Emotiv headset disconnects before any EEG data is recorded (e.g. powered off right after connecting, or Cortex session interrupted), the app now properly transitions to the disconnected state. Previously the UI would stay stuck on "connected" because `go_disconnected` was only called when the CSV recording file had been opened — which requires at least one EEG frame.
- **Disconnect events for all break paths**: the data watchdog timeout and event-channel-closed paths in the session runner now call `on_disconnected` to emit the `device-disconnected` event and toast, consistent with the explicit `DeviceEvent::Disconnected` path.
- **Emotiv headset disconnect/failure warnings**: the adapter now handles Cortex warning codes 102 (HEADSET_DISCONNECTED) and 103 (HEADSET_CONNECTION_FAILED) in addition to codes 0 and 1. This triggers an immediate disconnect instead of waiting for the 15-second data watchdog timeout.
- **Emotiv EEG subscribe error no longer causes infinite reconnect loop**: subscribe failures (e.g. Cortex error -32230 "stream not supported") are no longer treated as disconnects. The connect flow now fails with a clear user-facing error explaining the license requirement, and auto-reconnect is disabled for non-recoverable configuration errors.

- **Emotiv subscribe race condition**: `connect_emotiv` now waits for the `SessionCreated` event before calling `subscribe`. Previously it called `subscribe` immediately after `client.connect()`, but `connect()` only opens the WebSocket — the auth flow (hasAccessRight → authorize → queryHeadsets → createSession) runs asynchronously. The subscribe was sent with an empty cortexToken and session ID, causing an immediate `-32014 Cortex token is invalid` error.

- **Emotiv session stability**: Upgraded to emotiv crate v0.0.4 which prevents `ACCESS_RIGHT_GRANTED`, `HEADSET_CONNECTED`, and `HEADSET_SCANNING_FINISHED` warning handlers from re-authorizing or re-querying headsets when a session is already active.

- **Emotiv scanner is now side-effect-free**: The Cortex scanner probe only authorizes — it does NOT send `queryHeadsets` or `getCortexInfo`. The scanner also skips polling entirely when a session is active or a reconnect is pending, and waits 5 seconds at startup to avoid racing with the auto-connect flow.

- **Emotiv auto-connect without pairing**: Cortex-discovered and USB-discovered devices are now treated as trusted transports and auto-connect when the app is idle, without requiring manual pairing first. BLE devices still require pairing as before (since BLE advertisements can come from any nearby device).

- **Emotiv reconnect uses correct device ID**: `start_session` now pins the scanner-level device ID (e.g. `"cortex:emotiv"`) into `status.device_id` before the adapter runs. This ensures `on_connected` pairs the device with the correct ID (instead of the Cortex session ID), and reconnect retries route to `connect_emotiv` via the `cortex:` prefix.

- **Device kind routing by ID prefix**: `detect_device_kind` now checks the device ID prefix (`cortex:` → emotiv, `usb:` → ganglion) before falling back to name-based detection. This ensures Cortex-discovered devices route to `connect_emotiv` regardless of their headset ID format.

- **Emotiv EEG subscribe confirmation**: after subscribing to Cortex streams, the connect flow now waits up to 3 seconds for the EEG DataLabels response to confirm the subscription succeeded. If EEG subscription fails (e.g. due to a missing license), the error is logged and a toast is shown instead of silently streaming IMU-only data with empty EEG channels. Events consumed during the confirmation wait are replayed into the adapter so DataLabels are not lost.

- **Emotiv headset disconnect detection**: The Emotiv adapter now translates Cortex API warning codes `CORTEX_STOP_ALL_STREAMS` (0) and `CORTEX_CLOSE_SESSION` (1) into `DeviceEvent::Disconnected`, giving the session runner instant notification when a headset goes away instead of waiting up to 15 seconds for the data watchdog to fire. `CortexEvent::Error` is also surfaced as a disconnect to trigger immediate reconnection.

- **Emotiv sample rate per model**: EPOC X, EPOC+, EPOC Flex, Insight 2, MN8, and X-Trodes now correctly report 256 Hz instead of the hardcoded 128 Hz. The sample rate is derived from the headset ID prefix (e.g. `EPOCPLUS-*` → 256 Hz, `INSIGHT-*` → 128 Hz). This affects DSP filter configuration, band analysis, artifact detection, and CSV recording timestamps.

- **Emotiv device visible in UI with correct channel count**: The dashboard now uses dynamic `channel_names` from the connected device when available, instead of always using the hardcoded 14-channel EPOC layout. This fixes Emotiv Insight (5ch), MN8 (2ch), and Flex (32ch) devices showing wrong/missing EEG waveforms. Colors auto-extend by cycling the palette for high-channel-count devices.

- **Emotiv headset name in UI**: The Emotiv adapter now reports the actual headset ID (e.g. "INSIGHT-5AF2C39E") as the device name instead of the generic "Emotiv", so the dashboard and session metadata show which model is connected.

- **LLM skill sub-command auto-redirect**: When the LLM calls a Skill API sub-command (e.g. `status`, `say`) or a `neuroskill` alias (e.g. `neuroskill`, `neuroskill-status`, `neuroskill-hooks`) as a top-level tool, the call is silently rewritten to `skill` with the correct `{"command": "..."}` at three layers: extraction (parse.rs), validation (tool_orchestration.rs), and execution (exec.rs).
- **LLM dedup loop fix**: When the model re-emits the same tool call on round 2 (all calls deduped), instead of returning empty text, the orchestrator now injects a nudge message telling the model to summarize the existing results, then continues to a new inference round.
- **Tool result not misdetected as tool call**: JSON objects with `"ok"` or `"command"` keys (tool results) are no longer falsely extracted as tool calls when the model quotes them in its response.
- **Model not responding after tool call**: Improved tool result message prefix to prevent the model from interpreting tool results as new user questions. Assistant messages now include tool call details.

- **Devices are no longer auto-paired**: previously every device that connected was automatically added to the paired list, even without the user clicking "Pair". Now only explicitly paired devices are remembered. The single exception is first-time onboarding: if no devices are paired at all, the first successful connection auto-pairs as a convenience so new users can test immediately.
- **Auto-connect requires explicit pairing**: the scanner no longer auto-connects USB or other "trusted transport" devices without pairing. Only devices the user has explicitly paired (or first-time onboarding) trigger auto-connect.
- **Startup auto-connect skipped when no paired devices**: on first launch with no paired devices, the app no longer blindly scans and connects to the first device it finds. The user must discover and pair a device manually.

- **Paired Emotiv devices no longer appear in discovered list**: discovered Cortex devices (e.g. `cortex:EPOCPLUS-06F2DDBC`) now correctly match against the legacy `cortex:emotiv` paired entry, so paired headsets show in the "Paired" section instead of "Discovered". On first successful connection, the legacy ID is automatically migrated to the real headset ID.

- **Fix rustls CryptoProvider panic at startup**: Multiple transitive dependencies (`tauri-plugin-updater`, `emotiv`, `hf-hub`/`fastembed`) activated both the `ring` and `aws-lc-rs` features of rustls 0.23, preventing automatic provider selection. The app now explicitly installs the `ring` crypto provider at the start of `run()`, fixing the "Could not automatically determine the process-level CryptoProvider" panic.

- **Signal quality limited to actual electrodes**: The `status` WebSocket command now returns `signal_quality` entries only for electrodes that exist on the connected device, instead of always returning 12 entries (padded with `no_signal` for non-existent channels). The quality vector is also cleared on disconnect and at startup.

- **Unknown tool calls show misleading "disabled" error**: When the LLM hallucinates non-existent tool names (e.g. "status", "neuroskill-status"), the error now correctly says "unsupported tool" with guidance to use available tools, instead of the misleading "tool disabled in settings".

### Refactor

- **Extracted shared Tauri mock**: Moved `buildTauriMock()` from `take-screenshots.mjs` into a shared `scripts/screenshots/tauri-mock.mjs` module, imported by both the screenshot and GIF scripts.
- **Enhanced Tauri mocks**: Added full DND config (enabled by default with all sub-settings visible), complete LLM tools config (web search, execution mode, context compression, skills), expanded screenshot config (all sliders/pickers), and skills/license mocks.

- **Settings tab cleanup**: Removed device listings (Supported Devices, Paired/Discovered Devices, OpenBCI config, Device API) from the Settings tab. These are already available in the dedicated Devices tab.
- **Settings tab cleanup**: Removed Signal Processing and EEG Embedding sections from the Settings tab. These are already available in the dedicated EXG tab.
- **StorageFormat enum**: Extended `StorageFormat` with a `Both` variant and `as_str()` method. `SessionWriter` now supports `Both(CsvState, ParquetState)` for dual-format recording.
- **Session cleanup**: `delete_session` now removes IMU data files and Parquet variants alongside CSV files.

- **Scanner log tag**: Added `scanner` subsystem to the logging system (`LogConfig`) so scanner events can be toggled independently from `bluetooth` session events.

- **New `gif_encode` module in `skill-screenshots`**: Extracted GIF encoding and representative-frame extraction into a dedicated module (`gif_encode.rs`) with `encode_gif()` and `representative_frame_png()` helpers.
- **Motion detection and burst capture in `platform.rs`**: Added `motion_score()` for pixel-diff comparison and `capture_burst()` for rapid multi-frame capture.
- **`gif_filename` column in screenshot store**: Added SQLite migration and `update_gif_filename()` method to `ScreenshotStore`. All query result types (`ScreenshotResult`) now include the `gif_filename` field.

- **Renamed `bt_error` to `device_error`**: the error field in `DeviceStatus` was named `bt_error` (Bluetooth-specific) but is used for all device connection errors including Cortex WebSocket (Emotiv), USB serial (OpenBCI), and BLE. Renamed to `device_error` throughout the backend, frontend types, and dashboard UI to reflect the transport-agnostic nature of the field. Also renamed `classify_bt_error` → `classify_device_error`.

- **Generalized device scanner**: Replaced `ble_scanner.rs` with `device_scanner.rs` — a unified background scanner that runs multiple transport-specific backends in parallel:
  - **BLE** — discovers Muse, MW75, Hermes, Ganglion, IDUN devices (existing logic, unchanged)
  - **USB serial** — polls for OpenBCI Cyton/CytonDaisy dongles by detecting FTDI FT231X USB VID/PID and common port patterns (ttyUSB, usbserial), every 5 seconds
  - **Cortex WebSocket** — checks for Emotiv headsets via the local EMOTIV Launcher service (`wss://localhost:6868`), every 10 seconds; only polls when Emotiv credentials are configured
  - All backends share the same auto-connect logic: if a paired device is discovered while idle, connect automatically

- **Transport tag on discovered devices**: `DiscoveredDevice` now carries a `transport` field (`ble`, `usb_serial`, `wifi`, `cortex`) so the UI can display a transport badge. The transport is inferred from device ID prefixes (`usb:`, `cortex:`, or BLE by default).

- **Split EXG tab from Devices tab**: the EXG tab now has its own view with Signal Processing (notch/high-pass/low-pass filters), EEG Embedding (epoch overlap), and GPU/Memory stats. The Devices tab retains paired/discovered devices, supported devices, OpenBCI config, Device API credentials, Scanner Backends, and Device Log.

### Build

- **New npm scripts**: Added `npm run screenshots` and `npm run gifs` convenience commands.
- **New dev dependencies**: Added `gif-encoder-2` and `sharp` for GIF frame encoding and resizing.

### UI

- **Emotiv EEG license error page**: when the Cortex API rejects the EEG stream subscription (-32230), the dashboard shows a dedicated violet card explaining the issue with a "Manage Emotiv Account" button that opens the Cortex Apps settings page directly.

- **Device info badge shows actual sample rate**: the dashboard device badge (e.g. "EPOCPLUS-06F2DDBC · 14ch · 256 Hz") now reads the sample rate from the backend status instead of being hardcoded. All non-Muse device badges (Ganglion, Emotiv, IDUN, Hermes) were updated.

- **Tool card i18n fallback for unknown tools**: The chat tool card now shows the raw tool name instead of a raw i18n key (e.g. "status" instead of "chat.tools.status") when the LLM calls an unrecognized tool.

- **Compact supported devices list**: Collapsed supported devices into an accordion layout — each company shows as a single row with a thumbnail preview and device count. Device image grid and connection instructions are only revealed on expand. Reduces vertical space usage significantly when browsing the Devices tab.

### LLM

- **`neuroskill` tool alias**: `neuroskill` is recognized as an alias for `skill`. Hyphenated forms like `neuroskill-status`, `neuroskill-sessions`, `neuroskill-hooks` map to the corresponding API command.
- **Skill tool description restructured**: Compact grouped categories instead of a flat list of 30+ commands. The `command` parameter includes a JSON Schema `enum` constraint.
- **Skill tool call examples added**: Both full and compact tool prompts include an explicit `skill` calling example.

### i18n

- **Scanner & storage format translations**: Added 23 missing i18n keys to de, fr, he, and uk locales — scanner backends, device log, log scanner, and recording format settings.

### Docs

- **UI Guide**: Added `docs/UI.md` — comprehensive visual walkthrough of every screen, panel, and interaction in the app. Uses GitHub collapsible sections with animated GIFs showing real UI recordings and static light/dark PNGs. Covers: Dashboard (metrics, electrode guide, collapsible sections), Settings (all 18 tabs with toggle-revealed panels), Chat (sidebar, settings/tools panels), Search (EEG/Text/Images modes), History (day view, session expansion), Session Detail, Compare, Help (all 11 tabs), Calibration, Onboarding, Labels, Focus Timer, Downloads, API, About, and What's New.

- **Update all crate README files**: Synchronized README.md files across all workspace crates with current source code. Added missing modules (`health_store`, `session_parquet`, `session_writer` in skill-data), documented the full device adapter system with 6 hardware drivers in skill-devices, expanded skill-constants with all constant groups (device-specific, LLM, tool-calling, WebSocket, DND, calibration, app metadata, skills), updated skill-settings for `UserSettings` rename and new types (`DeviceApiConfig`, `SleepConfig`, `DoNotDisturbConfig`, `ScreenshotConfig`), documented skill-llm engine sub-modules (`actor`, `generation`, `protocol`, `state`, `think_tracker`, `tool_orchestration`), expanded skill-vision with full API docs, and added `skill-headless` and `skill-skills` to the AGENTS.md workspace crates table.

### Dependencies

- **emotiv**: bumped from 0.0.5 to 0.0.7 — adds `CortexEvent::HeadsetsQueried` and `CortexHandle::query_headsets()` for safe headset enumeration; guards `connect_headset`/`create_session` side effects behind `auto_create_session` flag.

- **emotiv**: bumped to 0.0.9 — adds `HEADSET_DISCONNECTED` (102) and `HEADSET_CONNECTION_FAILED` (103) warning constants; emits `CortexEvent::Disconnected` immediately when either is received; logs failed stream subscriptions.

- **emotiv**: bumped to 0.0.8 — failed stream subscriptions are now logged and emitted as `CortexEvent::Error` instead of being silently ignored.

## [0.0.44] — 2026-03-19

### Features

- **Skills auto-refresh**: Community skills are now periodically downloaded from GitHub to `~/.skill/skills/`. Users can configure the refresh interval (off / 12 h / 24 h / 7 d) or trigger a manual sync from the Tools settings tab. A background task checks freshness and downloads the latest tarball when stale. The new `sync` feature in `skill-skills` handles download, extraction, and metadata tracking via a `.skills_last_sync` sidecar file.
- **Skills download on onboarding**: Community skills are automatically downloaded when onboarding completes, so fresh installs have the latest skills available immediately.
- **Agent Skills settings card**: Separate card in the Tools tab listing all discovered skills with their descriptions (pulled from SKILL.md frontmatter). Individual skills can be toggled on/off, with bulk Enable All / Disable All actions. Disabled skills are excluded from the LLM system prompt. Changes are live-applied to the running LLM server without restart.

### Bugfixes

- **Missing `skill_api` in chat tool config**: Added the missing `skill_api` property when constructing `toolConfig` from loaded LLM config, fixing a TypeScript error.

- **Missing Cursor import on macOS**: Added `use std::io::Cursor` to `skill-screenshots/src/platform.rs` to fix compilation error on macOS.

### UI

- **Skill API tool toggle**: The NeuroSkill Skill API tool can now be enabled/disabled from both the Settings > Tools tab and the Chat tools panel, matching all other built-in tools. Previously the Skill API tool was always injected when available with no user-facing toggle.

### i18n

- **Skill API tool strings**: Added i18n labels and descriptions for the Skill API tool toggle in English, German, French, Hebrew, and Ukrainian.

## [0.0.43] — 2026-03-18

### Features

- **Auto context size recommendation**: When the user has not set an explicit context size, the LLM server now uses `llmfit`-style memory estimation to pick the largest power-of-two context (2K–128K) that fits the system's available GPU/unified memory with 15% headroom, instead of always defaulting to 4096. Each catalog entry now carries `params_b` and `max_context_length` metadata so the estimator knows the model's parameter count and trained context ceiling. User-set context values are capped at the model's maximum. The UI context-size picker dynamically hides options that exceed the active model's trained limit and now offers 64K and 128K choices for models that support them.

- **Parquet data consumption across the app**: All data-reading paths now check for both `.parquet` and `.csv` files, preferring Parquet when it exists. This ensures sessions recorded in Parquet format are fully visible in history, metrics analysis, session search, and the metrics cache.
  - `find_metrics_path` / `find_ppg_path` helpers try `.parquet` then `.csv`
  - `load_metrics_csv` dispatches to `load_metrics_from_parquet` for `.parquet` files
  - `read_metrics_time_range` handles both formats for timestamp patching
  - `is_session_data` matches both `.csv` and `.parquet` EEG data files
  - `extract_timestamp` strips `.csv`, `.parquet`, and `.json` suffixes
  - `skill-commands` session lookup resolves `.parquet` before `.csv`
  - Metrics disk cache validates mtime against whichever data file exists
  - File size reporting checks for `.parquet` data files

- **Parquet recording format**: EEG, PPG, and metrics data can now be stored in Apache Parquet format (Snappy compression) instead of CSV. Set `storage_format: "parquet"` in settings or use the `set_storage_format` Tauri command. Default remains CSV for backward compatibility.
  - `exg_<ts>.parquet` — raw EEG samples (timestamp + N channel columns)
  - `exg_<ts>_ppg.parquet` — PPG optical data with vitals
  - `exg_<ts>_metrics.parquet` — derived band-power metrics (~4 Hz)
  - New crate deps: `parquet`, `arrow-array`, `arrow-schema` (all v54)
  - `SessionWriter` enum wraps both `CsvState` and `ParquetState` with identical API
  - Tauri commands: `get_storage_format`, `set_storage_format`
  - Setting persisted in `settings.json` as `storage_format: "csv" | "parquet"`

- **Split/sharded GGUF support**: The LLM catalog and downloader now support multi-part (split) GGUF models. Added `shard_files` field to `LlmModelEntry` listing all shard filenames in order. The new `download_model()` function downloads shards sequentially with overall progress tracking, pause/resume per-shard, and cancellation between shards. Delete properly removes all shard files. The frontend shows shard count on download buttons and current shard progress during download.

- **MiniMax M2.5 full catalog**: Added 11 quant variants of MiniMax M2.5 (456B MoE, 46B active) to the LLM catalog via `unsloth/MiniMax-M2.5-GGUF` — from TQ1_0 (52 GB single file) through Q8_0 (226 GB, 6 shards). The Q4_K_M quant is marked as recommended.

- **Embedding pipeline resamples non-256 Hz devices**: The ZUNA model expects 1280 samples (5 s × 256 Hz). Non-256 Hz devices now accumulate 5 seconds at their native rate and linearly resample to 1280 samples before encoding. Previously, MW75 (500 Hz) fed 2.56 s of data and Emotiv (128 Hz) fed 10 s, producing wrong-duration epochs with mismatched frequency content.
- **EEG chart dynamically sized for device sample rate**: `EegChart` now accepts a `sampleRate` prop and sizes its waveform ring buffer and spectrogram columns to always show ≈15 seconds of history regardless of device. Added `bufSizeForRate()` and `specColsForRate()` helpers. Previously the buffer was hardcoded to 3840 samples (15 s at 256 Hz only).

### Bugfixes

- **Fix missing `storage_format` in `UserSettings::default()`**: Added the missing field initializer in the `Default` impl for `UserSettings` in `skill-settings`.

- **Fix duplicate `MUSE_SAMPLE_RATE` import**: Removed redundant import in `eeg_embeddings/mod.rs` and added missing `CHANNEL_NAMES` import.

- **Fix renamed IDUN field**: Updated `use_60hz` → `mains_freq_60hz` in `session_connect.rs` to match upstream `idun` crate API change.

- **MoE detection for hardware fit**: The hardware-fit analyzer now detects MoE models from the `"moe"` tag in addition to inferring from family name patterns, improving fit predictions for MiniMax M2.5 and similar models.

- **CI: add missing libpipewire-0.3-dev package**: `cargo check` on Linux CI failed because the `xcap` crate transitively depends on `pipewire-sys` / `libspa-sys`, which require the PipeWire development headers. Added `libpipewire-0.3-dev` to the apt package lists in `ci.yml` and `release-linux.yml` and bumped the cache version keys to force re-caching.

- **Fix clippy warnings**: Removed unused `std::io::Cursor` import in `skill-screenshots`, changed doc comment to plain comment in `session_runner.rs`, replaced `map_or(true, …)` with `is_none_or(…)` in LLM download/server commands, and used `matches!` macro in `session_connect.rs`.

- **Session history only loaded `muse_` files**: All session-file lookups across `skill-history`, `skill-commands`, and `settings_cmds` now accept both `exg_` and `muse_` prefixes. Previously recordings from non-Muse devices were invisible.
- **Orphaned CSV sessions hardcoded 256 Hz sample rate**: When a JSON sidecar was missing, `sample_rate_hz` was set to `Some(256)`. Now set to `None` (unknown) since the actual rate cannot be determined without metadata.
- **Emotiv electrode count in ElectrodeGuide**: Updated `EMOTIV_EPOC_LABELS` from 12 to all 14 electrodes, and tab count from "12" to "14".
- **Non-Muse electrode quality strip said "Muse signal"**: Changed label to generic "Signal".

- **Device routing missed "neurable" and "ige" prefixes**: `detect_device_kind` in `lifecycle.rs` had its own copy of device-name matching that was out of sync with `DeviceKind::from_name`. A Neurable headphone would be routed to the Muse connect path, and an IGE-prefixed IDUN Guardian would also fall through to Muse. Refactored to delegate to the canonical `DeviceKind::from_name` — single source of truth.
- **Emotiv auto-detects actual channel count**: `EmotivAdapter` now detects the real channel count from the first EEG packet. Previously it always assumed EPOC (14 channels); connecting an Insight (5-ch) or MN8 (2-ch) would produce misaligned EEG frames with wrong channel counts.

- **Emotiv TS channel count/names**: Fixed `EMOTIV_CAPS` in `device.ts` — `channelCount` was 12 instead of 14, and electrode names were missing `"F8"` and `"AF4"` (EPOC X/EPOC+ have 14 channels).
- **MW75 Rust detection missing "neurable"**: Added `n.contains("neurable")` to `DeviceKind::from_name` so devices advertising as "Neurable-XYZ" are correctly identified as MW75, matching the TypeScript detection logic.
- **Hermes TS electrode names**: Replaced generic `["Ch1",...,"Ch8"]` with proper 10-20 names `["Fp1","Fp2","AF3","AF4","F3","F4","FC1","FC2"]` to match the Rust constants in `skill-constants`.
- **IDUN adapter META capability**: Added `DeviceCaps::META` to `IdunAdapter` caps, since the adapter emits `DeviceEvent::Meta` for `GuardianEvent::DeviceInfo` but was not declaring the capability.

- **FAA uses name-based electrode lookup**: Frontal Alpha Asymmetry in `eeg_bands.rs` now resolves left/right frontal electrodes by 10-20 name instead of hardcoded indices [1]/[2]. Previously, non-Muse devices computed FAA from wrong electrodes (e.g. Emotiv used F7/F3 — both left hemisphere).
- **Cognitive load uses name-based electrode lookup**: `compute_cognitive_load` now finds frontal (theta) and parietal (alpha) electrodes by 10-20 name prefix instead of assuming Muse's 4-channel index layout. Falls back to index-based split for generic labels.
- **Laterality index uses name-based hemisphere detection**: `laterality_index_fn` now determines left/right hemisphere from 10-20 naming convention (odd=left, even=right) instead of hardcoded indices [0..1] vs [2..3]. Previously, MW75 computed laterality from 4 left-hemisphere channels only.
- **IDUN battery not shown in UI**: Added `isIdun` to the `hasBattery` derived flag in `+page.svelte` so the battery indicator renders for IDUN Guardian (which reports battery via BLE).
- **Ganglion showed Muse electrode labels**: Added `GANGLION_CH`/`GANGLION_COLOR` constants and wired them into the dashboard channel-label selector. Previously, connecting a Ganglion displayed Muse names (TP9/AF7/AF8/TP10) instead of generic Ch1–Ch4.
- **Hermes channel labels in constants.ts**: Updated `HERMES_CH` from generic `Ch1–Ch8` to proper 10-20 names matching Rust `HERMES_CHANNEL_NAMES`.
- **Emotiv constants.ts channel count/names**: Updated `EMOTIV_EEG_CHANNELS` from 12 to 14, added missing `F8`/`AF4` to `EMOTIV_CH` and corresponding colours.
- **Misleading Ganglion sample-rate comment**: Corrected doc comment on `MUSE_SAMPLE_RATE` — Ganglion uses 200 Hz, not 256 Hz.

- **Non-Muse devices had no device_id in status**: `on_connected` now stores `info.id` in `status.device_id` when it hasn't been set yet. Previously only Muse's connect path set this field, leaving it `None` for MW75, Hermes, Emotiv, IDUN, and Ganglion — breaking reconnection targeting and paired device tracking.
- **Device identity fields not populated from DeviceInfo**: `on_connected` now copies `serial_number`, `firmware_version`, `hardware_version`, `bootloader_version`, `mac_address`, and `headset_preset` from the adapter's `DeviceInfo` into `DeviceStatus`. Previously these were only populated from Muse Control JSON meta events.
- **Meta handler only parsed Muse short keys**: `process_meta` now accepts both Muse-style short keys (`sn`, `ma`, `fw`, `hw`, `bl`, `tp`) and long-form keys (`serial_number`, `mac_address`, `firmware_version`, `hardware_version`, `bootloader_version`, `headset_preset`). IDUN Guardian's `DeviceInfo` meta event (which uses long keys) was previously ignored.

- **Rust `DeviceKind` enum missing Ganglion, MW75, Hermes**: Added `Ganglion`, `Mw75`, and `Hermes` variants with correct capabilities (channel count, sample rate, IMU flags). Previously Ganglion was lumped into `OpenBci` and MW75/Hermes had no representation.
- **`DeviceKind::from_name` missing prefixes**: Added `simblee`, `mn8`, and `guardian` prefix detection; split Ganglion from OpenBCI; added MW75 (substring) and Hermes detection.
- **Frontend `deviceCapabilities()` incomplete**: Added `GANGLION_CAPS` (4ch/200Hz), `MW75_CAPS` (12ch/500Hz), and `HERMES_CAPS` (8ch/250Hz) with correct electrode names. Ganglion was previously detected as OpenBCI (8ch/250Hz).
- **Frontend Ganglion detection wrong**: `"simblee"` prefix now correctly returns Ganglion caps instead of falling through to unknown.

- **BandAnalyzer hardcoded 256 Hz sample rate**: Added `BandAnalyzer::new_with_rate(sample_rate)` and wired it through `SessionDsp`. Previously, PSD bin-frequency mapping and PAC computation used `MUSE_SAMPLE_RATE` (256 Hz) for all devices. On MW75 (500 Hz) this mapped bins at nearly 2x the correct frequency, placing "beta 13-30 Hz" at ~25-59 Hz. On Emotiv (128 Hz) the "beta band" actually integrated ~6.5-15 Hz (theta+alpha). All band powers, ratios (TAR, BAR, DTR), and derived scores (meditation, drowsiness, headache, consciousness indices) were wrong for non-Muse devices.
- **ArtifactDetector hardcoded 256 Hz and Muse electrode indices**: Rewrote blink detection to accept a sample rate and resolve frontal electrodes by 10-20 name via `ArtifactDetector::with_channels(sample_rate, channel_names)`. Previously, blink refractory timing was wrong for non-256 Hz devices, and only Muse's AF7/AF8 (indices 1/2) were checked — other devices detected blinks on wrong channels or not at all.
- **Session runner now sets device sample rate in filter config**: Before creating the DSP pipeline, the session runner writes the device's actual sample rate into `FilterConfig.sample_rate`, ensuring the `EegFilter` frequency-domain operations (low-pass, high-pass, notch) use correct bin frequencies for all devices.

- **Embedding worker used Muse channel names and 256 Hz for all devices**: `load_from_named_tensor` in the ZUNA embedding worker was called with hardcoded `CHANNEL_NAMES` (TP9/AF7/AF8/TP10) and `MUSE_SAMPLE_RATE` (256 Hz) regardless of the connected device. Now receives actual channel names and sample rate via `EpochMsg`, set from the device descriptor at session start.
- **Embedding overlap computation used 256 Hz**: `EegAccumulator::set_overlap_secs` converted seconds to samples using `MUSE_SAMPLE_RATE`. Now uses the device's actual sample rate.
- **Scanning message showed "Looking for Muse" for MW75, Hermes, and IDUN**: Added device-specific scanning messages using the `connectingTo` i18n key for non-Muse/non-Ganglion/non-Emotiv devices.

- **Emotiv dashboard showed 14 channel labels but only 12 had data**: The DSP pipeline caps at `EEG_CHANNELS` (12), so the last two EPOC electrodes (F8, AF4) were never forwarded to the frontend. Aligned `EMOTIV_CH`, `EMOTIV_COLOR`, `EMOTIV_CAPS`, ElectrodeGuide, and ElectrodePlacement to show 12 channels matching the pipeline output. Prevents undefined values in the signal quality grid and EEG expanded view.

- **Emotiv adapter uses DataLabels for channel detection**: When the Cortex API sends `DataLabels` for the "eeg" stream, the adapter now updates its channel count and names to match the actual headset (EPOC 14-ch, Insight 5-ch, MN8 2-ch, Flex 32-ch). Previously only the first-packet sample-count fallback was used.
- **IDUN Guardian notch filter now respects user setting**: `connect_idun` now reads the user's notch filter preference (50 Hz / 60 Hz) from the app settings and passes it to `GuardianClientConfig::use_60hz`. Previously the on-device notch always defaulted to 60 Hz, producing mains artifacts for users in 50 Hz countries (Europe, Asia, Africa, most of South America).

- **Emotiv & IDUN devices never connected**: `detect_device_kind` in the session lifecycle had no arms for Emotiv or IDUN device names, causing them to fall through to the Muse connect path. Added prefix matching for Emotiv (`emotiv`, `epoc`, `insight`, `flex`, `mn8`) and IDUN (`idun`, `guardian`) devices, and wired the `"emotiv"` / `"idun"` kinds to their respective `connect_emotiv` / `connect_idun` factory functions.

- **Emotiv & IDUN dashboard UI incomplete**: Added `isEmotiv` / `isIdun` capability flags to the dashboard, device images, alt text, battery visibility for Emotiv, and device-specific scanning message for Emotiv Cortex API connections.
- **ElectrodeGuide missing Emotiv & IDUN tabs**: Added Emotiv EPOC (14-ch) and IDUN Guardian (1-ch) tabs with correct electrode positions to the 3D electrode guide.
- **MN8 earbuds not detected in frontend**: `deviceCapabilities()` was missing the `mn8` prefix for Emotiv MN8 earbuds.

- **Emotiv/IDUN channel labels defaulted to Muse 4-ch**: Dashboard channel labels and colors now correctly show 14 Emotiv EPOC channels or 1 IDUN Guardian channel instead of falling back to the Muse TP9/AF7/AF8/TP10 labels.
- **ElectrodePlacement SVG missing Emotiv & IDUN**: Added 14-electrode Emotiv EPOC and single-electrode IDUN Guardian presets to the 2D electrode placement diagram with correct 10-20 positions.
- **Device info badge only shown for Ganglion**: Non-Muse device info badges (channel count, sample rate) now also appear for Emotiv, IDUN, and Hermes connections.
- **EEG expanded grid always 2-column**: The expanded EEG channel grid now adapts columns to the channel count (2 for ≤4ch, 3 for 5-8ch, 4 for >8ch) instead of being hardcoded to 2 columns except MW75.

- **Session runner re-reads pipeline channels after Emotiv auto-detection**: When Emotiv auto-detects the actual channel count (Insight 5-ch, MN8 2-ch vs assumed EPOC 14-ch), the session runner now picks up the updated `pipeline_channels` on the first EEG frame. Previously the snapshot was taken before any events arrived, so the DSP pipeline would process 14 channels even when only 5 were active.

- **MW75 reconnects to the correct device**: `connect_mw75` now uses `scan_all()` + `connect_to(device)` with `preferred_id` matching, so reconnection targets the previously-paired headphone instead of picking the first MW75 found.
- **Hermes reconnects to the correct device**: Same fix as MW75 — `connect_hermes` now uses `scan_all()` + `connect_to(device)` with `preferred_id` matching.
- **Emotiv CSV has correct channel header**: CSV creation is now deferred until the first EEG frame arrives, after Emotiv's channel auto-detection (DataLabels) has resolved the actual channel count. Previously the CSV was opened with 14-column EPOC headers even when an Insight (5-ch) was connected.

- **Fix screenshot capture on Linux (Wayland) and Windows**: Replaced shell-command-based screenshot capture (`xdotool`, `import`, `scrot`, `grim`, `swaymsg` on Linux; PowerShell `CopyFromScreen` on Windows) with the `xcap` crate. This provides native, dependency-free screen capture on both X11 and Wayland (via PipeWire) on Linux, and native Win32/WGC capture on Windows. Includes a dark-frame detection guard that skips all-black captures.

- **Downloaded LLM model auto-activates**: When the first model finishes downloading (or the current active model is missing/deleted), the newly downloaded model is automatically set as active. Previously only activated when `active_model` was empty — a stale reference to a deleted model prevented auto-selection.
- **"Start LLM Engine" auto-selects a model**: When the user clicks Start and no model is active (or the active model file is missing), the engine now automatically picks the first available downloaded model, activates it, and proceeds with loading. Previously it would fail with a generic error asking the user to manually click "Use".

- **Metrics CSV/Parquet header now uses actual channel names**: The metrics header was hardcoded to 4 Muse channels (TP9/AF7/AF8/TP10 × 12 bands = 48 columns). For MW75 (12-ch) or Emotiv (14-ch), the data row had more band-power columns than the header, corrupting CSV alignment. Added `build_metrics_header(channel_names)` that generates the correct per-channel band columns dynamically. Both CSV and Parquet metrics writers now use it.
- **Metrics reader detects channel count from header**: `load_metrics_csv` and `load_metrics_from_parquet` now find the "faa" column position to compute the cross-channel index offset dynamically. Previously hardcoded to column 49 (4 channels × 12 bands + 1), causing all index values to read from wrong columns for non-4-channel devices.

- **QualityMonitor window size now matches device sample rate**: Added `QualityMonitor::with_window(channels, window)` and wired it to use the device's EEG sample rate (≈1 second window). Previously the window was hardcoded to 256 samples — only 0.51 s at 500 Hz (MW75) or 2 s at 128 Hz (Emotiv), causing quality to be assessed over inconsistent time windows.
- **HeadPoseTracker IMU rate now configurable**: Added `HeadPoseTracker::with_imu_rate(hz)`. Gyro integration (`dt`), stillness EMA, gesture window, and refractory period all used the hardcoded Muse IMU rate (52 Hz). At different rates, `dt = 1/52` would produce wrong pitch/roll/yaw accumulation and incorrect stillness scores.

- **Fix screenshot service mock data**: Fixed GPU stats mock to use 0–1 fractions instead of raw integers (was showing 4000% instead of 30%). Fixed `get_csv_metrics` timeseries to use correct abbreviated `EpochRow` field names (`med`, `cog`, `drow`, `sc`, `mu`, `ha`, `hm`, `hc`, etc.) so session charts render properly. Fixed `get_sleep_stages` mock to return `{ epochs: [], summary: null }` instead of `null` to prevent crashes in the compare page.

- **Fix dashboard light screenshot empty**: Added a warm-up step before taking the first screenshot so Vite finishes compiling, and increased wait time for the dashboard page to ensure Svelte fully bootstraps before capture. Dashboard is now also captured as full-page.

- **Fix search EEG screenshot empty**: The `stream_search_embeddings` mock now properly sends streaming results through the Tauri Channel by extracting the Channel's callback ID and delivering `started`, `result`, and `done` messages with realistic neighbor data including labels and metrics.

- **Fix search images broken thumbnails**: Broken `<img>` elements (pointing to non-existent local screenshot server) are now replaced with coloured placeholder SVGs that mimic app windows (VS Code, Firefox, Terminal) after search results render. Removed duplicate search-images handler that was re-triggering search and overwriting the placeholder replacement.

- **Session metadata hardcoded Muse values**: `write_session_meta` wrote `"sample_rate_hz": 256`, `"channels": ["TP9","AF7","AF8","TP10"]`, and `"channel_count": 4` for all devices. Now uses actual device values from `DeviceStatus` (set at session start). Recordings from MW75, Emotiv, Hermes, IDUN, and Ganglion had incorrect metadata in the JSON sidecar file.
- **Ganglion connected at wrong sample rate**: `connect_ganglion` passed `EEG_SAMPLE_RATE` (256 Hz) to `OpenBciAdapter::make_descriptor`, but Ganglion runs at 200 Hz. Added `GANGLION_SAMPLE_RATE` (200 Hz) constant and use it instead. This caused the entire DSP pipeline (filter, band analyzer, artifact detector) to use 256 Hz for a 200 Hz device.
- **Missing constants in prelude**: Added `GANGLION_SAMPLE_RATE`, `GANGLION_CHANNEL_NAMES`, `HERMES_*`, and `MW75_*` constants to the `skill-constants` prelude for easier access across crates.

- **⚠️ BREAKING: `muse-status` event renamed to `status`**: The Tauri IPC event and WebSocket broadcast event `muse-status` has been renamed to `status` to reflect its device-agnostic nature. All frontend listeners, the WS server, and documentation have been updated. **External WS clients that subscribe to `muse-status` must update to `status`.**

### Refactor

- **Unified `exg_` session file convention**: New recordings use `exg_<timestamp>.csv` and `exg_<timestamp>.json` for all devices, replacing the Muse-only `muse_` prefix. Full backward compatibility: the history loader, session analysis, embedding search, and settings commands all accept both `exg_` and legacy `muse_` files.

- **`detect_device_kind` delegates to `DeviceKind::from_name`**: Eliminated duplicated device-name matching constants in `lifecycle.rs`. All device-name detection now flows through `skill-data::device::DeviceKind::from_name`.

- **DeviceKind type updated**: Added missing `"ganglion"`, `"mw75"`, and `"hermes"` variants to the TypeScript `DeviceKind` union so it matches all backend device kinds.
- **Stale JSDoc**: Updated `device_kind` field comment in `types.ts` to reference `DeviceKind` instead of listing an incomplete set.

- **Device config: Rust as single source of truth**: Eliminated duplicate device definitions between Rust and Svelte. `crates/skill-data/src/device.rs` is now the canonical source for device families, capabilities (channel count, PPG, IMU, central electrodes, full montage, sample rate, electrode names), and the supported-devices catalog (companies, models, images, instruction keys). The Svelte frontend receives capability flags via `DeviceStatus` fields (`has_ppg`, `has_imu`, `has_central_electrodes`, `has_full_montage`) and fetches the supported-devices catalog via the new `get_supported_companies` Tauri command. `src/lib/device.ts` is now a thin type-only wrapper; `src/lib/supported-devices.ts` loads data from Rust at startup. The old `detect_device_kind` in `lifecycle.rs` now delegates to `DeviceKind::as_str()`.

- **Split i18n into namespace files**: Replaced monolithic ~3000-line locale files (`en.ts`, `de.ts`, `fr.ts`, `he.ts`, `uk.ts`) with 15 namespace-based files per locale under `src/lib/i18n/<locale>/` (`common`, `dashboard`, `settings`, `search`, `calibration`, `history`, `hooks`, `llm`, `onboarding`, `screenshots`, `tts`, `perm`, `help`, `help-ref`, `ui`). Each locale folder has a barrel `index.ts` that merges all namespaces.
- **Added `TranslationKey` type safety**: Generated `keys.ts` with a union type of all 2731 valid translation keys. The `t()` function now accepts `TranslationKey` for compile-time checking on static keys, with a `string` overload for dynamic/computed keys.
- **Extracted shared `i18n-utils.ts`**: Moved duplicated `extractKeys()` logic from `sync-i18n.ts` and `audit-i18n.ts` into a shared `src/lib/i18n/i18n-utils.ts` module.
- **Updated i18n tests**: Test suite now validates per-namespace file consistency (74 tests, all passing).
- **Updated scripts**: `sync-i18n.ts` and `audit-i18n.ts` now operate on the new directory structure and use shared utilities.

### Build

- **Decouple skill-settings from skill-screenshots**: Moved `ScreenshotConfig` struct and its pure helper methods from `skill-screenshots` into `skill-settings`, breaking the transitive `skill-settings` → `skill-screenshots` → `xcap` → `pipewire` dependency chain. Crates like `skill-router`, `skill-history`, and `skill-settings` no longer require `libpipewire-0.3` to compile. The `fastembed_model_enum()` helper stays in `skill-screenshots` as a standalone function since it depends on the `fastembed` crate. `skill-screenshots` now imports `ScreenshotConfig` from `skill-settings` instead of owning it.

- **Feature-gate `xcap` in skill-screenshots**: Added a `capture` feature (default on) that gates the `xcap` dependency. The top-level binary sets `default-features = false` so `cargo clippy` works without `libpipewire-0.3` on dev machines. Screen capture gracefully returns `None` when the feature is disabled. CI and release builds work unchanged since pipewire is installed there.

### i18n

- **Emotiv scanning message**: Added `dashboard.connectingEmotiv` key in all five languages (en, de, fr, uk, he).

- **Replaced `\u` escape sequences with UTF-8 characters**: Converted 11 `\uXXXX` escapes in de, fr, he, uk i18n files to native UTF-8 per the project encoding rule.

### Dependencies

- **Added `xcap` 0.9 for Linux and Windows**: Cross-platform screen capture library replacing external CLI tool dependencies.

## [0.0.42] — 2026-03-18

### Features

- **Per-device API credentials**: added persistent device API configuration with a dedicated Emotiv Cortex client ID/secret editor in both Devices and Settings tabs.
- **Emotiv connection integration**: Emotiv connection now reads credentials from settings first, with environment variables as fallback.

- **Emotiv device support**: Added `EmotivAdapter` for Emotiv EPOC X, EPOC+, Insight, and Flex headsets via the Cortex WebSocket API (JSON-RPC 2.0). Streams EEG (up to 14 ch @ 128 Hz), motion/IMU, and battery data through the unified session runner.
- **IDUN Guardian device support**: Added `IdunAdapter` for the IDUN Guardian in-ear EEG earbud over BLE. Streams single-channel bipolar EEG (1 ch @ 250 Hz), 6-DOF IMU (accelerometer + gyroscope), and battery data.
- **Device constants**: Added hardware constants for Emotiv (EPOC 14-ch, Insight 5-ch, 128 Hz sample rate, channel labels) and IDUN Guardian (1-ch, 250 Hz, channel label) to `skill-constants`.
- **DeviceKind::Idun**: Extended `DeviceKind` enum in `skill-data` with the `Idun` variant, capability flags, and name-based detection (`idun`, `ige`, `guardian` prefixes).
- **TypeScript device layer**: Added `"idun"` to the `DeviceKind` union and `IDUN_CAPS` capability table in `device.ts` for UI-side device detection.

- **IDUN per-device API token**: extended device API configuration with an `IDUN` token field and UI editors in Devices and Settings tabs.
- **IDUN connector integration**: when configured, the IDUN token is applied to `IDUN_API_TOKEN` before connection, so cloud-decoding paths can consume it.

- **RE-AK device image**: `deviceImage()` in both DevicesTab and SettingsTab now maps Nucleus-Hermès BLE names (`hermes`, `nucleus`, `re-ak`, `reak`) to the correct device image.

### Performance

- **Improve Windows CI build caching**: stabilize Rust cache reuse with a shared `rust-cache` key and enable persisted GitHub Actions `sccache` backend to increase cache-hit rates and reduce repeated full recompiles.
- **Add compile cache diagnostics**: print `sccache --show-stats` before and after Rust compile in the Windows release workflow to make cache effectiveness visible in job logs.

- **Faster Windows builds with lld-link**: Auto-detect LLVM's `lld-link` linker on Windows (both CI and local dev via `tauri-build.js`), replacing the slower MSVC `link.exe`. Combined with the previously split compile/package CI steps, this should significantly reduce Windows release CI time.

### Bugfixes

- **Chart refs reactivity**: Declared `chartEl` and `bandChartEl` with `$state(...)` in `+page.svelte` to fix `non_reactive_update` warnings.

- **Device API save error handling**: `saveEmotivApi` and `saveIdunApi` now wrap the Tauri invoke in `try/catch`; failed saves surface an inline error message instead of silently flashing "Saved".
- **IDUN token env var safety**: replaced `std::env::set_var("IDUN_API_TOKEN", …)` (process-wide mutation, unsafe in multi-threaded Rust) with passing the token via `GuardianClientConfig::api_token`.

- **Restore ExgTab compatibility path**: added `src/lib/ExgTab.svelte` as a compatibility shim to prevent missing-module 404 requests.

- **Fix `app_log` ambiguity in lifecycle.rs**: Removed erroneous `app_log` item import from `crate` that conflicted with the `app_log!` macro defined in `lib.rs`. Replaced unused `Emitter` import with `Manager` to provide the `.state()` method needed by the macro.

- **Fix `app_log!` ambiguity in lifecycle module**: remove incorrect `app_log` item import from `src-tauri/src/lifecycle.rs` and clean unused Tauri imports in lifecycle/scanner modules so the Tauri crate compiles cleanly.

- **Enable GPU acceleration in Linux and macOS release builds**: Added missing `--features llm-vulkan` to the Linux CI cargo build and `--features llm-metal` to the macOS CI cargo build, ensuring GPU-accelerated LLM inference is included in release binaries (matching what `tauri-build.js` injects for local builds).

- **Windows CI: enable llm-vulkan feature**: Add missing `--features llm-vulkan` to the Windows release cargo build command, ensuring Vulkan GPU offloading for LLM inference is included in release builds (matching what `tauri-build.js` injects locally).

- **Fix `llama-cpp-4` version mismatch**: Non-macOS platforms used `0.2.10` while macOS used `0.2.12`; aligned to `0.2.12` everywhere.
- **Fix `package.json` dead script**: Removed duplicate `taur:build:win:nsis` key (typo missing `i`).
- **Replace silent `catch {}` in chat page**: All 18 empty catch blocks in `chat/+page.svelte` now log warnings/errors to the console instead of silently swallowing failures.

### Refactor

- **Extract lifecycle and quit modules from lib.rs**: Moved session lifecycle (start/cancel/disconnect/reconnect backoff) into `lifecycle.rs` and quit-confirmation dialogs into `quit.rs`, reducing `lib.rs` from 1,778 to 1,495 lines.
- **Add `DeviceStatus::reset_disconnected()` method**: Replaces 15+ manual field resets in `go_disconnected` with a single method call, preventing missed fields when new status fields are added.
- **Consolidate mutex lock acquisitions in `setup_app`**: Merged 4 separate lock/unlock cycles for LLM autostart, embedding model, model status, and HF repo into a single critical section.
- **Extract device-kind detection into constants**: Replaced inline string matching (`starts_with("ganglion")`, `contains("mw75")`) with named constants and a `detect_device_kind()` function with unit tests.

- **Remove duplicated sections from Settings tab**: Removed Muse Devices, OpenBCI, Signal Processing, EEG Embedding, and GPU/Memory sections from SettingsTab since they now live in the dedicated Devices and EXG tabs. The Settings tab now only contains Activity Tracking, Logging, Data Directory, and WebSocket Server settings.

- **Standardize device image naming**: renamed device assets to consistent vendor-prefixed names (`muse-*`, `openbci-*`, `emotiv-*`, `idun-*`) and updated all UI references accordingly.

- **Shared supported devices config**: extracted Supported Devices company/device/instruction definitions into a single shared module used by both Devices and Settings tabs.

### Build

- **Add `rustfmt.toml` and `clippy.toml`**: Added formatting and lint configuration for consistent Rust code style.

### UI

- **Devices settings tab**: Dedicated "Devices" tab (second in Settings sidebar) showing paired and discovered BCI devices with pair/forget/set-default actions.
- **EXG settings tab**: New "EXG" tab (third in Settings sidebar) for OpenBCI board configuration, signal processing filters (notch, high/low-pass), EEG embedding pipeline, and GPU/memory stats.

- **Fold Device API by company**: Device API settings are now grouped into collapsible company sections (`Emotiv Cortex`, `IDUN Cloud`) in both Devices and Settings tabs.

- **No visual behavior change**: existing device thumbnails continue to render, now via standardized local file names.

- **Device images**: Use `object-cover` so device thumbnails and previews fill their containers fully.

- **Add MN8 and X-Trodes device photos**: device cards now map `MN8` to `/devices/emotiv-mn8.webp` and `X-Trodes`/`Xtrodes` names to `/devices/emotiv-x-trodes.webp`.

- **Add model-specific Emotiv device photos**: device cards now map `EPOC`/`Emotiv` to `/devices/emotiv-epoc-x.webp`, `Insight` to `/devices/emotiv-insight.webp`, and `Flex` to `/devices/emotiv-flex-saline.webp`.

- **Use provided IDUN and Emotiv photos**: device rows now show the supplied IDUN Guardian and Emotiv Epoc images in Devices and Settings tabs instead of the generic fallback icon.

- **Add IDUN token helper link**: added a direct link to the IDUN dashboard in Device API settings to make token setup easier.

- **Localize IDUN and Emotiv device images**: switched device-card image sources to bundled local files (`/devices/idun-guardian.png` and `/devices/emotiv-epoc.png`) to avoid runtime dependence on external hosts.

- **RE-AK Nucleus-Hermès asset**: replaced the temporary Hermes placeholder with the official RE-AK image and bundled it as a local static asset.
- **Device naming correction**: updated Supported Devices labeling and instructions to use the proper spelling `Nucleus-Hermès`.

- **Save per API provider section**: Device API settings now have separate Save buttons inside each folded company section (`Emotiv Cortex`, `IDUN Cloud`) in both Devices and Settings tabs.

- **Company mapping correction**: moved `MW75 Neuro` from `Muse` to a dedicated `Neurable` section in Supported Devices.
- **RE-AK support card**: added `RE-AK` with `Hermes Nucleus` to Supported Devices and included a local bundled image asset.

- **Supported devices section**: added a compact Supported Devices section in Devices and Settings tabs that lists supported companies and device images.
- **Expandable connection guidance**: clicking a company name or any device image now expands brief connection instructions and opens the related settings panel (OpenBCI, Emotiv, IDUN).

- **Supported device card alignment**: refined Supported Devices grid spacing and card sizing for a cleaner, more consistent layout.
- **Uniform image framing**: all device images now render inside fixed-size white background frames with centered `object-contain` scaling to normalize mixed aspect ratios.

- **Supported Devices search**: Add fuzzy search to filter devices and companies by name in the Supported Devices section.
- **Supported Devices layout**: Reduce spacing and heights to make the grid more compact (smaller gaps, smaller thumbnails, smaller text).

### i18n

- **Device API section fully translated**: all hardcoded strings in the Device API card (section title, provider titles, descriptions, field labels, show/hide, save/saved) are now driven by `t()` keys under `settings.deviceApi.*`.

- **Supported Devices localization**: moved Supported Devices section title, company labels, device names, and connection instructions to i18n keys and replaced hardcoded strings in both tabs.

## [0.0.41] — 2026-03-17

### Features

- **Agent Skills discovery**: Added `skill-skills` crate that discovers `SKILL.md` files from `~/.skill/skills/` (user), `<cwd>/.skill/skills/` (project), and the bundled `skills/` git submodule. Discovered skills are injected into the LLM system prompt as an `<available_skills>` XML block so the model can load specialised instructions via `read_file` on demand. Skills require a `description` in YAML frontmatter; invalid index-style `SKILL.md` files (without description) allow recursion into subdirectories. Deduplication by name (first wins) and symlink real-path. Added `skills` git submodule from `https://github.com/NeuroSkill-com/skills.git` providing 10+ bundled EEG/protocol skills.

- **CLI: screenshot search commands**: Added `search-images` and `screenshots-around` CLI commands. `search-images "query"` searches screenshots by OCR text in semantic (embedding HNSW) or substring (SQL LIKE) mode. `screenshots-around --at <utc>` finds screenshots near a given timestamp within a configurable window. Both commands support `--json`, `--full`, and `--k` flags. Also added the corresponding `search_screenshots` and `screenshots_around` WebSocket/HTTP commands to the server dispatcher.

- **Configurable SNR exit threshold for DND**: The SNR level below which focus mode is forcibly deactivated is now a user setting (`snr_exit_db`) instead of a hardcoded constant. Default changed from 5 dB to 0 dB so DND only exits when the signal is completely lost. A new preset picker in the DND settings UI lets users choose 0 / 3 / 5 / 10 / 15 dB. Translations added for EN, DE, FR, UK, HE.

- **Context breakdown inspector**: Click the context usage ring in the chat header to open a popover showing the proportional breakdown of context window usage — system prompt, EEG context, tool definitions, user messages, assistant messages, thinking/reasoning, tool results, and current completion tokens. Includes a stacked bar visualization and detailed legend with token counts and percentages. Localized in all 5 languages.

- **skill-headless: Headless / Headful modes**: Replaced the `visible: bool` flag with a `Mode` enum (`Mode::Headless` and `Mode::Headful`). Headless mode positions the window off-screen so nothing is ever shown to the user while still giving the webview real pixel dimensions. Headful mode shows the window on-screen for debugging, demos, or interactive automation. In headless mode, `SetViewport` ensures the window stays off-screen after resize.

- **skill-headless: network interception**: Added request/response interception support. `EnableInterception` monkey-patches `fetch()` and `XMLHttpRequest` to capture all HTTP traffic. Navigation events are recorded via wry's navigation handler. `SetBlockedUrls` blocks navigations matching URL substring patterns. `GetInterceptedRequests` retrieves the full network log (requests, responses, navigations) with optional clear-on-read. Includes 11 tests covering fetch GET/POST, XHR with custom headers, navigation capture, URL blocking, and log clearing.

- **HealthKit data ingestion endpoints**: Added HTTP REST and WebSocket endpoints to receive Apple HealthKit data from a companion iOS app. New endpoints: `POST /v1/health/sync` (idempotent batch upsert of sleep, workouts, heart rate, steps, mindfulness, and generic metrics), `POST /v1/health/query` (query by type and time range), `GET /v1/health/summary` (aggregate counts), `GET /v1/health/metric_types` (list stored metric types). Data is stored in `~/.skill/health.sqlite` via a new `health_store` module in `skill-data`. All endpoints are also available as WS commands (`health_sync`, `health_query`, `health_summary`, `health_metric_types`).

- **Standalone LLM logger**: Added `skill_llm::log` module with pluggable callback sink (`set_log_callback`) and `llm_log!` macro. All `eprintln!("[llm] ...")` / `eprintln!("[chat_store] ...")` calls in `engine.rs` and `chat_store.rs` now route through the unified logger. On the Tauri side, `llm::init_llm_logger()` wires LLM output through `SkillLogger` so the `llm` and `chat_store` subsystem toggles in log config control visibility. Added `llm` and `chat_store` fields to `LogConfig`, new rows in the Settings logging grid, and i18n keys for all five locales (en, de, fr, uk, he). Also fixed the TtsTab `LogConfig` interface to include the missing `hooks` field.

- **LLM auto-start on launch**: Added `autostart` field to `LlmConfig`. When enabled + a model is downloaded and selected, the LLM server starts automatically during app setup with a 500ms delay to let the UI render first. Toggle added to the LLM settings tab.
- **Atomic model switch**: New `switch_llm_model` Tauri command that atomically stops the running server, waits for full shutdown, sets the new active model, and starts the new one — eliminating the fragile 150ms sleep race in the frontend.
- **Abort feedback in chat**: The stop/abort button now shows a spinner and "Aborting…" state while the abort is in flight, and is disabled to prevent double-clicks.
- **Context window warning**: A warning banner appears above the chat input when context usage exceeds 85% (amber) or 95% (red), showing the current usage percentage.
- **Per-session generation params**: Temperature, max tokens, top-k, top-p, and thinking level are now saved per chat session (new `params` column in `chat_sessions` table). Params auto-save on change (debounced 500ms) and restore when switching sessions.
- **Regenerate button**: Hover over the last assistant message to see a "Regenerate" button that removes the response and re-sends the last user message with current params.
- **Edit & resend on user messages**: Hover over any user message to see "Edit & resend" which populates the input with that message's text and removes all subsequent messages.
- **Live tok/s indicator**: During streaming, a live tokens-per-second counter is shown below the assistant message. After completion, the final tok/s is included in the timing line alongside TTFT and token counts.
- **Open LLM settings from chat**: The empty chat state (when server is stopped) now includes an "Open LLM settings" button alongside "Start server".
- **Reduced settings panel height**: Chat settings and tools panels reduced from 50vh to 40vh max to leave more room for the message list on small screens.

- **Best-result scoring for web search**: Rendered page content is now scored by text quality (word count, presence of numbers/data indicators like temperatures and percentages, uniqueness of words) with penalties for CSS/JS garbage. Only the best 1-2 results are included in the compact output instead of all 5, giving the LLM focused, high-quality content to summarize.

- **Configurable web search provider**: the `web_search` tool now supports three backends — **DuckDuckGo** (default, no API key), **Brave Search** (free tier: 2,000 queries/month with API key), and **SearXNG** (self-hosted instance URL). A new `WebSearchProvider` config struct holds the backend choice, Brave API key, and SearXNG URL. Each backend falls back to DuckDuckGo HTML scraping if it fails.
- **Search provider UI**: added a backend selector (DuckDuckGo / Brave / SearXNG) to the Tools settings tab, with conditional API key and URL inputs.

- **Skill API tool for LLM chat**: Added a built-in `skill` tool that gives the LLM direct access to the full NeuroSkill WebSocket API. The LLM can now query device status, list sessions, create labels, search EEG embeddings, manage hooks, control DND, run calibrations, and more — all without requiring the user to copy-paste data. The tool connects to the local HTTP API server and supports all commands from the CLI (status, sessions, session_metrics, label, search_labels, interactive_search, search, compare, sleep, say, notify, calibrate, timer, hooks, dnd, calibrations CRUD, umap, and read-only LLM management). Dangerous LLM self-management commands (start/stop/delete/select) are blocked for safety. Enabled by default via the `skill_api` toggle in Settings → LLM → Tools.

- **skill-headless crate**: New headless browser engine providing a CDP-like command API over wry/tao. Supports navigation, JS evaluation, DOM queries, CSS/script injection, cookie and storage management, viewport emulation, cache clearing, screenshots (canvas-based DOM walker), and element wait primitives — all driven from any thread via a channel-based command/response protocol over an off-screen system webview. Includes 29 advanced tests covering HTML rendering (background, font, flexbox, grid, positioning), PNG screenshots at multiple sizes, viewport resizing (320x240 to 1920x1080), WebGL/WebGL2 context creation and shader compilation, and custom user-agent injection.

- **Sleep schedule settings**: Added a new "Sleep" section in Settings with configurable bedtime and wake-up time. Includes five presets (Default 23:00–07:00, Early Bird 21:30–05:30, Night Owl 01:00–09:00, Short Sleeper 00:00–06:00, Long Sleeper 22:00–08:00), a 24-hour clock visualization, and duration summary. Sleep window is persisted and can be used for session classification and sleep staging analysis.

- **Tool-call logging toggle in Settings**: Added a `tools` flag to the logging configuration so users can enable/disable tool-call logging from Settings. The toggle controls the `skill-tools::log` subsystem which traces tool invocations, safety approvals, completion times, and errors. Wired into the central `SkillLogger` with `init_tool_logger` callback and `set_tool_logging` runtime toggle. Added i18n strings for en, de, fr, uk, he.

- **Separate tool-call logger**: Added a standalone pluggable logger (`skill-tools::log`) for tool-call tracing, following the same pattern as `skill-llm::log`. Logs tool invocations (name + args), completion times, safety approval events, and errors. Use `set_log_callback` to route output to the app logger and `set_log_enabled` to toggle at runtime. The `tool_log!` macro short-circuits formatting when logging is disabled.

- **Configurable tool context compression**: Added a new "Context compression" setting (Off / Normal / Aggressive) in Settings → LLM → Tools that controls how tool results are compressed before being injected into the conversation context. Normal mode caps web search results to 5, truncates long URLs, and compresses old tool results. Aggressive mode uses tighter limits for small context windows. Custom overrides for max search results and max result characters are available when compression is enabled.

- **Configurable tool hop limits**: Added UI controls for `max_rounds` (how many think→tool→think cycles per message) and `max_calls_per_round` (how many tools per cycle) in both the LLM Settings tools card and the Chat window tools panel. Preset buttons (1/3/5/10 for rounds, 1/2/4/8 for calls) with active highlight. i18n keys added for all 5 locales.

- **Tools master toggle**: Added an `enabled` master switch to the LLM tool configuration that disables all tools at once. When turned off, no built-in tools are available to the LLM regardless of individual tool toggles. The toggle appears in both the Settings LLM tab and the Chat tool panel, with i18n support for all languages (en, de, fr, uk, he).

- **Standalone TTS logger**: Added `skill_tts::log` module with pluggable callback sink (`set_log_callback`) and `tts_log!` macro. All `eprintln!("[tts] ...")` / `eprintln!("[neutts] ...")` calls in `kitten.rs`, `neutts.rs`, and `lib.rs` now route through the unified logger. On the Tauri side, `tts::init_tts_logger()` wires TTS output through `SkillLogger` so the `tts` subsystem toggle in log config controls TTS log visibility. Logging is enabled by default and can be toggled at runtime via `set_log_enabled`.

- **WebSocket & HTTP chat persistence**: Chats initiated via the WebSocket `llm_chat` command and `POST /llm/chat` endpoint are now persisted to the same SQLite chat store used by the Chat window. User and assistant messages (including tool calls) are saved to `chat_history.sqlite`, making them visible in the Chat window sidebar and recoverable across restarts. A `session_id` field is returned in both the WS `session` frame and the `done`/response payload, and callers can pass `session_id` in subsequent requests to continue an existing conversation. New sessions are auto-titled from the first user message.

- **Headless browser rendering in web_fetch tool**: Added `render` parameter to the `web_fetch` LLM tool. When `render=true`, pages are loaded in a headless browser (via `skill-headless`) that executes JavaScript, enabling content extraction from SPAs and dynamically rendered pages. Supports optional `wait_ms`, `selector` (CSS selector to wait for), and `eval_js` (custom JS to evaluate) parameters.

- **Headless browser rendering in web_search tool**: Added `render` and `render_count` parameters to the `web_search` LLM tool. When `render=true`, the top N search result URLs are visited in a headless browser and their rendered text content is included in the results under a `rendered_text` field, giving the LLM access to full page content including JS-rendered material.

### Performance

- **Parallel URL fetching for web search**: When `render=true`, all search result URLs are now fetched concurrently using scoped threads instead of sequentially. Total fetch time equals the slowest single URL rather than the sum of all. This typically reduces render=true latency from 10-15s to 3-5s for 3 URLs.

- **Drop full Linux portable build from CI**: Removed the `linux-portable-package` job from `ci.yml` so Linux CI only runs `cargo check` + `clippy` (matching Windows). The full release build and packaging are already covered by the dedicated `release-linux.yml` workflow. This dramatically reduces CI wall-clock time on every push and PR.

- **Screenshot duplicate detection**: when a new screenshot is identical to the previous one (same resized-PNG hash), the embed thread now copies the vision embedding, OCR text, and OCR text embedding from the previous row instead of re-running the vision encoder, OCR engine, and text embedder. This eliminates redundant GPU/CPU inference when the screen content hasn't changed (e.g. idle desktop, paused video).

### Bugfixes

- Fixed trailing garbage bytes in `crates/skill-tools/src/types.rs` that could cause a compilation failure.

- **Remove broken public SearXNG instance scraping**: public SearXNG instances universally block automated API access with 429 rate limits or anti-bot captchas. Removed the background instance list fetcher and random instance selection. SearXNG now requires a user-provided self-hosted instance URL.

- **Add tests to previously untested crates**: Added 35 new unit tests across 4 crates that had zero test coverage:
  - `skill-exg` (17 tests): cosine distance (identical, opposite, orthogonal, edge cases), fuzzy matching (exact, case-insensitive, substring, typo, empty), Levenshtein distance.
  - `skill-commands` (13 tests): DOT escaping, SVG escaping, text truncation, turbo colormap, graph generation.
  - `skill-eeg/band_metrics` (10 tests): spectral edge frequency, spectral centroid, Hjorth parameters, permutation entropy, sample entropy, DFA exponent, Higuchi fractal dimension.
  - `skill-history/cache` (5 tests): timeseries downsampling (noop, exact count, endpoint preservation, min-2), sleep stage analysis.

- **Web search no longer stalls after returning URLs**: Improved `web_search` tool description to instruct the LLM to use `render=true` for factual/current-data queries (weather, prices, scores, news). When `render=false`, the tool result now includes a follow-up hint telling the model to fetch page content. Added a weather example to the system prompt so the model learns the correct pattern.
- **Context window no longer fills up during multi-step tool chains**: The orchestration loop now condenses prior-round tool results to one-line summaries after each round (e.g. `[location: Boston, MA, US (America/New_York)]`, `[web_search: 5 results for "weather Boston"]`). The model already consumed those results and chose its next action, so the full content is no longer needed. This frees ~200-500 tokens per prior round, allowing 3-4 step chains (location → search → fetch → answer) to complete even on 4 K context models. Additionally, `web_search` returns compact text instead of verbose JSON, `web_fetch` is capped to configured limits, and headless-rendered page text is reduced from 4 K to 2 K chars per URL.

- **Accent color consistency**: All UI elements now honor the Appearance accent setting. Replaced hardcoded `oklch(0.58 0.24 293)` violet values in the markdown renderer CSS with `var(--color-violet-*)` tokens, converted every `purple-*` Tailwind class to the remapped `violet-*` family, and switched inline-style hex accent colors (`#8b5cf6`, `#a855f7`, `#c084fc`) to CSS custom properties across dashboard gauges, focus timer, compare page, and EEG indices.

- **Rotating browser User-Agents**: replaced bot-like User-Agent strings with a pool of 10 realistic browser UAs (Chrome, Firefox, Safari, Edge on Windows/macOS/Linux) rotated on each request to reduce fingerprinting.
- **Fix DuckDuckGo HTML search**: mimic real form submission by adding `Origin` header, correct `Referer`, and the `b=` submit-button field. Fixed HTML parser to split on `class="result results_links"` (the actual outer wrapper) instead of `class="result__body"` which is now a multi-class attribute and no longer matches.
- **Extracted `parse_ddg_html`**: separated HTML parsing from HTTP fetching for testability. Added offline unit tests for result parsing and DDG redirect URL unwrapping.

- **Fix macOS headless build**: Removed non-existent `EventLoopBuilderExtMacOS` import and `with_any_thread` call — tao 0.34 does not gate event loop thread affinity on macOS.
- **Fix skill-screenshots build**: Added missing `GenericImageView` import, made `CapturedImage` fields `pub(crate)`, cfg-gated `Path` import, removed unused `CapturedImage` re-import in capture.rs.
- **Fix skill-llm warnings**: Removed unused imports in engine.rs and handlers.rs (`SystemTime`, `UNIX_EPOCH`, axum types, `GenParams`, `unix_ts_ms`, `HeaderMap`, `Sse`).
- **Fix SleepTab Svelte error**: Wrapped `{@const}` tags in `{#if true}` blocks — `{@const}` must be an immediate child of a block tag, not a raw element like `<svg>`.

- **Headless browser no longer crashes the app on macOS**: On macOS, tao requires the event loop on the main thread, but Tauri already owns it. Previously, the headless browser launch panicked with "EventLoop must be created on the main thread!" on a spawned thread, aborting the process. Fix: on macOS, `Browser::launch` is disabled via `set_unavailable()` at startup and an external renderer is registered that reuses Tauri's existing webview infrastructure. A hidden `WebviewWindow` is created with `on_page_load` detection — the renderer waits for the actual page load event instead of a fixed delay, with a 30-second timeout. The user can cancel the fetch at any time via the tool cancellation button. This gives full JS-rendered page content (weather widgets, SPAs, etc.) without a second event loop. On Linux/Windows the standalone headless browser continues to work as before.
- **Web search render=true no longer returns empty/error results**: When the external renderer times out or returns empty content for a URL, the system now automatically falls back to plain HTTP fetch + HTML tag stripping for that specific URL instead of propagating the error. This ensures the LLM always gets usable content and doesn't waste rounds retrying.

- **Fix LLM crash when prompt exceeds n_batch**: Long prompts triggered a fatal `GGML_ASSERT(n_tokens_all <= cparams.n_batch)` abort in llama.cpp, killing the entire process. The prompt is now decoded in chunks of `n_batch` tokens, preventing the native assertion failure.

- **Screenshot "sessions only" gate never re-engages after disconnect**: `session_start_utc` was set when scanning began but never reset to `None` in `go_disconnected`, so `is_session_active()` permanently returned `true` after the first connection attempt. Screenshots continued capturing even with no device connected. Now `session_start_utc` is always cleared on disconnect, including during auto-reconnect retries (no data is streaming, so it is not an active session).

- **Fix unused import warnings in skill-router / src-tauri**: Removed ~30 unused `use` statements from `src-tauri/src/lib.rs`, gated `std::sync::Mutex` import in `state.rs` behind `#[cfg(not(feature = "llm"))]`, removed unused `CalibrationProfile` import from `helpers.rs`, and converted doc comment on macro invocation in `window_cmds.rs` to a regular comment to silence `unused_doc_comments` warning.

- **Remove broken DuckDuckGo JSON API search**: the DuckDuckGo Instant Answer API (`api.duckduckgo.com`) has been deprecated and returns empty results for most queries, adding unnecessary latency. Removed it and now use HTML scraping directly as the sole search strategy.

- **Fix all clippy warnings**: Resolved redundant field name, unnecessary cast, unneeded return statement, and unused imports across the workspace. Zero clippy warnings remain.

- **DND focus mode now works on all devices**: OpenBCI and Hermes sessions were missing the Do Not Disturb tick logic (only Muse and MW75 had it). The shared `session_runner` now runs DND for every device that produces EEG band snapshots.

- **Battery alerts use `BatteryEma` from `skill-devices`**: Replaced two inline EMA implementations (Muse, MW75) with the existing `BatteryEma` struct, ensuring consistent smoothing and alert thresholds across devices.

### Refactor

- **Shared calibration CRUD service**: Extracted `calibration_service.rs` with `create_profile`, `update_profile`, `delete_profile`, `list_profiles`, and `get_profile` functions. Both the Tauri IPC commands (`window_cmds`) and the WebSocket API (`ws_commands`) now delegate to this single service, eliminating duplicated state mutation logic.

- **Extract LLM HTTP handlers from `engine.rs` (2,502 → 2,079 lines)**: Moved all axum HTTP handlers, auth helpers, and the router builder into a new `handlers.rs` (449 lines) in the `skill-llm` crate. `engine.rs` retains the inference actor, tool orchestration, and shared state.

- **Extract web search backends from `skill-tools/exec.rs` (1,551 → 944 lines)**: Moved DuckDuckGo HTML, Brave API, SearXNG, and headless fetch code into `search.rs` (616 lines).

- **Extract platform capture from `skill-screenshots/capture.rs` (1,527 → 1,145 lines)**: Moved macOS/Linux/Windows window capture and image decoding into `platform.rs` (392 lines).

- **Extract EEG band metrics from `skill-eeg/eeg_bands.rs` (1,510 → 1,222 lines)**: Moved advanced metric functions (SEF, Hjorth, entropy, DFA, consciousness indices) into `band_metrics.rs` (298 lines).

- **Chat page component extraction**: Split the monolithic `chat/+page.svelte` (2720 lines) into 9 focused modules — `ChatHeader`, `ChatSettingsPanel`, `ChatToolsPanel`, `ChatMessageList`, `ChatInputBar`, `ChatToolCard` components plus `chat-types.ts` and `chat-eeg.ts` utility modules. The main page is now 897 lines (67% reduction) with each extracted component under 400 lines.
- **Compare page logic extraction**: Extracted types, constants, helpers, insight computation, UMAP analysis, and all canvas drawing functions from `compare/+page.svelte` (2582 lines) into `compare-types.ts` and `compare-canvas.ts`. The main page is now 1924 lines (25% reduction) with 663 lines of pure logic in reusable modules.
- **History page helper extraction**: Extracted types, constants, date helpers, format utilities, label analysis, and grid constants from `history/+page.svelte` (2332 lines) into `history-helpers.ts` (201 lines). The main page is now 2224 lines.
- **Search page logic extraction**: Extracted types, helpers, colormap functions, analysis computation, graph deduplication, and kNN visualization from `search/+page.svelte` (2192 lines) into `search-types.ts` (302 lines). The main page is now 1935 lines (12% reduction).
- **UMAP viewer helper extraction**: Extracted pure math, color, timestamp, and geometry helpers from `UmapViewer3D.svelte` (1806 lines) into `umap-helpers.ts` (167 lines). The main component is now 1765 lines.
- **Deduplicated shared code**: Consolidated `BandSnapshot` type (single canonical source in `BandChart.svelte`, re-exported via `chat-types.ts`), `SESSION_COLORS` (moved to `constants.ts`, re-exported from `compare-types.ts` and `history-helpers.ts`), and `fmtSize`/`fmtGB`/`fmtBytes` format helpers (added to `format.ts`, removed 3 inline duplicates).

- **Deduplicate metrics JSON serialization in `DayStore`**: Extracted the ~60-field metrics-to-JSON serialization into a shared `metrics_to_json()` function, eliminating the copy-pasted logic between `insert()` and `insert_metrics_only()`. Reduces `day_store.rs` by ~65 lines.

- **Replace raw `SQLITE_OPEN_READ_ONLY` flags with `open_readonly()` helper**: Replaced 11 inline `rusqlite::Connection::open_with_flags(…, READ_ONLY)` calls across 7 files with the existing `skill_data::util::open_readonly()` helper for consistency with the workspace crates.

- **Deduplicate `MutexExt` trait**: Moved the poison-recovering `MutexExt` trait to `skill-constants` (zero-dependency crate). `skill-data` and `skill-jobs` now re-export from the single canonical definition instead of maintaining independent copies.

- **Add `AppStateExt` helper trait**: Introduced a blanket `AppStateExt` trait on `Manager<Wry>` that replaces the verbose `app.state::<Mutex<Box<AppState>>>()` pattern (137 call sites) with `app.app_state()`. Cleaned up newly-unused `Mutex` and `AppState` imports across 13 files.

- **Remove 14 re-export shim modules**: Eliminated one-line facade modules in `lib.rs` (e.g. `mod eeg_bands { pub use skill_eeg::eeg_bands::*; }`) that only proxied upstream crate items. All 59 call sites now reference the source crates directly (`skill_eeg::`, `skill_data::`) making dependencies explicit and reducing indirection.

- **Unified device session via `DeviceAdapter` trait**: Replaced four copy-pasted session modules (`muse_session.rs`, `mw75_session.rs`, `openbci_session.rs`, `hermes_session.rs` — 2,070 lines) with a trait-based architecture (1,970 lines). Added `DeviceAdapter` async trait, unified event types (`DeviceEvent`, `EegFrame`, `PpgFrame`, `ImuFrame`, `BatteryFrame`), and capability flags (`DeviceCaps`) to `skill-devices::session`. Each device has a small adapter (107–223 lines) that translates vendor events into the common vocabulary. A single generic event loop in `session_runner.rs` handles DSP, CSV, DND, battery, and emit for all devices.

- **Split `llm.rs` into module directory**: Refactored the 1537-line `src-tauri/src/llm.rs` into `src-tauri/src/llm/` with focused sub-modules: `mod.rs` (re-exports, logger, emitter), `cmds/catalog.rs` (catalog queries), `cmds/downloads.rs` (download lifecycle), `cmds/selection.rs` (model selection), `cmds/server.rs` (server lifecycle), `cmds/chat.rs` (chat persistence), `cmds/streaming.rs` (IPC streaming), `cmds/hardware_fit.rs` (hardware prediction). All public API paths remain unchanged.

- **Split `skill-llm/engine.rs` into module directory**: Refactored the 2070-line `crates/skill-llm/src/engine.rs` into `engine/` with focused sub-modules: `mod.rs` (re-exports, macros), `logging.rs` (log buffer, file sink), `protocol.rs` (wire types), `state.rs` (server state, cell, status), `think_tracker.rs` (think-budget enforcement), `images.rs` (base64 decoding), `tool_orchestration.rs` (multi-round tool loop), `sampling.rs` (token sampling loop), `generation.rs` (text/multimodal generation), `actor.rs` (OS thread event loop), `init.rs` (public init). All public API paths remain unchanged.

- **Split `eeg_embeddings.rs` (2,434 → 3 files)**: Removed 624 lines of dead code (`#[cfg(any())]` blocks). Split the remaining 1,810 lines into `mod.rs` (414 lines, public API + EegAccumulator), `day_store.rs` (372 lines, per-day HNSW + SQLite), and `worker.rs` (1,051 lines, background embed worker + hook matcher + weight helpers).

- **Split `ws_commands.rs` (2,417 → 3 files)**: Extracted `hooks.rs` (315 lines, hooks_get/set/status/suggest/log handlers) and `llm_cmds.rs` (418 lines, all LLM WebSocket commands), reducing `mod.rs` to 1,714 lines. The central `dispatch()` function remains in `mod.rs`.

- **Split `settings_cmds.rs` (1,789 → 3 files)**: Extracted `dnd_cmds.rs` (207 lines, Do Not Disturb automation commands) and `hook_cmds.rs` (274 lines, hook distance suggestion and audit log), reducing `mod.rs` to 1,341 lines.

- **Split `skill-commands` crate `lib.rs` (1,710 → 2 files)**: Extracted `graph.rs` (803 lines, DOT and SVG generation for interactive search results), reducing `lib.rs` to 917 lines.

- **Extract `chat-utils.ts` from `chat/+page.svelte` (2,945 → 2,720 lines)**: Moved pure utility functions (tool-call fence stripping, danger detection, assistant output parsing) into a standalone TypeScript module for testability and reuse.

- **Split `lib.rs` into `state.rs` + `helpers.rs`**: Extracted `AppState`, `DeviceStatus`, `LlmState`, IPC packet structs, and all `impl` blocks into `state.rs` (~410 lines). Extracted time helpers, emit/toast helpers, settings persistence, device upsert, and state access shortcuts into `helpers.rs` (~250 lines). `lib.rs` dropped from 2,217 to 1,557 lines (–660).

- **Renamed `MuseStatus` → `DeviceStatus`**: The status struct is used for all devices (Muse, MW75, Hermes, OpenBCI) — the old name was misleading. Updated all Rust and TypeScript/Svelte references. The Tauri event name `"muse-status"` is kept for backward compatibility with existing WS clients.

- **`window_cmd!` / `window_tab_cmd!` macros for window commands**: Replaced 10 boilerplate `open_*_window` Tauri commands (each 7 lines) with 2–3 line macro invocations. New windows now require a single `window_cmd!` call instead of a full `#[tauri::command] pub async fn` definition.

- **Extract `ws_commands/search.rs` (525 lines)**: Moved search_labels, search, compare, session_metrics, and interactive_search handlers out of `ws_commands/mod.rs`, reducing it from 1,687 to 1,162 lines.

- **Extract `skill-history/cache.rs` (751 lines)**: Moved disk cache, metrics computation, sleep staging, and batch loading out of `skill-history/lib.rs`, reducing it from 1,384 to 654 lines.

- **Deduplicate `yyyymmdd_utc()`**: Replaced the 29-line hand-rolled calendar calculation in `helpers.rs` with a one-line delegation to the canonical `skill_data::util::yyyymmdd_utc()`.

- **Added 25 tests for `skill-devices::session` adapters**: Covers channel accumulation (Muse alignment of per-electrode delivery, partial/complete frames, out-of-range electrodes), event translation (EEG, PPG, IMU, battery, connected, disconnected, activation-skipped, packets-dropped-skipped), synthetic connected injection (MW75 RFCOMM, OpenBCI), capability flags, descriptor construction, and pipeline channel capping. Total: 34 tests in `skill-devices` (up from 9).

- **Made adapter handle fields `Option`-based for testability**: `Mw75Adapter` and `HermesAdapter` handle fields are now `Option<Handle>` with `new_for_test()` constructors that pass `None`, avoiding unsafe `MaybeUninit` hacks. `OpenBciAdapter` gained `from_receiver()` for direct async channel injection without needing private `StreamHandle` fields.

### Build

- **clean:rust npm script**: Added `npm run clean:rust` to remove all Rust `target` directories (under `crates/` and `src-tauri/`).

- **Added `tokio`, `async-trait`, `bitflags` deps to `skill-devices`**: Only `tokio/sync` (channel primitives) is used — no runtime. Added `tokio-util` to `src-tauri` for `CancellationToken`.

### CLI

- **`health` command**: New CLI command family for Apple HealthKit data. Subcommands: `health` / `health summary` (aggregate counts), `health sleep` / `health workouts` / `health hr` / `health steps` / `health metrics` (typed queries with `--start`, `--end`, `--limit`), `health metric-types` (list stored types), and `health sync` (push data from iOS companion). Human-readable formatting with color-coded output for each data type.

- **`sleep-schedule` command**: New CLI command to view and update the sleep schedule. Supports `sleep-schedule` (show current) and `sleep-schedule set --bedtime HH:MM --wake HH:MM --preset <id>` (update). Available presets: default, early_bird, night_owl, short_sleeper, long_sleeper.

- **LLM error diagnostics in CLI**: The `llm chat` command (both single-shot and REPL modes) now classifies known LLM error patterns (batch overflow, context window exceeded, decode failures, template errors, native panics, tokenization failures) and prints actionable hints alongside the error message.

### UI

- **Log viewer filtering**: The LLM server log in the settings tab now has level filter tabs (All / Info / Warn / Error) and a text search box. The line count shows filtered/total when a filter is active, and "No matching lines" is shown instead of "No log output yet" when filtering produces no results.

- **Tool context compression controls**: Added compression level selector and optional max-search-results / max-result-chars overrides to both the Settings → LLM → Tools tab and the inline chat tools panel.

- **Separate Tools settings tab**: Moved LLM chat tools configuration (per-tool toggles, SearXNG URL, execution mode, max rounds, max calls per round) out of the LLM tab into its own dedicated "Tools" settings tab with a wrench icon and i18n labels for all five languages.

- **Web search sources panel**: When expanding a `web_search` tool call in chat, a new "Sources" section shows each fetched domain with its quality score, content size, and a "best" badge for the highest-scoring result. Each source is expandable to reveal the URL and a content preview, allowing users to inspect exactly what data the LLM received from each site.

- **LLM tools section in own card**: Moved the built-in chat tools configuration out of the collapsible Advanced Inference Settings panel into its own top-level section card with a header showing the enabled count (e.g. "5/8"). The execution mode toggle is placed in a footer row within the same card. This makes tools discoverable without expanding the Advanced section.

- **Hide charts when no session is active**: Band Powers and EEG Waveforms charts are now hidden on the main dashboard when the device is not connected, reducing visual clutter in the disconnected/scanning states.
- **Show PPG/IMU only for capable devices**: PPG charts, PPG metrics, Head Pose card, and IMU chart are now gated by `deviceCaps.hasPpg` / `deviceCaps.hasImu` so they only appear for devices that actually have those sensors.

- **Open Folder button next to data dir input**: Moved the "Open Folder" button from below the default path label to inline next to the data directory input field in Settings, making it more discoverable and accessible.

### Server

- **`sleep_schedule` / `sleep_schedule_set` WS commands**: New WebSocket API commands for reading and writing the sleep schedule configuration. Partial updates supported — only fields present in the request are changed.

### i18n

- Added `llm.autostart`, `llm.autostartDesc` keys to all 5 locales (en, de, fr, uk, he).
- Added `chat.btn.aborting`, `chat.btn.regenerate`, `chat.btn.editResend`, `chat.tokSec`, `chat.ctxWarning`, `chat.noModelBtn`, `chat.logFilter.*` keys to en locale.

- **Search provider strings**: added search provider selector, Brave API key, and SearXNG URL translations in en, de, fr, uk, and he.

- **Context compression labels**: Added translations for context compression settings in English, German, French, Hebrew, and Ukrainian.

- **Sources label**: Added "Sources" / "Quellen" / "Sources" / "מקורות" / "Джерела" translations for the web search sources UI.

- **Convert all Unicode escapes to literal UTF-8**: Replaced all `\uXXXX` escape sequences in i18n locale files (en, de, fr, uk, he) with their literal UTF-8 characters. This makes the translation strings human-readable in source and avoids encoding issues across platforms.

- **Add 14 missing chat translation keys**: Added `chat.btn.aborting`, `chat.btn.editResend`, `chat.btn.regenerate`, `chat.ctxCompact`, `chat.ctxWarning`, `chat.logFilter.*`, `chat.noModel*`, and `chat.tokSec` to German, French, Ukrainian, and Hebrew. All 5 languages now have identical key sets (2,646 keys each).

### Docs

- **SKILL.md**: Added full `health` command reference with subcommand table, CLI examples, HTTP equivalents, JSON response shapes, sync payload format, and common metric types.

- **SKILL.md**: Added `sleep-schedule` command reference with examples, HTTP equivalents, JSON response shapes, and preset table.

- **Updated Discord invite link**: Changed Discord link to `https://discord.gg/Rcvb8Cx4cZ` across README, constants, and help dashboard.

### Dependencies

- **Upgrade skill-headless wry to 0.54**: Updated `skill-headless` crate from `wry` 0.49 to 0.54.3 to match the workspace's tauri-runtime-wry dependency, resolving a `kuchikiki` version conflict. The only API change was renaming `WebViewBuilder::with_web_context()` to `WebViewBuilder::new_with_web_context()`.

### CI

- **Discord commit message**: All CI workflow Discord notifications now include the last commit subject line, making it easier to identify which change triggered the build.

## [0.0.40] — 2026-03-16

### Bugfixes

- **CI: fix latest.json encoding and Python indentation**: Fixed IndentationError in macOS workflow (`if not notes:` was mis-indented inside `except` block) and inconsistent indentation in Linux workflow. Replaced literal `™` with `\u2122` escape in Python scripts and added `ensure_ascii=False` to all `json.dump` calls so `latest.json` is always coherent UTF-8 across all three platform CIs.

- **Fix LINUX.md path in packaging scripts**: Updated `package-linux-dist.sh` and `package-linux-system-bundles.sh` to reference `docs/LINUX.md` instead of the non-existent root-level `LINUX.md`, fixing a `cp: cannot stat` error on Linux CI.

- **Windows CI: fix trademark character encoding**: Replace literal `™` (U+2122) with PowerShell escape `$([char]0x2122)` in the Windows release workflow to prevent `Unexpected token` parse errors.

### Docs

- **AGENTS.md: add CI shared-artifact encoding rule**: New section documenting that all CI workflows must produce and consume `latest.json` as UTF-8 without BOM, with no literal non-ASCII characters in CI scripts.

## [0.0.39] — 2026-03-15

### Bugfixes

- **Fix syntax error in German locale**: Removed corrupted duplicate fragment (`ult de;`) at the end of `src/lib/i18n/de.ts` that caused a build failure.

- **Fix LLM catalog crash on Windows**: Replaced the `llm_catalog.json` symlink in `skill-llm` with a direct `include_str!` path to the source file. Git on Windows checks out symlinks as plain-text files (containing the target path), which caused an invalid-JSON panic at startup.

- **Fix mw75 RFCOMM Send bound violation on Windows**: Bumped `mw75` to 0.0.6 which wraps the RFCOMM `tokio::spawn` future with an `AssertSend` adapter. WinRT COM objects (`IInputStream`, `DataReader`, `StreamSocket`, `IVectorView`, etc.) are thread-safe under MTA but not marked `Send` by the `windows` crate.

- **Fix mw75 RFCOMM build on Windows**: Vendored `mw75` crate with fix for `READ_BUF_SIZE` constant that was gated behind `#[cfg(target_os = "linux")]` but used in the Windows RFCOMM code path, causing compilation failure.

- **Fix mw75 Windows RFCOMM build**: Updated to mw75 v0.0.4 which fixes compatibility with `windows` crate v0.62 by replacing removed `.get()` calls with async/await.

- **Fix 12 failing tests after EEG_CHANNELS bump to 12**: Updated `constants.test.ts` to expect `EEG_CHANNELS = 12` (matching Rust `skill-constants`), decoupled `EEG_CH`/`EEG_COLOR` length assertions from `EEG_CHANNELS` (they are Muse-specific with 4 entries), and updated `BAND_CANVAS_H` expected value from 290 to 642. Fixed over-escaped `\\\"` sequences in `helpTts.apiBody`, `helpFaq.a33`, and `helpSettings.calibrationTtsBody` across de/fr/he/uk locale files that caused the i18n key-extraction regex to detect spurious extra keys (`command\`, `text\`). Updated stale comment in `constants.ts`.

- **Fix Windows CI PowerShell parse errors**: Added UTF-8 BOM to `create-windows-nsis.ps1`, `release-windows.ps1`, and `setup-build-cache.ps1` so Windows PowerShell 5.1 correctly reads non-ASCII characters (™, —). Replaced `?.` null-conditional operator (PowerShell 7+ only) in `release-windows.ps1` with a PS 5.1-compatible alternative.

### Refactor

- **Remove vendored mw75 crate**: Migrated from a local vendored copy of `mw75` to the published crates.io version (0.0.3). The Windows RFCOMM `READ_BUF_SIZE` fix was upstreamed and published.

## [0.0.38] — 2026-03-15

### Features

- **Proactive Hooks**: background EEG monitoring that triggers actions when brain-state matches configured labels. Per-hook scenarios (cognitive/emotional/physical), keyword suggestions, distance threshold, fire history, WebSocket events, and full CLI CRUD.

- **Hook distance suggestion**: analyses HNSW data to recommend optimal thresholds with percentile visualization.

- **LLM coding-agent tools**: bash execution, read/write/edit file, web search (DuckDuckGo JSON + HTML fallback), web fetch, search_output for navigating large outputs. Safety approval dialogs for dangerous operations. Context-aware tool calling with automatic history trimming.

- **Tool-call cards with rich detail views**: expandable cards per tool type with cancel/stop, danger detection, and structured argument display.

- **Chat session archive (soft-delete)**: archive instead of permanent delete, with restore and permanent-delete from archive section.

- **Neurable MW75 Neuro headphone support**: Full 12-channel EEG session at 500 Hz. BLE activation + RFCOMM data streaming (behind `mw75-rfcomm` feature flag). Electrode placement guide shows MW75 ear-cup layout with 6 electrodes per ear (FT7/T7/TP7/CP5/P7/C5 left, FT8/T8/TP8/CP6/P8/C6 right). All 12 channels render in the dashboard: signal quality dots, EEG waveforms, spectrogram, and band powers. DSP pipeline processes all active channels. Device presets for Muse (4ch), Ganglion (4ch), and MW75 (12ch) in electrode guides.

- **Hermes 10-20 electrode labels**: Replaced generic `Ch1`–`Ch8` channel names with standard 10-20 positions (`Fp1`, `Fp2`, `AF3`, `AF4`, `F3`, `F4`, `FC1`, `FC2`) in `skill-constants` and the electrode placement SVG guide.

- **Hermes V1 EEG headset support**: Added full session support for the Hermes V1 headset (8-channel ADS1299 at 250 Hz, 9-DOF IMU). The `hermes-ble` crate is added to `skill-devices` and re-exported. All data streams over BLE GATT — no RFCOMM needed. BLE scanner recognises devices whose name starts with "Hermes". Session handles EEG (8 channels through DSP pipeline), IMU (accel + gyro → head pose), and packet-drop detection. Dashboard renders 8 channels dynamically with device-specific labels and colours. Electrode placement guide and 3D electrode guide include Hermes V1 tab. Constants added to `skill-constants` (`HERMES_EEG_CHANNELS`, `HERMES_SAMPLE_RATE`, `HERMES_CHANNEL_NAMES`).

- **Screenshot capture + vision embedding system**: periodic active-window capture with CLIP vision embedding (ONNX) and HNSW index. macOS CoreGraphics FFI, Linux X11/Wayland, Windows GDI. Configurable interval, size, quality, and embedding backend.

- **OCR text extraction + text embedding**: on-device OCR via `ocrs` crate. Dual HNSW architecture for visual and text similarity search.

- **Screenshots Settings UI tab**: full configuration with live re-embed progress.

- **Configurable OCR engine, GPU/CPU toggle**: `ScreenshotConfig` extended with OCR engine, model, and GPU settings.

### Performance

- **Unblock main & settings window startup by moving I/O-heavy Tauri commands to async threads**: Converted 12 synchronous `#[tauri::command]` handlers that performed directory scanning, JSON parsing, or SQLite queries on the Tauri IPC executor thread to `async` commands using `tokio::task::spawn_blocking`. This prevents those operations from stalling window rendering and other IPC calls during startup. Affected commands: `list_sessions`, `delete_session`, `list_embedding_sessions` (history_cmds); `get_daily_recording_mins`, `suggest_hook_distances`, `get_hook_log`, `get_hook_log_count`, `list_serial_ports` (settings_cmds); `query_annotations`, `get_recent_labels`, `delete_label`, `update_label`, `get_stale_label_count` (label_cmds).

- **History view: batch metrics loading with disk cache**: Replaced per-session IPC waterfall (4 concurrent `get_csv_metrics` calls) with a single `get_day_metrics_batch` call that loads all sessions' metrics in one roundtrip. Added persistent `_metrics_cache.json` disk cache next to each session CSV — subsequent loads skip CSV re-parsing entirely. Timeseries payloads are downsampled to ≤360 points on the backend, reducing transfer size for sparklines and heatmaps. Week view also uses a single batch call for all 7 days. Adjacent-day prefetching now batch-loads as well.

- **Shared text embedder — eliminate 3 redundant model copies (~390 MB RAM saved)**: consolidated four independent `fastembed::TextEmbedding` instances into a single app-wide `Arc<Mutex<TextEmbedding>>`.

- **GPU optimization across all model systems**: enabled flash attention and KQV offloading in LLM; GPU-first mmproj loading on Linux; DirectML on Windows and CUDA on Linux for screenshot embeddings.

- **Async history commands**: Converted `list_session_days`, `list_sessions_for_day`, `get_session_metrics`, `get_session_timeseries`, `get_csv_metrics`, `get_day_metrics_batch`, and `get_sleep_stages` from synchronous Tauri commands to async commands using `tokio::task::spawn_blocking`, preventing UI thread blocking during heavy file I/O and CSV/SQLite parsing.

### Bugfixes

- **Heatmap session counts**: `daySessionCounts` now reads from the localStorage day cache to return actual session counts instead of always 1, making month/year heatmap intensity meaningful.

- **Fix clippy warnings and a11y lint in history page**: Resolved 9 clippy warnings — `let_and_return` in ws_commands.rs, `double_ended_iterator_last` and `collapsible_str_replace` in llm.rs, `iter_cloned_collect` in hermes/mw75 sessions, `needless_borrow` in settings_cmds.rs. Converted label dot `<span>` elements to `<button>` with `aria-label`, `onfocus`/`onblur` handlers for keyboard accessibility.

- **History view hover fixes**: Fixed label hover not working on the day-grid heatmap canvas — hovering a cell containing a label now correctly highlights matching labels and shows the label tooltip. Fixed week view hover completely missing — added mouse event handlers to the day-dots canvas so both epoch dot and label circle hovers show tooltips. Moved tooltip rendering outside view-mode conditionals so tooltips are visible in all views.

- **History label dot tooltips no longer clipped**: Replaced `absolute`-positioned tooltips on label dots with a `fixed`-position portal element, preventing overflow clipping from parent containers with `overflow-hidden`. Tooltips now always render above all content regardless of DOM nesting.

- **Fix missing objc2 dependencies in skill-screenshots**: Added `objc2` and `objc2-app-kit` as macOS-only dependencies to resolve compilation errors when resolving the frontmost application PID via NSWorkspace.

- **Fix all svelte-check warnings and i18n gaps**: Resolved 5 Svelte compiler warnings — fixed `propColors` initial-value capture in `EegChart.svelte` by using `EEG_COLOR` fallback, fixed `device` prop capture in `ElectrodeGuide.svelte` by deferring initial tab selection to `$effect`, and fixed non-interactive element warning in `history/+page.svelte` by using `role="toolbar"` with `tabindex`. Added 215 missing English-fallback i18n keys across de, fr, he, and uk locales.

- **Fix `$state` invalid placement in titlebar-state**: Moved `$state(initial)` from a `return` statement to a variable declaration initializer, fixing the Svelte 5 `state_invalid_placement` error in `src/lib/titlebar-state.svelte.ts`.

### Refactor

- **Dynamic multi-channel DSP pipeline**: `EEG_CHANNELS` raised from 4 to 12 (max across all devices). `EegFilter` and `BandAnalyzer` track active channels and only wait for channels that have received data before firing GPU batches. Muse/Ganglion sessions use channels 0–3; MW75 uses all 12. Inactive channels have zero overhead.

- **Dynamic channel rendering**: EegChart, BandChart, signal quality, and EEG channel values all accept dynamic channel count/labels/colors via props. MW75 renders 12 channels in a 3-column grid; Muse/Ganglion render 4 in 2 columns.

- **Centralize hardcoded constants with prelude module**: added `skill_constants::prelude` re-exporting ~60 most-used constants; moved 50+ hardcoded values from `skill-eeg` (signal quality thresholds, artifact detection params, head pose params), `skill-data` (PPG IBI limits, DND identifiers), `skill-devices` (SNR thresholds), `skill-label-index` (index file names, HNSW params), `skill-llm` (catalog file, log dir/cap), `skill-tts` (event name, silence duration, KittenTTS config), `skill-tray` (menu rebuild debounce), `skill-tools` (tool-call delimiters, bash limits), and `src-tauri` (session gap, PPG channels, active window idle, WS request log cap) into `skill-constants` as the single source of truth; added `skill-constants` dependency to 5 crates that previously lacked it.

- **Cross-crate deduplication**: consolidated shared utilities into `skill-data::util` — `date_dirs`, `MutexExt`, UTC timestamp formatters (`yyyymmdd_utc`, `yyyymmddhhmmss_utc`, `unix_to_ts`, `ts_to_unix`, `fmt_unix_utc`, `civil_from_unix`), and `open_readonly` SQLite helper. Removed duplicate implementations from `skill-commands`, `skill-label-index`, `skill-exg`, `skill-screenshots`, and `skill-router`.
- **Screenshot HNSW dedup**: replaced near-identical vision/OCR HNSW load/save/rebuild function pairs in `skill-screenshots` with a generic `load_or_rebuild_hnsw_generic` + `save_hnsw_to` parametrized by path and fetch closure.
- **Band-snapshot enrichment**: extracted `enrich_band_snapshot` + `SnapshotContext` into `skill-devices`, eliminating ~90 lines of duplicated PPG/artifact/head-pose/composite-score/GPU enrichment from `muse_session.rs` and `openbci_session.rs`.
- **DND decision dedup**: replaced ~200-line inline DND decision block in `muse_session.rs` with the existing `skill_devices::dnd_tick()` pure function and proper state round-tripping.
- **Constants consistency**: fixed hardcoded `"labels.sqlite"` in `skill-data::label_store` to use `LABELS_FILE` from `skill-constants`.

- **Deduplicate frontend types and formatting helpers**: extracted shared TypeScript interfaces into `$lib/types.ts`; extracted 12 formatting functions into `$lib/format.ts`; extracted `SleepAnalysis` into `$lib/sleep-analysis.ts`; updated 15 consumer files; eliminates ~30 duplicate interface definitions and ~20 duplicate utility functions.

- **Canvas chart dedup**: migrated ImuChart, PpgChart, GpuChart, and BandChart to use the shared `animatedCanvas` Svelte action from `use-canvas.ts`, eliminating duplicated ResizeObserver + requestAnimationFrame + DPR scaling boilerplate. EegChart is intentionally left as-is due to its spectrogram tape + MutationObserver + frame-skip complexity.

- **HuggingFace cache path consolidation**: added `hf_cache_root()`, `hf_model_dir()`, and `hf_ensure_dirs()` helpers to `skill-data::util`. Replaced the manual env-var resolution in `skill-exg::resolve_hf_weights` and the duplicated `Cache::from_env().path()` + folder construction pattern in both `skill-exg` and `skill-llm::catalog`. Removed the now-unused `dirs` crate dependency from `skill-exg`.

- **Titlebar store factory**: added `createTitlebarState()` and `createTitlebarCallbacks()` in `titlebar-state.svelte.ts`. Refactored `chat-titlebar`, `history-titlebar`, and `label-titlebar` stores to use the shared factory instead of raw `$state()` calls.

- **Deduplicate Rust and frontend code**: replaced duplicated `MutexExt` trait in `src-tauri` with re-export from `skill-data::util`; removed duplicated `unix_secs_now()` in `eeg_embeddings.rs`; exported `rgba()` from `theme.ts` and removed duplicate in `GpuChart.svelte`; extracted `MuseStatus::reset_for_scanning()` to replace ~20-line identical status reset blocks in `muse_session.rs` and `openbci_session.rs`; converted 14 pure re-export shim files to inline module declarations in `lib.rs` and deleted the files; added `PPG_SAMPLE_RATE` to `skill-constants` and derived sample-rate constants in `session_csv` from the canonical source; eliminated `skill-eeg/src/constants.rs` shim file.

- **Deduplicate Tauri backend boilerplate**: Added `skill_dir()`, `read_state()`, `mutate_state()`, and `mutate_and_save()` helpers to `lib.rs`, replacing ~30 repetitive `state.lock_or_recover().skill_dir.clone()` call sites across `commands.rs`, `ws_commands.rs`, `settings_cmds.rs`, `label_cmds.rs`, `history_cmds.rs`, `session_analysis.rs`, and `global_eeg_index.rs`. Added `search_params()` helper in `commands.rs` to deduplicate the k/ef clamping pattern. Added generic `load_json_or_default()`, `save_json()`, and `init_wal_pragmas()` to `skill-data::util`, replacing duplicated JSON config I/O and SQLite PRAGMA patterns across `skill-settings`, `skill_log`, `activity_store`, `screenshot_store`, `hooks_log`, and `eeg_embeddings`. Applied `mutate_and_save()` to ~12 simple set-then-persist patterns in `window_cmds.rs` and `settings_cmds.rs`.

- **Extract `skill-commands` workspace crate**: moved EEG embedding search, timestamp helpers, SVG/DOT graph generation, PCA projection, and streaming search (2,321 lines) into `crates/skill-commands/`. Zero Tauri dependencies.

- **Extract `skill-devices` workspace crate**: moved DND focus-mode engine, composite EEG scores, and battery EMA from `muse_session.rs` (774 lines) into `crates/skill-devices/`. Zero Tauri dependencies. Includes 9 unit tests.

- **Extract `skill-exg` workspace crate**: moved cosine distance, fuzzy matching, HF weight management, GPU cache, and epoch metrics from `eeg_embeddings.rs` (2,613 lines) into `crates/skill-exg/`. Zero Tauri dependencies.

- **Extract `skill-jobs` workspace crate**: moved sequential job queue (384 lines) into `crates/skill-jobs/`. Zero Tauri dependencies. All 3 unit tests pass.

- **Extract `skill-router` workspace crate**: moved UMAP projection, embedding/label loaders, cluster analysis, and metric rounding from `ws_commands.rs` (2,408 lines) into `crates/skill-router/`. Zero Tauri dependencies.

- **Extract `skill-settings` workspace crate**: moved persistent configuration types and disk I/O (924 lines) into `crates/skill-settings/`. All 27 unit tests pass.

- **Extract `skill-tray` workspace crate**: moved progress-ring overlay, shortcut formatting, and dedup helpers from `tray.rs` (674 lines) into `crates/skill-tray/`. Pure `std`, zero dependencies. Includes 8 unit tests + 2 doc-tests.

- **Flatten `llm/` and `tts/` module directories into single files**: converted multi-file modules into single files. No API or import path changes.

- **Remove dead `llm/` duplicates**: deleted 1,277 lines of dead code (byte-identical copies of `catalog.rs` and `chat_store.rs`).

- **Deduplicate frontend formatting, canvas setup, navigation, and UI patterns**: replaced ~15 inline date formatting calls with shared `format.ts` helpers; added `dateToLocalKey()`, `dateToCompactKey()`, `fromUnix()`, `toUnix()`, `setupHiDpiCanvas()`, `getDpr()`, `fmtCountdown()`, `fmtDateTimeLocalInput()`, `parseDateTimeLocalInput()`, `localKeyToUnix()` to `format.ts`; replaced 23 DPR canvas setup blocks across 8 chart files; migrated `EegIndices`, `FaaGauge`, `HeadPoseCard` to `CollapsibleSection`; created `$lib/navigation.ts` with shared window-open helpers used by 8 files; created `ConfirmAction` UI component replacing inline confirm-delete in history and labels; cleaned up compare page internal duplication (5 DPR blocks, inline date helpers).

- **Move muse-rs and openbci dependencies to skill-devices crate**: Moved `muse-rs` and `openbci` dependency declarations from `src-tauri/Cargo.toml` to `crates/skill-devices/Cargo.toml` and re-exported them. Updated all imports in `muse_session.rs` and `openbci_session.rs` to use the re-exports via `skill_devices::`.

- **Extract `skill-history` crate**: Moved all session history, metrics, time-series, sleep staging, and analysis logic from `src-tauri/src/{history_cmds,session_analysis}.rs` into a new `crates/skill-history` workspace crate with zero Tauri dependencies. The Tauri files now contain only thin async IPC wrappers that delegate to `skill_history::*` and run on `spawn_blocking` threads. Types (`SessionEntry`, `SessionMetrics`, `EpochRow`, `CsvMetricsResult`, `SleepStages`, `HistoryStats`, `EmbeddingSession`) and all pure functions (`list_sessions_for_day`, `load_metrics_csv`, `get_session_metrics`, `get_sleep_stages`, `compute_compare_insights`, `analyze_sleep_stages`, `analyze_search_results`, `compute_status_history`, etc.) are now public API in the crate.

- **Shared canvas lifecycle action**: added `src/lib/use-canvas.ts` (`animatedCanvas` Svelte action) to DRY the ResizeObserver + requestAnimationFrame + DPR scaling boilerplate duplicated across EegChart, BandChart, PpgChart, GpuChart, and ImuChart. Existing charts are not yet migrated (tracked as a TODO).

### Build

- **Extract `skill-screenshots` workspace crate**: moved 1,533 lines of screenshot capture, vision embedding, HNSW search, and OCR into `crates/skill-screenshots/`.

- **Extract `skill-tools` workspace crate**: moved all LLM tool logic (definitions, execution, parsing, validation, safety) into `crates/skill-tools/`.

- **Create `skill-constants` crate**: single source of truth for all constants.

- **Extract `skill-data` workspace crate**: moved 2,984 lines of pure data/utility logic into `crates/skill-data/`.

- **Extract `skill-tts` workspace crate**: moved 1,307 lines of TTS logic into `crates/skill-tts/`.

- **Extract `skill-eeg` workspace crate**: moved 3,459 lines of EEG DSP into `crates/skill-eeg/`.

- **Extract `skill-llm` workspace crate**: moved 7,421 lines of LLM logic into `crates/skill-llm/`.

- **sccache + mold for faster builds**: auto-detected build caching. ~54% faster clean rebuilds.

- **Fragment-based changelog system**: replaced single-file `CHANGELOG.md` editing with `changes/unreleased/` fragments. Each change gets its own `.md` file; `npm run bump` compiles fragments into `changes/releases/<version>.md`, deletes consumed fragments, and rebuilds `CHANGELOG.md` from all release files. All 20 historical releases migrated. Supports `--rebuild` to regenerate from archives.

### CLI

- **Dynamic signal quality rendering**: `status` command now renders signal quality for any number of EEG channels (4/8/12) instead of hardcoding Muse's tp9/af7/af8/tp10 keys. Added device support note and tool-calling documentation to CLI header comment.

- **Full LLM management via CLI**: `llm select`, `llm mmproj`, `llm autoload-mmproj`, `llm pause`, `llm resume`, `llm downloads`, `llm refresh`, `llm fit`, `llm add`.

- **Calibration profile CRUD**: `calibrations create`, `calibrations update`, `calibrations delete`.

- **Hooks CRUD**: `hooks list`, `hooks add`, `hooks remove`, `hooks enable/disable`, `hooks update`, `hooks suggest`, `hooks log`.

### UI

- **History day view — 24×720 heatmap grid**: replaced the linear 24-hour timeline bar and epoch dot canvas with a dense heatmap grid (24 hour-columns × 720 five-second rows); cells colored by session color with opacity modulated by relaxation+engagement; hour headers, 15-minute grid lines, minute labels; scrollable canvas (max 420px); cursor-following tooltip with HH:MM:SS and data values.

- **History day view — rainbow label circles**: replaced text-based label display with tiny colored circles in session rows, expanded details, timeline legend, and canvas; rainbow hue distribution (0°–300° HSL) based on temporal order; hover highlights exact-match labels (glow ring + scale) and temporally close labels (within 5 min, brightness/glow); popover tooltips on hover; cross-session matching.

- **History day view — screenshot indicators on heatmap**: cells that have a corresponding screenshot show a small blue diamond indicator on the canvas grid; hovering a screenshot cell displays a floating image preview (loaded from the local API server); preview disappears immediately when the mouse moves to a different cell; screenshot data loaded per-day via `get_screenshots_around`.

- **History view UX improvements**: 14 enhancements to the history view:
  1. Week view shows total recording duration per day in sidebar (e.g. "2s · 1h 30m").
  2. Clicking on the day grid heatmap scrolls to and expands the corresponding session.
  3. Week view entire row is clickable to navigate to day view (not just sidebar).
  4. Month view calendar cells show mini duration bars proportional to recording hours.
  5. Day grid draws a red "now" marker hairline when viewing today.
  6. Keyboard shortcuts 1/2/3/4 switch between year/month/week/day views; arrow keys navigate in calendar views too.
  7. Week view today row has a visible left-edge accent bar instead of nearly invisible tint.
  8. Cross-highlighting between grid and session list — hovering a cell highlights the session row below; hovering grid sets a primary ring on the matching session card.
  9. Week↔Day view transitions use a subtle fade animation.
  10. Day view shows an aggregate daily summary card (total duration, avg relaxation/engagement, label count) above the session list when there are multiple sessions.
  11. Fixed `daySessionCounts` always returning 1 — heatmap intensity now reflects actual session count per day from cache.
  12. Month/year view tooltips show recording duration alongside session count.
  13. Expanding a session row scrolls it into view smoothly.
  14. Week view shows small session color legend dots in each day row.

- **MW75 Neuro device photo**: Added product image for the MW75 Neuro headphones, displayed on the dashboard when connected and in the device list in Settings.

- **History view accent consistency**: migrated all interactive/highlight colors in the history view from hardcoded `emerald-*` to accent-aware `violet-*` (remapped by the Appearance accent setting). Affected: year/month heatmap cells and legend swatches, month calendar day-count text, screenshot canvas diamond indicator (now reads `--color-violet-500` CSS property), and screenshot tooltip dot. Semantic status colors (`emerald-500` for positive trend, `red-*` for destructive actions) remain unchanged.

### i18n

- **Complete translation pass across de/fr/he/uk**: all untranslated keys now translated. `npm run audit:i18n` reports 0 untranslated keys.

- **Translation audit script** (`scripts/audit-i18n.ts`): detects untranslated keys with exemption system for technical terms.

### Docs

- **AI models reference**: Added `docs/AI.md` documenting all AI/ML models used across the codebase — LLM catalog, ZUNA EEG foundation model, CLIP/Nomic vision embeddings, OCR engines, TTS backends, HNSW search, and UMAP projection.

- **Add `README.md` to every workspace crate**: created README files for all 19 crates that were missing one; each contains heading and description from `Cargo.toml`.

- **Expand `crates/*/README.md`**: rewrote all 17 crate READMEs with comprehensive documentation — overview, public API tables, feature flags, and dependency lists.

- **Rewrite root `README.md`**: restructured to reference all workspace crate READMEs and `docs/` guides; added project layout section with crate table, vendored crates, and frontend structure.

- **Add workspace crate outline to `AGENTS.md`**: brief table of all 17 workspace crates and 2 vendored crates with one-line purpose descriptions.

- **Move `CHANGELOG.md` to project root**: merged the root-level entries into the comprehensive `docs/CHANGELOG.md` and moved to root.

- **Update SKILL.md for new devices, tools, and screenshots**: Added "Supported Devices" section documenting Muse, OpenBCI Ganglion, MW75 Neuro (12ch), and Hermes V1 (8ch) with channel counts and sample rates. Updated `status` JSON example to show device-agnostic fields (`eeg_channels`, multi-device name examples, dynamic signal quality keys). Added "Built-in Tool Calling" section to the LLM docs covering bash, read/write/edit, web search/fetch tools with safety info. Added "Screenshots (UI-only Feature)" section documenting capture, CLIP vision embedding, OCR, and dual HNSW search. Updated table of contents numbering.

- **Mermaid crate dependency graph**: Added an interactive Mermaid diagram to the README Architecture section showing how all workspace crates depend on each other and connect to the Tauri app and SvelteKit frontend.

- **Constants sync guard**: improved doc comment in `src/tests/constants.test.ts` explaining how the test file guards Rust↔TypeScript constant drift.

## [0.0.37] — 2026-03-14

### History — Calendar Heatmap View

- **Calendar heatmap** replaces the old single-day paginator as the default history view. Choose between **Year**, **Month** (default), **Week**, and **Day** granularity via a segmented control in the titlebar.
- **Year view**: GitHub-style contribution heatmap with weekly columns, month labels, and an intensity legend (emerald gradient). Click any day with recordings to drill into the day view.
- **Month view**: traditional calendar grid with per-day session counts and heat-colored backgrounds. Out-of-month days are dimmed; today is ring-highlighted.
- **Week view**: 24-hour timeline grid per day with canvas-rendered **epoch dots** — one dot per 5-second EEG embedding, color-coded by session. Each dot's Y position maps relaxation value within the session's band. Session bar fallback renders while timeseries loads. Day label sidebar shows weekday, date, and session count; click to navigate to day view. Empty days show a subtle placeholder.
- **Day view – epoch dot canvas**: new canvas timeline below the existing 24h bar renders all epoch dots for the day, with session color legend and label count summary. Label markers appear as amber triangles with text.
- **Always-rendered UI**: the calendar grid renders immediately, even while data is loading or when there are no recordings yet. A skeleton pulse animation shows in the day-label slot during loading; empty state shows a gentle hint with the clock icon and guidance text.
- **Reworked titlebar UX**: themed `history-viewmode-seg` segmented control (accent-aware via `--color-primary`); context-sensitive navigation — day mode shows prev/next arrows with position counter, calendar modes show period-appropriate prev/next with a formatted label (e.g. "March 2026"); skeleton loading animation for day labels.
- **i18n**: all new strings (`history.view.year/month/week/day`, `history.heatmap.less/more/none/hours/dayStreak`, `history.session`) translated across English, German, French, Hebrew, and Ukrainian.

### LLM — Hardware Fit Prediction

- **`get_model_hardware_fit` Tauri command**: uses `llmfit-core`'s `ModelFit::analyze` against the user's detected hardware (`SystemSpecs::detect()`, cached via `OnceLock`) to predict whether each catalog model will run. Returns fit level, run mode, estimated memory, tok/s, and composite score.
- **Per-quant fit badges** in the LLM settings tab: 🟢 Runs great / 🟡 Runs well / 🟠 Tight fit / 🔴 Won't fit. Hover tooltip shows run mode, memory breakdown, and estimated speed.
- **Hardware fit detail row** below each model entry showing run mode, memory utilization, estimated tok/s, and score.
- **Hardware summary line** above the family dropdown showing available memory from the detected GPU/RAM pool.
- **i18n**: `llm.fit.*` keys translated across all 5 locales.

### UI

- **Refactored `CustomTitleBar.svelte`**: collapsed the macOS and Windows/Linux duplicate branches into shared Svelte 5 snippets (`windowControls`, `centerContent`, `actionButtons`, `historyHead`, `tbBtn`, plus reusable icon snippets). Single unified template switches element order based on platform. Eliminated all duplicated HTML and CSS blocks. **975 → 533 lines (45% reduction).**
- Main-window titlebar now tints red when Bluetooth is unavailable (`bt_off` state), giving an immediate visual cue that the BLE adapter is off or missing. Uses the semantic `--color-error` token so the tint respects both light and dark themes.

### Chat — Tool Calling (pi-mono architecture)

- Implemented pi-mono style tool calling architecture with structured lifecycle events, argument validation, and configurable execution modes.
- Added **JSON Schema argument validation** for tool calls using the `jsonschema` crate — tool arguments are now validated against the tool's JSON Schema `parameters` definition before execution, with detailed error messages on validation failure (modelled after pi-mono's `validateToolArguments` with AJV).
- Added **configurable tool execution mode**: `parallel` (prepare sequentially, execute concurrently — default) and `sequential` (execute one-by-one in order). Persisted in `settings.json` under `llm.tools.execution_mode`.
- Added **configurable max tool rounds** (`max_rounds`, default 3) and **max tool calls per round** (`max_calls_per_round`, default 4) — both persisted in settings.
- Added **rich tool-execution lifecycle events** via IPC: `ToolExecutionStart` (with tool_call_id, tool_name, validated args) and `ToolExecutionEnd` (with result JSON and is_error flag), alongside the legacy `ToolUse` status events for backwards compatibility.
- Added **`BeforeToolCallFn` / `AfterToolCallFn` hook type definitions** for future extensibility — allows blocking tool execution or overriding results programmatically (modelled after pi-mono's `beforeToolCall`/`afterToolCall` hooks).
- Added execution mode toggle UI in both the Chat window settings panel and Settings → LLM tools section.
- Fully localised new strings in all five languages (EN, DE, FR, UK, HE).
- Added 4 new Rust unit tests for argument validation (valid args, missing required, no schema, wrong type) — all 15 tool tests pass.

### Chat — Tool Calling

- Added tool calling support to the LLM chat window with four built-in tools: **Date & Time**, **Location** (IP geolocation via ipwho.is), **Web Search** (DuckDuckGo Instant Answer API), and **Web Fetch** (fetch & read web pages).
- Added per-tool enable/disable toggles in the chat settings panel — persisted via `settings.json` under `llm.tools`.
- Added live tool-use indicators on assistant messages (calling → done/error) via a new `ToolUse` IPC chunk variant.
- Added a **Tools** badge in the chat header showing the number of enabled tools.
- Tool toggles are only shown when the model is running (`supports_tools` flag from the server status).
- Fully localised in all five languages (EN, DE, FR, UK, HE).

### Bugfixes

- Fixed quit confirmation dialog never receiving focus — set the parent window on the `rfd::MessageDialog` so the popup appears focused and modal on Linux/Windows instead of opening behind the main window.

- Fixed malformed thought traces that began with an unmatched opening `json` code fence and partial JSON fragments, which caused the rest of the thought bubble markdown to render incorrectly. The shared `normalizeMarkdown()` helper now strips that narrow orphaned preamble while preserving legitimate closed fenced code blocks.
- Fixed another chat tool-call transcript leak in the frontend parser. `stripToolCallFences()` now mirrors the Rust-side tool-call prefix heuristic instead of relying on narrow fence regexes, so incomplete or malformed fenced JSON blocks with blank lines or partial bodies are suppressed before they can appear in the lead-in bubble.
- Hardened chat markdown normalization for malformed model output. Emphasis repair now runs through a shared `normalizeMarkdown()` utility that protects fenced code blocks and inline code spans, trims stray spaces inside `*`/`**` delimiters, and falls back to raw `<strong>`/`<em>` tags when CommonMark flanking rules would still reject the emphasis. Added unit coverage for the repaired cases.
- Fixed expanded thought panels rendering raw markdown while final answers rendered correctly. The thought body now uses the shared `MarkdownRenderer`, so the same markdown normalization and parsing logic applies in both places. Added a muted renderer variant to preserve the thought-panel visual treatment.
- Fixed bold/italic not rendering when models emit `**word **` (space before closing delimiter) or `**Label:**value` (closing `**` preceded by punctuation followed by a non-whitespace character — CommonMark non-right-flanking edge case). Extended `normalizeMd()` in `MarkdownRenderer` with a trailing-space strip pass and a targeted conversion of punctuation-adjacent patterns to raw `<strong>`/`<em>` HTML so they always render as bold/italic regardless of CommonMark delimiter rules.

- Reworked chat-window assistant turn parsing/rendering so one streamed assistant turn can display as separate bubbles for lead-in text, tool activity, collapsed thinking, and final response, instead of merging tool chatter, `<think>` content, and the user-facing answer into one Markdown bubble.
- Fixed bold and italic text not rendering in the final answer bubble when a model emits `** Word:**` (space inside the `**` delimiters).  Added a `normalizeMd()` pre-pass in `MarkdownRenderer` that strips stray leading/trailing spaces inside `**…**` and `*…*` delimiters before handing the string to `marked`.
- Fixed multi-tool calls emitted as a single dict object: `{"date": {}, "location": {}}`.  Models like Qwen3 batch all tool calls as one JSON object where each key is a tool name and each value is the parameter object.  The previous extractor only recognised the OpenAI `{"name":"...","parameters":{}}` and `tool_calls:[...]` shapes.  Fix: added `KNOWN_TOOL_NAMES` constant (`date`, `location`, `web_search`, `web_fetch`), `is_dict_style_multi_tool()` helper, updated `extract_calls_from_value` (iterates over dict entries as calls), `is_tool_call_value` and `looks_like_tool_call_json_prefix` (early-exit when a known tool name appears as a JSON key), and frontend `stripToolCallFences` (same dict-style heuristic).  11/11 Rust unit tests pass.
- Fixed multi-tool and multi-round tool-calling rendering.  Root causes: (1) text emitted by the LLM before a tool call (e.g. "I'll use the date tool") appeared in the response bubble while the tool was running, then snapped to the lead-in position once the next inference round began; (2) with non-thinking models, consecutive rounds' text concatenated into one blob.  Fix: on the first `tool_use "calling"` event per round, the current `rawAcc` is parsed and frozen into `frozenLeadIn`/`frozenThinking`, `rawAcc` is reset to empty, and all subsequent delta/done/error handlers merge the frozen state back via `mergeWithFrozen()`.  Tools from multiple rounds accumulate correctly in the `toolUses` array.
- Replaced the blinking text cursor shown during LLM inference with a spinning SVG arc on the avatar column; the "AI" avatar is restored once generation completes.
- Reordered assistant turn sub-bubbles into strict chronological sequence: *thinking* (collapsed) → *lead-in text* → *tool-use indicators* → *response*.
- Fixed partial tool-call JSON fences and literal `</think>` tags appearing in the chat response bubble for tool-calling models (e.g. Qwen3).  Root causes: (1) the stream sanitizer emitted partial fence text before accumulating enough tokens to recognise the fence as a tool call; (2) tool-calling turns emit two separate `<think>` blocks (pre-tool and post-tool) which the single-pair extractor left unstripped in `content`.  Fix: added `stripToolCallFences()` to the frontend that removes both complete and incomplete fenced tool-call blocks, and rewrote `parseAssistantOutput()` to collect all `<think>…</think>` pairs across a multi-turn response, merging them into a single thinking block while routing the final segment to the answer bubble.
- Fixed chat message formatting when a model starts a JSON tool-call code fence and never closes it before `<think>`: the streaming sanitizer now suppresses incomplete trailing tool-call fences/JSON early enough that Markdown never treats the rest of the assistant reply as one giant code block.
- Fixed chat thinking-panel separation when `<think>` appears after other assistant text (for example after tool-call lead-in text): the chat UI now extracts think blocks from anywhere in the assistant message instead of only when `<think>` is the first visible token.
- Fixed chat tool-calling transcript leakage: assistant JSON tool payloads emitted in OpenAI-style inline/fenced blocks are now stripped from visible streamed output/history (not only `[TOOL_CALL]...[/TOOL_CALL]` markers), so users no longer see raw call JSON before the final natural-language answer.
- Updated the built-in `date` tool response to include explicit local-time metadata (`iso_local`, timezone abbreviation/name, and UTC offset) plus `iso_utc`, so assistant time answers can reliably default to the user's local timezone instead of guessing from epoch values.
- Fixed a follow-up chat tool-calling parser gap where some models output `{"tool":"date","parameters":{}}` instead of `{"name":"date",...}`; extractor now treats `tool` as a valid alias for function name so built-in tool execution triggers for that payload shape too.
- Fixed in-app chat tool-calling compatibility when models emit OpenAI-style function-call JSON directly in assistant text (including `{"name":"date","parameters":{}}`, fenced `json` blocks, and `{"tool_calls":[...]}` envelopes) instead of llama.cpp `[TOOL_CALL]...[/TOOL_CALL]` markers; the Rust extractor now detects these payload shapes and executes built-in tools correctly.
- Reduced title/menu redraw churn by deduplicating unchanged window-title writes (`setTitle`) and skipping no-op titlebar title-observer state updates.
- Reduced spacing between titlebar close/maximize/minimize controls across all windows by matching shared `CustomTitleBar` window-control button width to the other titlebar icon buttons (`30px`).
- Fixed Tailwind v4 `Invalid declaration: onMount` dev-server errors across `CustomTitleBar.svelte`, `+page.svelte`, `GpuChart.svelte`, `DisclaimerFooter.svelte`, and others — `@tailwindcss/vite` v4.2's `enforce:"pre"` transform matched `.svelte?svelte&type=style&lang.css` virtual modules before the Svelte compiler had extracted the `<style>` block, causing the CSS parser to choke on JavaScript imports. Patched `vite.config.js` with a shim that skips all `.svelte` style virtual module IDs in Tailwind's transform plugins. Also removed empty `<style></style>` blocks in `whats-new/+page.svelte` and `UmapViewer3D.svelte`.
- Fixed mmproj crash when the vision projector file is missing on disk — added an `exists()` guard before calling `mtmd_init_from_file` (which can abort/segfault on some platforms instead of returning null); switched from `active_mmproj_path()` to `resolve_mmproj_path(autoload)` so auto-detection works properly; stale paths where the file has been deleted are now filtered out with a warning instead of passed to the C library.
- Fixed app crash after mmproj fails to load — the clip/vision GPU warmup in `MtmdContextParams` (enabled by default) could corrupt Vulkan GPU state when the mmproj file was incompatible with the text model, causing the subsequent text-model warmup decode to abort the process. Disabled the clip warmup at init time (deferred to the first real multimodal request); wired up `no_mmproj_gpu` and `mmproj_n_threads` settings that were defined in `settings.rs` but never passed to the native library; added a file-size sanity check (files < 1 KB are rejected as truncated downloads); wrapped `init_from_file` in `catch_unwind` so a native panic cannot take down the application; improved error messages to include the file path and size for easier diagnostics.
- Fixed Linux mmproj startup crashes caused by unstable mtmd/Vulkan projector initialization paths on some driver stacks: mmproj now defaults to CPU projector init on Linux (while preserving normal text-model GPU offload), and advanced users can explicitly re-enable mmproj GPU init with `SKILL_FORCE_MMPROJ_GPU=1`.
- Fixed stale `mmproj` fallback selection on startup: when the active text model belongs to a known catalog repo, startup now rejects projector paths from a different repo (for example, a 27B projector with a 4B model), logs a clear mismatch warning, and continues in text-only mode without calling mtmd on the incompatible file.
- Fixed Linux WebKit startup abort on Wayland caused by `stacker::maybe_grow` swapping the main-thread stack before JavaScriptCoreGTK initialised. Linux now raises `RLIMIT_STACK` to 64 MiB and runs Tauri on the original main-thread stack; macOS and Windows keep the existing `stacker` path.
- Fixed Linux app auto-close after startup caused by implicit `RunEvent::ExitRequested` handling: implicit exits are now prevented consistently, main window is hidden instead, and only explicit quit paths run full shutdown.
- Fixed intermittent `npm run tauri dev` startup failure on Linux (`scripts/build-espeak-static.sh` exit `141`): replaced a SIGPIPE-prone `ar -t ... | head -1` cache-check pipeline (with `set -o pipefail`) by a safe `mapfile`-based first-object read, preventing false build-script aborts on valid archives.

### LLM Catalog

- Added Qwen3.5 27B Claude 4.6 Opus Reasoning Distilled model family (`eugenehp/Qwen3.5-27B-Claude-4.6-Opus-Reasoning-Distilled-GGUF`) to the LLM catalog with 17 quant variants (Q2_K through BF16/F16).
- Added OmniCoder 9B model family (`Tesslate/OmniCoder-9B-GGUF`) to the LLM catalog with 13 quant variants (Q2_K through BF16) — a coding-focused 9B model.

## [0.0.36] — 2026-03-12
### CI Runtime

- Fixed macOS updater 404: renamed the macOS updater tarball from `NeuroSkill™.app.tar.gz` (non-ASCII URL) to `NeuroSkill_<version>_aarch64.app.tar.gz` so the URL stored in `latest.json` is pure ASCII and resolves correctly in the Tauri updater HTTP client.

## [0.0.35] — 2026-03-12
### CI Runtime

- Fixed cross-platform `latest.json` merge encoding in release workflows: Windows now writes `latest.json` as UTF-8 without BOM, and Linux/macOS loaders read with `utf-8-sig` to tolerate BOM-prefixed manifests and avoid `JSONDecodeError: Unexpected UTF-8 BOM`.

## [0.0.34] — 2026-03-12

### CI Runtime

- Fixed Windows release CI `Update latest.json` step crashing with "The property 'windows-x86_64' cannot be found" when `latest.json` already exists: `ConvertFrom-Json` returns a `PSCustomObject` whose properties cannot be set by dot-notation for new hyphenated names; the workflow now uses bracket-notation for hashtable/ordered-dict platforms and `Add-Member -Force` for PSCustomObject platforms.

## [0.0.33] — 2026-03-12
### CI Runtime

- Fixed Windows release CI PowerShell parser failures in `.github/workflows/release-windows.yml` by switching `latest.json` fallback `notes` text to ASCII-safe content and removing backtick-escaped tag/version string literals in the Discord webhook payload fields.

## [0.0.32] — 2026-03-12

### CI Runtime

- Windows release workflow now auto-detects NSIS artifacts across both valid output layouts (`src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis` and `src-tauri/target/release/bundle/nsis`) in the build/sign step and artifact collection step, preventing false "bundle dir not found" / "installer not found" failures when Rust emits host-layout release outputs.

## [0.0.31] — 2026-03-12

### CI Runtime

- Release CI contributor attribution now comes only from git commit authors in the tagged release range (`previous_tag..current_tag`), and release workflows no longer append GitHub auto-generated release-note contributors.
- Windows release CI fallback for post-compile Tauri crash: `.github/workflows/release-windows.yml` now detects `npx tauri build` failure after the Rust binary is already produced (for example exit `-1073741571`/stack-overflow path), then recovers by running `npx tauri bundle --bundles nsis --no-sign`, recreating the updater zip (`*.nsis.zip`) from the generated installer, and signing that updater artifact via `npx tauri signer sign` so release publishing can continue.
- Windows release CI now uses the same primary packaging path that works locally (`npm run tauri:build:win:nsis`) instead of direct `npx tauri build` bundling, then signs the generated installer (when a cert is present) and creates/signs updater artifacts (`*.nsis.zip` + `*.nsis.zip.sig`) in workflow.
- Added npm script alias `taur:build:win:nsis` and switched Windows release CI to run `npm run taur:build:win:nsis` exactly.
- Windows release CI now installs NSIS explicitly before packaging (`choco install nsis` when `makensis` is missing), validates `makensis.exe` discovery, and exports `NSIS_DIR`/PATH so `scripts/create-windows-nsis.ps1` runs reliably on `windows-latest` runners.

## [0.0.30] — 2026-03-12

### Build / Tooling

- **Fix macOS release CI Pillow install**: added `--break-system-packages` to the `pip3 install Pillow` command in `.github/workflows/release-mac.yml` to resolve PEP 668 externally-managed-environment error on the `macos-26` runner.

## [0.0.29] — 2026-03-12

### Refactor

- macOS titlebar button order: switched the close and minimize button positions in the shared custom titlebar component so all macOS windows now use the requested control order.

- **macOS quit-time Metal teardown ordering**: added a one-time blocking shutdown helper in `src-tauri/src/lib.rs` and invoke it on explicit `RunEvent::ExitRequested` (`code = Some(_)`) before process exit continues. This now tears down LLM actor state and TTS backends earlier than `RunEvent::Exit`, reducing late `ggml-metal` static-destruction assertions (`GGML_ASSERT([rsets->data count] == 0)`) on macOS quit.

- **macOS shutdown abort in Metal teardown (`GGML_ASSERT([rsets->data count] == 0)`)**: added a blocking `Shutdown` command to the `tts/kitten.rs` worker and wired it into `tts_shutdown()` so `RunEvent::Exit` now waits for KittenTTS resources to drop before process exit/static destructor cleanup. This prevents late Metal/ggml teardown asserts when quitting after KittenTTS and LLM were active.

- **Heap-allocate AppState**: changed `Mutex<AppState>` → `Mutex<Box<AppState>>` across all Rust source files (`lib.rs`, `tray.rs`, `shortcut_cmds.rs`, `muse_session.rs`, `ws_commands.rs`, `openbci_session.rs`, `active_window.rs`, `label_cmds.rs`, `session_csv.rs`, `session_analysis.rs`, `llm/cmds.rs`, `session_dsp.rs`, `ble_scanner.rs`, `window_cmds.rs`, `history_cmds.rs`, `settings_cmds.rs`, `api.rs`, `commands.rs`, `global_eeg_index.rs`) to move the large `AppState` struct onto the heap, reducing main-thread stack frame size and mitigating stack overflow risk on platforms with smaller default stacks.
- **Extract LLM state into `Box<LlmState>`**: moved all LLM-related fields (`llm_config`, `llm_catalog`, `llm_downloads`, `llm_logs`, `llm_state_cell`, `llm_loading`, `llm_start_error`, `chat_store`) out of `AppState` into a dedicated `LlmState` sub-struct stored as `Box<LlmState>`, accessed via `s.llm.config`, `s.llm.catalog`, etc.  This further reduces `AppState`'s on-stack footprint and groups all LLM concerns behind a single heap-allocated pointer.
- **Construct AppState on a dedicated thread**: added `AppState::new_boxed()` that spawns a 32 MiB-stack thread to run `Box::new(AppState::default())`, avoiding the main-thread stack overflow that occurred on macOS when the large struct + `generate_handler!` frame exceeded the default stack limit.
- **Add macOS/Linux 32 MB main-thread stack size**: emit `-Wl,-stack_size,0x2000000` (macOS) and `-Wl,-z,stacksize=33554432` (Linux) via `cargo:rustc-link-arg-bins` in `build.rs`.  Using `rustc-link-arg-bins` instead of target-wide `rustflags` in `.cargo/config.toml` ensures the flag applies only to the final executable — ld64 rejects `-stack_size` when linking dylibs/cdylibs (the lib crate), which caused `ld: -stack_size option can only be used when linking a main executable`.
- **Extract `setup_app()` / `setup_background_tasks()`**: moved the ~650-line `.setup()` closure body and the updater/DND poll loops into separate `#[inline(never)]` top-level functions so LLVM cannot merge their stack frames with the already-huge `run()` frame produced by `generate_handler!` with ~150 commands.
- **Dynamic stack growth via `stacker`**: added `stacker = "0.1"` dependency and wrapped the `skill_lib::run()` call in `main()` with `stacker::maybe_grow(32 MiB, 64 MiB, ...)`.  This dynamically extends the main-thread stack using `mmap` + inline-asm stack-pointer swap (via `psm`) without changing the thread identity, which is required on macOS where Cocoa/AppKit mandates the event loop runs on the original main thread.  Linker flags (`-Wl,-stack_size`) were unreliable because macOS ld64 rejects them on dylibs and Tauri's mixed `crate-type = ["staticlib", "cdylib", "rlib"]` build triggers both lib and bin linking.

### Build / Tooling

- **Release notes now include changelog section**: all tagged release workflows (`.github/workflows/release-linux.yml`, `.github/workflows/release-mac.yml`, `.github/workflows/release-windows.yml`) now extract the matching `## [x.y.z]` block from `CHANGELOG.md` and pass it to `softprops/action-gh-release` via `body_path`, so GitHub Release information includes the version-specific changelog alongside generated release notes.

- **Preview artifacts now include changelog notes**: `.github/workflows/pr-build.yml` now generates `preview-notes.md` from the matching `CHANGELOG.md` `## [x.y.z]` section (based on `tauri.conf.json` version) and uploads it with the preview DMG/updater artifacts so pre-release testers get version-specific notes with each build.

- **Windows NSIS discovery false-negative fix**: corrected `scripts/create-windows-nsis.ps1` NSIS lookup when `makensis` is already on PATH. The script previously used `Split-Path` twice on `Get-Command makensis`, which resolved to the parent of the NSIS directory and could incorrectly fail `makensis.exe` checks. It now uses the direct parent folder and also accepts `NSIS_DIR` set to either the NSIS directory or a full `makensis.exe` path.

- **Windows NSIS PowerShell argument parsing fix**: corrected `scripts/create-windows-nsis.ps1` candidate-path construction to precompute `$TargetBinary`/`$HostBinary` and then build `$BinaryCandidates` from variables. This avoids a PowerShell parse/invocation edge case where comma-separated inline `Join-Path` calls inside `@(...)` could be interpreted as an array passed to `-ChildPath`, causing `Cannot convert 'System.Object[]' to the type 'System.String'`.

- **Windows NSIS standalone packaging path fallback**: `scripts/create-windows-nsis.ps1` now auto-detects the prebuilt release binary from either `src-tauri/target/x86_64-pc-windows-msvc/release/skill.exe` (explicit target build) or `src-tauri/target/release/skill.exe` (default host-target build). This fixes `npm run tauri:build:win:nsis` failing after a successful `tauri build --no-bundle` when Rust outputs the host-layout path; the script now also places NSIS output under the detected release directory's `bundle/nsis` folder.

- **macOS `.app` manual assembly fallback**: when the Tauri CLI bundler process itself stack-overflows (exit 134 SIGABRT or 139 SIGSEGV) during the `--bundles app` phase — which is a Tauri CLI issue, not the app binary — `scripts/tauri-build.js` now detects the crash, verifies the release binary was already built, and assembles the `.app` bundle manually using `ditto`, `codesign --force --deep --sign -`, the project's `Info.plist`, icons, entitlements, and resources from `tauri.conf.json`.  This makes `npm run tauri:build:mac -- --bundles app` reliable even when the Tauri CLI has stack issues.
- **Standalone macOS `.app` assembler**: added `scripts/assemble-macos-app.sh` that builds the `.app` directory structure from a pre-built release binary without invoking the Tauri CLI bundler at all.  New npm script `npm run tauri:build:mac:app` compiles with `--no-bundle` then runs the assembler.  Copies binary, merges `Info.plist` with required `CFBundle*` keys (including `CFBundleIconFile` and `NSHighResolutionCapable`), copies `icon.icns` + resources via `ditto`, and ad-hoc codesigns.
- **macOS DMG creator**: replaced the custom 800-line `scripts/create-macos-dmg.sh` with a single-pass [`appdmg`](https://github.com/LinusU/node-appdmg) approach.  Generates a branded background image (app icon + product name + version, 660×520 @1x + @2x Retina, dark/light mode adaptive) and a version-badged volume icon (`.icns` with "v0.0.28" pill overlay) via Pillow, then calls `appdmg` with a full spec: app + Applications symlink (top row), README.md + LICENSE + CHANGELOG.md (bottom row), icon positions, window size, ULFO+APFS format.  `appdmg` handles Finder view setup via AppleScript in one pass — no `hdiutil convert` round-trips that corrupt APFS volumes, no Python `ds_store`/`mac_alias` that crash Finder, no `hdiutil udifrez` SLA that corrupts DMGs on macOS 14+.  Both `release-mac.yml` and `pr-build.yml` CI workflows install `appdmg` + Pillow and use the shared script.  Also fixed `assemble-macos-app.sh` Info.plist generation: replaced regex string injection with `plistlib` so `NSHighResolutionCapable` is a proper boolean `<true/>` (not `<string>true</string>`) and added `CFBundleName`, `CFBundleDisplayName`, `LSMinimumSystemVersion`, `NSRequiresAquaSystemAppearance` keys.
- **Windows NSIS installer script**: added `scripts/create-windows-nsis.ps1` and `npm run tauri:build:win:nsis` for standalone Windows NSIS installer creation that bypasses the Tauri CLI bundler.  Generates branded installer images (header 150×57, welcome panel 164×314 with app icon + version via Pillow), bundles `README.md`, `CHANGELOG.md`, `LICENSE`, resources (espeak-ng-data, neutts-samples), creates Start Menu + Desktop shortcuts, registers in Add/Remove Programs, and optionally signs with `signtool.exe` via `CERTIFICATE_THUMBPRINT`.  The GPL-3.0 `LICENSE` is shown as a license agreement page during installation.

## [0.0.27]

### Bug Fixes

- **Feature-gated compilation for `--no-default-features` builds**: added `#[cfg(feature = "llm")]` / `#[cfg(not(feature = "llm"))]` guards in `lib.rs` (stub `llm_state_cell` field + Default impl), `api.rs` (conditional `Mutex` import), `settings_cmds.rs` (split `set_llm_config` into feature-gated paths), `tray.rs` (`ellipsize_middle` helper), and `llm/mod.rs` (`allowed_tools` field); added `#[cfg(any(feature = "tts-kitten", feature = "tts-neutts"))]` guards in `tts/mod.rs` for imports, helpers, constants, and `impl` blocks so the crate compiles cleanly when built with `--no-default-features` or any subset of feature flags.

### Dependencies

- **Auto-enable GPU backend for LLM by platform**: `llama-cpp-4` now receives `metal` on macOS and `vulkan` on Linux/Windows via target-specific dependency feature merging in `Cargo.toml`, so the `llm` feature automatically uses the correct GPU backend without requiring manual `--features llm-metal` / `--features llm-vulkan` flags.
- **Bump `llama-cpp-4` from 0.2.9 → 0.2.10** (and `llama-cpp-sys-4` 0.2.9 → 0.2.10).

### Build / Tooling

- **Fix CI release binaries missing embedded frontend**: release and preview workflows that bypass the Tauri CLI (`cargo build --release` directly) were producing dev-mode binaries that attempted to load the UI from `localhost:1420` instead of serving the embedded SvelteKit build output.  Root cause: the Tauri crate gates frontend embedding behind its `custom-protocol` Cargo feature, which `npx tauri build` activates automatically but raw `cargo build` does not.  Added a `custom-protocol` feature to `src-tauri/Cargo.toml` forwarding to `tauri/custom-protocol` and pass `--features custom-protocol` in `release-linux.yml`, `release-mac.yml`, `pr-build.yml`, and `release-windows.ps1`.
- `npm run bump` now also rotates the changelog release header automatically: it preserves a fresh `## [Unreleased]` section and inserts `## [x.y.z] — YYYY-MM-DD` for the newly bumped version.
- macOS local Tauri build stability: `scripts/tauri-build.js` now injects `--no-bundle` by default for `build` runs (while still forcing `--target aarch64-apple-darwin --no-sign`), unless the caller explicitly passes `--bundle`/`--bundles`/`--no-bundle`; this avoids post-compile bundle-phase crashes where `npx tauri build --target aarch64-apple-darwin --no-sign` fails but `--no-bundle` succeeds.

### Features

- **Hooks lifecycle completeness pass**: hook triggers now surface full runtime context (last-trigger time, matched label, and one-click session open), emit both in-app toast + native OS notification payloads, and persist immutable trigger snapshots into dedicated `hooks.sqlite` JSON audit rows; the trigger path runs in the background embedding worker with panic isolation and dedicated `hooks` logger toggles, while docs/tests/examples/locales were updated together (`SKILL.md`, `cli.ts`, `test.ts`, Help/FAQ + flow diagram, and `en`/`de`/`fr`/`he`/`uk` translations).
- **Proactive Hooks rename + scenarios**: renamed user-facing Hooks copy from “Automation Hooks” to **Proactive Hooks** and added per-hook scenario modes (`any`, `cognitive`, `emotional`, `physical`) so triggers can be gated by live state metrics in the background worker.
- **Hooks keyword picker keyboard UX**: keyword suggestions now support keyboard navigation (`↑` / `↓` / `Enter` / `Esc`) in addition to click-to-apply.
- **Hooks quick examples**: added one-click starter scenarios (cognitive deep-work guard, emotional calm recovery, physical body-break) to speed up hook creation.
- **Hooks keyword suggestions while typing**: Settings → Hooks now shows live keyword suggestions in the add-keyword flow by combining fuzzy matches from `labels.sqlite` with semantic nearest-label hits from the label text HNSW index; suggestion chips include source tags (`fuzzy`, `semantic`, or `fuzzy+semantic`) and can be clicked to add quickly.
- **Hooks button text-fit polish**: small action buttons in Hooks now use wrap-safe sizing (`h-auto` + multiline text) so localized labels fit without clipping.
- **Hooks scenario dropdown theming polish**: scenario selector now uses themed custom select styling (`appearance-none`, semantic border/ring tokens, custom chevron) for consistent dark/light appearance.
- **Hooks heading naming tweak**: Hooks tab card heading now uses the concise localized tab label ("Hooks") instead of longer variant text.
- **Settings sidebar resize**: Settings tab navigation sidebar is now mouse-resizable with a drag handle, bounded min/max width, and persisted width between opens.
- **Settings titlebar clarity**: settings window title now always includes localized “Settings” plus the active tab name (for example “Settings — Hooks”).

- **Hook distance suggestion**: new "Suggest threshold" button in Settings → Hooks that analyses real HNSW and SQLite data — finds labels matching the hook's keywords, computes cosine-distance distribution of recent EEG embeddings against those label references, and presents a percentile bar (min/p25/p50/p75/max) with a one-click "Apply" action to set the recommended threshold.
- **Hooks WS/CLI observability expansion**: added websocket commands `hooks_suggest` and `hooks_log`, plus CLI subcommands `hooks suggest` and `hooks log` (limit/offset pagination) for scriptable threshold recommendations and audit-log inspection over either WebSocket or HTTP tunnel transport.
- **Hook fire history viewer**: expandable "Hook fire history" section in Settings → Hooks with paginated (20/page) collapsible event rows showing timestamp, label, distance, command, and threshold-at-fire metadata.
- **Last-trigger relative age**: the last-trigger display in Settings → Hooks now shows a live relative-time label (e.g. "12s ago", "3m ago") that updates every second alongside the absolute timestamp.
- Added a new **Settings → Hooks** tab for user-defined automation hooks: each hook supports name, enabled flag, multiple keywords, command payload, custom text payload, configurable EEG distance threshold, and configurable recent-reference count (clamped to 10–20).
- Added backend hook persistence and runtime matching pipeline: hook rules are saved in `settings.json`, hook keyword queries use fuzzy matching plus text-embedding/HNSW nearest-label expansion, then map to recent label-window EEG references; incoming EEG embeddings now trigger websocket broadcasts when close enough, with payload `{ hook, context: "labels", command, text }`.

### Documentation

- **Proactive Hooks docs/examples refresh**: updated `SKILL.md` hooks scenarios and jq examples, refreshed CLI help/output text in `cli.ts` to include scenario metadata, and extended `test.ts` hook status smoke checks to validate `hook.scenario` when hooks exist.
- Added hooks explainers in Help/FAQ including a compact hook flow diagram and a dedicated trigger-mechanics FAQ entry.

### Bug Fixes

- **Single-instance runtime enforcement**: app startup now initializes `tauri-plugin-single-instance`, so opening NeuroSkill while it is already running no longer starts a second process; the existing `main` window is restored/focused instead.

- **Windows CI Rust warning cleanup (`dead_code`)**: removed the non-Linux `linux_has_appindicator_runtime()` stub from `src-tauri/src/lib.rs` so only the Linux implementation is compiled; this eliminates the Windows-only `function is never used` warning while preserving the Linux tray-runtime guard behavior.

### Documentation

- **README Linux packaging quickstart added**: added a concise Development-section command block in `README.md` for Linux release-style local packaging (`tauri:build:linux:x64:native` for AppImage, then `package:linux:system:x64:native -- --skip-build` for manual `.deb`/`.rpm`), including an explicit `ALLOW_LINUX_CROSS=1` cross-target example.
- **Linux setup docs now include tray runtime dependency guidance**: updated `LINUX.md` with a dedicated runtime prerequisite for `tauri dev` (`libayatana-appindicator3-1`, with `libappindicator3-1` fallback) and added troubleshooting steps for the startup error `Failed to load ayatana-appindicator3 or appindicator3 dynamic library`.
- **Linux docs cross-link clarity pass**: added a reciprocal pointer in `LINUX.md` back to `README.md` Development prerequisites and explicit wording that missing appindicator runtime packages can break `npm run tauri dev` at startup.
- **Linux packaging command docs aligned with workflows**: updated the `LINUX.md` build section to recommend the canonical local flow (`npm run tauri:build:linux:x64:native` for AppImage, then `npm run package:linux:system:x64:native -- --skip-build` for `.deb`/`.rpm` via `dpkg-deb`/`rpmbuild`), with cross-target examples when `ALLOW_LINUX_CROSS=1` is intentional.

### Bug Fixes

- **Rust clippy warning cleanup (embeddings/settings)**: marked argument-heavy constructor/spawn entry points in `src-tauri/src/eeg_embeddings.rs` with targeted `#[allow(clippy::too_many_arguments)]` (matching the existing worker rationale), and replaced the manual `Default` implementation for `HookStatus` with `#[derive(Default)]` in `src-tauri/src/settings.rs`.
- **Rust hooks settings compile fix (`E0596`)**: fixed `set_hooks` in `src-tauri/src/settings_cmds.rs` by binding the locked app state as mutable before assigning `s.hooks`, resolving `cannot borrow 's' as mutable, as it is not declared as mutable` during `cargo clippy`/build.
- **Linux tray is now mandatory with fail-fast startup guard**: before tray initialization, startup probes for loadable appindicator shared objects; when `libayatana-appindicator3` / `libappindicator3` is missing, startup aborts immediately with a clear prerequisite error instead of panicking inside `libappindicator-sys` or running without tray.
- **Linux `tauri dev` tray-runtime preflight**: `scripts/tauri-build.js` now checks for a loadable appindicator runtime (`libayatana-appindicator3.so*` or `libappindicator3.so*`) before launching `npx tauri dev`; when missing, it exits early with distro-aware install guidance (`apt`/`dnf`/`pacman`/`zypper`) instead of letting the app crash at startup with a `libappindicator-sys` panic.
- **`npm run bump` Linux preflight dependency clarity**: added an explicit `pkg-config` guard before `cargo clippy` in `scripts/bump.js` that checks `webkit2gtk-4.1`, `javascriptcoregtk-4.1`, and `libsoup-3.0`; when missing, bump now fails fast with actionable `apt install` guidance instead of surfacing a lower-level `webkit2gtk-sys` build-script crash.
- **Strictest non-status accent normalization (UMAP/Embeddings)**: removed remaining category-only orange/sky/emerald/violet highlight styling in UMAP and Embeddings controls (preset chips, pipeline badges, slider thumb/focus affordance, and dimension legend badges) in favor of semantic `primary` / `ring` tokens so generic interactive emphasis consistently follows Appearance accent settings.
- **Strict accent policy completion for generic selectors**: updated the remaining non-status selected controls in Calibration profile editing (break-duration and iterations chips) to use semantic `primary` tokens instead of hardcoded `amber`/`emerald`, and clarified `AGENTS.md` guidance that semantic status colors remain allowed only for true status signaling.
- **Follow-up accent normalization for non-status highlights**: converted remaining generic hardcoded `rose`/`emerald` selection and focus styles (UMAP timeout/cooldown controls, EEG overlap selector summary badges, and interactive search query focus ring) to semantic `primary` / `ring` tokens, while leaving semantic success/warning/error colors unchanged.
- **Broader accent-token consistency sweep**: replaced numerous hardcoded interactive blue states (selected chips/buttons, focus rings, and status badges) with semantic `primary` / `ring` tokens across Appearance, Settings, Focus Timer, History, Labels, Calibration, API, Search, and related tabs so accent-like UI feedback consistently follows the Appearance accent mapping.
- **Accent setting now applies to native form controls and remaining interactive toggles**: added a global `accent-color` base rule tied to the remapped accent palette so checkboxes/radios/ranges/progress controls follow the selected Appearance accent, and replaced remaining hardcoded non-remapped accent classes in interactive Search/UMAP controls.
- **Updater fallback on install failure**: when automatic update download/install fails in the Updates tab, the UI now gives an explicit "download online" fallback and automatically opens the latest GitHub releases page (`https://github.com/NeuroSkill-com/skill/releases/latest`) so users can immediately fetch the newest installer manually.
- **macOS white screen on first launch**: `win.show()` was called in Tauri's `setup` closure before WKWebView had loaded any content, producing a solid white frame until the next compositor cycle.  Fixed by removing the eager `setup` show and adding a new `show_main_window` Tauri command that is invoked from `+layout.svelte` `onMount`; the window now becomes visible only after the page has fully rendered.  Secondary windows (settings, help, calibration, etc.) and the new-user onboarding flow are unaffected — `show_main_window` is a no-op for any window whose label isn't `"main"` or whose onboarding flag is unset.
- **What's New version picker theme mismatch**: the navigation dropdown in `/whats-new` used transparent/native select styling that could ignore app theme colors in the standalone window. The picker now uses explicit themed control styles (`appearance-none`, theme-aware background/border/text) plus a custom caret so light/dark appearance matches the rest of the UI.
- **Appearance accent color not applied consistently across UI**: accent selection previously remapped only Tailwind `violet-*` variables, while many controls and gradients used `blue-*`, `indigo-*`, or `sky-*` classes and stayed on default hues. Accent application now remaps those accent-like families together so interactive highlights, rings, sliders, and accent gradients consistently follow the selected Appearance accent.

### CI Runtime

- Windows release workflow reliability fix: `.github/workflows/release-windows.yml` now uses ASCII-safe Discord title strings in the notify step to avoid Windows PowerShell parser/encoding failures, and the Tauri build step now runs with `--verbose` plus bundle-directory diagnostics when `npx tauri build` exits non-zero (so packaging failures surface actionable logs instead of a bare exit code).
- Linux release workflow now bypasses Tauri bundling entirely (macOS-style): it compiles frontend + Rust only, builds `.deb`/`.rpm` via `scripts/package-linux-system-bundles.sh`, builds the portable Linux tarball via `scripts/package-linux-dist.sh`, signs those outputs with `tauri signer`, and publishes updater metadata from the signed portable tarball instead of AppImage bundle artifacts.
- CI Linux packaging scope reduced to tarball-only in `.github/workflows/ci.yml`: removed the `linux-release` job that produced `.deb`/`.rpm`/`.AppImage`, so Linux CI now only runs the portable package flow and publishes `.tar.gz` artifacts.
- Tauri frontend bundling contract guard: added `scripts/verify-tauri-frontend-structure.js` and wired it into `npm run build` (`package.json`) so `tauri build` (via `beforeBuildCommand`) now fails fast unless the configured `src-tauri/tauri.conf.json` `build.frontendDist` path contains valid built assets (`index.html` + `_app/immutable` JS/CSS) rather than raw source files.
- Linux/macOS/Windows bundling workflows now run an explicit `npm run -s verify:tauri:frontend` step before packaging (`.github/workflows/ci.yml`, `.github/workflows/release-linux.yml`, `.github/workflows/release-mac.yml`, `.github/workflows/release-windows.yml`) to enforce the same Tauri asset layout contract in CI.
- Windows release Discord notifier fix: `.github/workflows/release-windows.yml` now sends the Discord payload from a PowerShell object serialized via `ConvertTo-Json` (instead of shell-escaped inline JSON), eliminating Discord API `50109` (`The request body contains invalid JSON`) failures after successful Windows builds.
- Windows release post-build hardening: `.github/workflows/release-windows.yml` now updates `latest.json` with native PowerShell (no `python3` dependency in Git Bash on `windows-latest`) and skips the Discord notification step when `DISCORD_WEBHOOK_URL` is unset, avoiding non-build-related exit failures after successful Windows artifact compilation.
- macOS release bundle frontend integrity: `.github/workflows/release-mac.yml` now copies the generated SvelteKit `build/` output into `Contents/Resources/app` with `ditto` during manual `.app` assembly and fails fast if `build/index.html`, copied `index.html`, copied `_app/immutable`, or copied JS/CSS assets are missing, preventing release artifacts that omit frontend HTML/JS/CSS/static files.
- Linux CI + release packaging now avoids Tauri for `.deb`/`.rpm`: both `.github/workflows/ci.yml` and `.github/workflows/release-linux.yml` build only AppImage via `tauri-build.js --bundles appimage`, then run `scripts/package-linux-system-bundles.sh` to generate `.deb` with `dpkg-deb` and `.rpm` with `rpmbuild`; this removes Tauri Linux deb/rpm bundler segfaults from automated Linux build paths while keeping artifact outputs unchanged.
- Linux workflow/script consistency pass: `package.json` Linux Tauri scripts (`tauri:build:linux:arm64`, `tauri:build:linux:x64:native`, `tauri:build:linux:x64`) now target AppImage-only bundling, and both Linux workflows call the npm script entrypoint for the AppImage build before running manual system-tool `.deb`/`.rpm` packaging.
- Linux CI/release workflow hardening: added native Linux x86_64 npm scripts (`tauri:build:linux:x64:native`, `package:linux:portable:x64:native`) and switched `.github/workflows/ci.yml` + `.github/workflows/release-linux.yml` to those scripts so hosted x86_64 runners no longer depend on `ALLOW_LINUX_CROSS` cross-mode execution paths.
- Linux CI execution policy refinement: in `.github/workflows/ci.yml`, heavy Linux bundling jobs (`linux-release` and `linux-portable-package`) now run by default on `push`, and can be explicitly enabled for manual `workflow_dispatch` runs via `run_linux_bundles=true`, keeping pull-request CI focused on faster validation.
- Updated GitHub Actions workflows to Node 24-ready action versions across CI and release workflows: `actions/checkout` → `v6`, `actions/setup-node` → `v6`, `actions/cache` → `v5`, and `Swatinem/rust-cache` → `v2.9.0`, removing the GitHub deprecation warnings about Node 20-based actions.
- Removed the Linux Rust job's apt archive cache from `.github/workflows/ci.yml`; that cache was low-value on hosted runners and was the most likely source of the `/usr/bin/tar` post-job save failure that was making the Rust CI job noisy or red despite successful build steps.
- Reintroduced Linux Tauri system dependency caching in CI and Linux release workflows via `awalsh128/cache-apt-pkgs-action` (`.github/workflows/ci.yml`, `.github/workflows/release-linux.yml`) so WebKit/GTK build dependencies are restored from cache instead of re-downloaded on every run.

### UI / Type Safety

- **Settings window width bump**: increased the default Settings window width from `680` to `760` (height unchanged) so tabs and controls have more horizontal room; applied consistently to Settings/Model/Updates entry paths that create the shared `settings` window.

### What's New window

- **Full changelog navigation**: the What's New window now parses the entire bundled `CHANGELOG.md` (via Vite `?raw` import) into individual version sections and renders each one with `MarkdownRenderer`; a compact navigation bar between the header and body provides "Newer ←" / "Older →" arrow buttons and a version-picker `<select>` dropdown so users can browse every release entry from a single window; scroll position resets to the top on each navigation step; a `1 / N` counter in the footer shows the current position; new i18n keys (`whatsNew.older`, `whatsNew.newer`, `whatsNew.unreleased`) added to all five locales (en, de, fr, he, uk)


- Reduced the untyped `any` surface in the Three.js-heavy UI components by introducing explicit typed scene/object wrappers in `src/lib/UmapViewer3D.svelte` and `src/lib/InteractiveGraph3D.svelte`; removed broad `any` refs and `@ts-ignore`, and kept behavior unchanged while making future refactors compile-time safer.

### i18n (0.0.4)

- Localized updater fallback messaging across all shipped locales (`en`, `de`, `fr`, `he`, `uk`) by adding translated keys for: (1) automatic-update install failure with online download guidance, and (2) failure to auto-open the download page; `UpdatesTab.svelte` now uses i18n keys instead of hardcoded English strings for both paths.
- Fixed a locale key-sync detection edge case for `de`, `fr`, `he`, and `uk`: normalized `llm.tools.locationDesc`, `llm.tools.webSearchDesc`, and `llm.tools.webFetchDesc` entries to standard `"key": "value"` formatting so `scripts/sync-i18n.ts --check` correctly counts them
- Ran `scripts/sync-i18n.ts --fix` to auto-backfill 138 missing keys in `src/lib/i18n/he.ts` with English fallbacks, restoring locale key-count parity (`2237` keys) so `npm run sync:i18n:check` passes.
- Completed German fallback translation coverage in [src/lib/i18n/de.ts](src/lib/i18n/de.ts) for the auto-synced OpenBCI/LLM/chat/help/downloads blocks and removed stale in-file TODO translation markers in that locale.
- Completed French/Hebrew/Ukrainian fallback translation coverage in [src/lib/i18n/fr.ts](src/lib/i18n/fr.ts), [src/lib/i18n/he.ts](src/lib/i18n/he.ts), and [src/lib/i18n/uk.ts](src/lib/i18n/uk.ts) for the same auto-synced OpenBCI/LLM/chat/help/downloads blocks, and removed stale in-file TODO translation markers.
- Fixed French placeholder consistency regression in [src/lib/i18n/fr.ts](src/lib/i18n/fr.ts) by restoring `llm.size` interpolation token to `{gb}` so runtime formatting and placeholder-consistency tests align.

### Focus / DND

- Linux Do Not Disturb automation support: implemented real Linux backend behavior in `src-tauri/src/dnd.rs` instead of non-macOS no-ops, with GNOME integration via `gsettings org.gnome.desktop.notifications show-banners` and KDE integration via `qdbus(6)` `org.kde.osdService.setDoNotDisturb`; OS-state polling now reports Linux DND state when detectable
- Linux DND fallback path: when GNOME and KDE DND APIs are unavailable, the backend now falls back to `xdg-desktop-portal` inhibit requests (`gdbus` to `org.freedesktop.portal.Inhibit`) with tracked request-handle lifecycle so disable calls close previously created portal requests
- Windows Do Not Disturb automation support: implemented a Windows backend in `src-tauri/src/dnd.rs` using the per-user notification banner toggle (`HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\PushNotifications\\ToastEnabled`) for enable/disable and OS-state query so focus automation works on Windows as well

### Repo hygiene

- Cleaned editor hygiene warnings by switching release workflows away from fragile cross-step `${{ env.* }}` references in expression contexts, and by normalizing historical changelog markdown structure so repo diagnostics stay quiet.
- Fixed a Rust docs lint warning (`clippy::doc_lazy_continuation`) in [src-tauri/src/dnd.rs](src-tauri/src/dnd.rs) by splitting the Linux bullet list and the Windows support note into separate rustdoc paragraphs.

### Build / CI (Unreleased)

- macOS `aarch64-apple-darwin` Tauri build fix: moved `MACOSX_DEPLOYMENT_TARGET` and `CMAKE_OSX_DEPLOYMENT_TARGET` into top-level Cargo `[env]` scope in `src-tauri/.cargo/config.toml` (they were accidentally nested under `[target.i686-pc-windows-gnu.env]`), so `llama-cpp-sys` now receives a 10.15 deployment target and avoids `std::filesystem` availability errors (`'path' is unavailable: introduced in macOS 10.15`) during CMake/C++ compilation.
- `npm run bump` now runs mandatory preflight gates before mutating versions: `npm run check`, `cargo clippy --manifest-path src-tauri/Cargo.toml`, then `npm run sync:i18n:check`; if any step fails, bump exits immediately and does not update version fields.
- Linux CI bundle stability: `scripts/tauri-build.js` now detects a Tauri CLI segfault (`exit 139`) during explicit multi-target bundle runs (for example `--bundles deb,appimage`) and automatically retries bundling sequentially per target so release jobs can still produce both `.deb` and `.AppImage` artifacts
- Linux CI single-target bundle stability: when an explicit Linux bundle run (for example `--bundles deb`) exits with `139`, `scripts/tauri-build.js` now verifies the expected bundle output for that target and treats the run as successful only if artifacts are present; the same artifact-aware tolerance is also applied per-target during sequential retry after a multi-target segfault.
- Linux CI per-target recovery hardened: when a Linux `tauri build --bundles <target>` run exits `139` before writing bundle artifacts, `scripts/tauri-build.js` now retries that target with `tauri bundle --bundles <target>` and only fails if expected artifacts are still missing after the fallback path.
- Linux CI release-bundle smoke test now fails if no `.deb` package is produced: `.github/workflows/ci.yml` verifies at least one `.deb` exists after bundling and checks both the explicit target-triple bundle path and fallback non-target path to catch segfault-recovery path regressions.
- Linux ARM64 build fallback (macOS-style crash isolation): for explicit bundle builds where Tauri crashes with `139`/`134` but the release binary already exists, `scripts/tauri-build.js` now exits successfully in compile-only mode and prints guidance; set `DISABLE_LINUX_CRASH_COMPILE_FALLBACK=1` to force hard failure.
- Added standalone Linux distribution packaging script `scripts/package-linux-dist.sh` to avoid Tauri bundling: it builds with `--no-bundle`, assembles `NeuroSkill/` (binary, bundled resources, launcher, icon, desktop entry, docs), and emits a portable `tar.gz` archive under `dist/linux/<target>/`.
- Added CI portable-package job in `.github/workflows/ci.yml`: `linux-portable-package` now runs `npm run package:linux:portable:x64`, verifies the generated `dist/linux/x86_64-unknown-linux-gnu/*.tar.gz`, and uploads it as a GitHub Actions artifact (`linux-portable-x86_64`).
- Added Linux `.deb` artifact upload in CI: the `linux-release` job in `.github/workflows/ci.yml` now resolves the generated package from the target/fallback bundle paths and uploads it as `linux-deb-x86_64` for direct download from Actions runs.
- Linux package matrix expanded to include `rpm`: Linux build scripts now request `--bundles deb,appimage,rpm`, and both CI/release workflows were updated to validate and publish `.rpm` alongside `.deb` and `.AppImage` artifacts.
- Added Linux integrity sidecars: workflows now generate `SHA256SUMS` files for Linux bundle outputs and portable tarball outputs, and `release-linux` now also generates detached `.sig` signatures for Linux release artifacts.
- Linux release stale-artifact guard: `.github/workflows/release-linux.yml` now removes cached `src-tauri/target/x86_64-unknown-linux-gnu/release/{bundle,skill}` and `dist/linux/x86_64-unknown-linux-gnu` before compile/package steps so rust-cache leftovers cannot be mistaken for fresh artifacts when assembling release outputs.
- Linux CI parity stale-artifact guard: `.github/workflows/ci.yml` now performs the same pre-build cleanup in the `linux-portable-package` job, clearing cached `target`/`dist` Linux output paths before packaging so uploaded CI tarballs always come from the current run.

## [0.0.24] — 2026-03-12

### UI

- Label window titlebar spacing + vertical fit: moved the add-label window title back to the side, rendered the EEG timer as a padded centered capsule in the shared titlebar, and changed `/label` from `h-screen` to `h-full min-h-0` so the bottom action row no longer gets clipped under the custom titlebar layout
- What's New window vertical fit fix: changed `/whats-new` root container from `h-screen` to `h-full min-h-0` and marked the changelog body as `min-h-0` so the shared custom titlebar no longer pushes the footer off-screen and the bottom `Got it` button remains visible
- Window vertical-fit sweep: switched the remaining titlebar-hosted route roots (`/`, `/about`, `/api`, `/calibration`, `/chat`, `/compare`, `/downloads`, `/focus-timer`, `/help`, `/history`, `/labels`, `/onboarding`, `/session`, `/settings`) from viewport height to parent-constrained height, adding `min-h-0` to the key scroll containers where needed so shared custom-titlebar layouts no longer clip bottom content or footers
- Search window titlebar center alignment: moved the mode segmented control to a true centered position in the shared titlebar (absolute center anchoring), increased control width budget, and tuned spacing/typography so all mode buttons render fully and stay visually aligned
- Label window titlebar timer: moved the live EEG-window elapsed timer from the add-label page header into the shared `CustomTitleBar` center area via a new `label-titlebar.svelte.ts` reactive store, removing the duplicate in-content strip while keeping the timer live
- Search window vertical fit fix: changed `/search` root container from `h-screen` to `h-full min-h-0` so it honors the `#main-content` constrained height under the custom 30px titlebar and no longer overflows/clips at the bottom
- Search window titlebar button rendering fix: updated the shared `CustomTitleBar` search layout to be shrink-safe (`search-window-head` + `search-mode-switch` now flex responsively, title truncates with ellipsis, and mode buttons use equal-width flex sizing) so all search mode buttons render reliably instead of clipping on narrower windows/locales
- History window titlebar consolidation: moved clock icon, title text, day pagination (prev/next + label + position indicator), compare toggle, labels toggle, and reload button from the in-page header into the shared custom titlebar via a new `history-titlebar.svelte.ts` reactive store and callbacks; the in-page header strip is removed and the history page retains only the labels browser panel and scroll content
- Help window titlebar consolidation: moved the search input, version badge, license label, ThemeToggle, and LanguagePicker from the in-page top bar into the shared custom titlebar via a new `help-search-state.svelte.ts` reactive store; the redundant in-page header strip is removed and the search state is shared between the help page and the titlebar seamlessly
- Fixed all windows being clipped at the bottom by exactly the custom titlebar height (30 px): `#main-content` now uses `box-sizing: border-box; height: 100vh` so the `padding-top: 30px` offset is contained within the viewport height rather than overflowing beneath the body's `overflow: hidden` boundary
- Settings window titlebar consolidation: moved the Settings title label, Help button, ThemeToggle, and LanguagePicker from the in-page top bar into the shared custom titlebar; the redundant in-page header strip is removed and the Help button is shown in the titlebar actions whenever the settings window is active
- API Status window: moved title and Refresh button from the in-page header into the shared custom titlebar; the title bar now shows the window title on all platforms, with a refresh icon button next to ThemeToggle and LanguagePicker; the in-page header section is removed
- Search window titlebar alignment: moved Search title and mode toggle buttons (EEG/Text/Interactive) from the in-content header into the shared custom titlebar, with mode switching synchronized between the titlebar and `/search` content
- Updated the shared custom titlebar to show each non-main window title in the titlebar itself and to scope main-only titlebar actions (label/history) to the main window; non-main windows now keep lightweight titlebar controls (theme/language + window controls)
- Removed duplicate in-content title bars from all secondary windows (about, compare, whats-new, focus-timer, session, labels, search, history, calibration, label, onboarding, chat); functional header controls (mode buttons, day pagination, compare toggle, recording badge, elapsed timer, TTS indicator) are preserved in-place while redundant title text, drag regions, and theme/language buttons are removed
- Added global themed scrollbar styling for app scroll containers so Windows windows no longer show default system scrollbars; includes light/dark variants and automatic fallback to system colors in forced-colors mode

### LLM

- Moved per-session LLM transcript files into a dedicated `~/.skill/llm_logs` directory (`skill_dir/llm_logs/llm_<unix-seconds>.txt`) so all LLM logs live in a standalone folder instead of the `skill_dir` root.
- Added i18n translations for all LLM built-in tool toggle labels and descriptions across all five supported locales (en, de, fr, he, uk); `TOOL_ROWS` in `LlmTab.svelte` is now a reactive `$derived` so labels update instantly on language change
- Added per-tool allow-list settings for LLM chat in Settings → LLM; `date`, `location`, `web_search`, and `web_fetch` can now be enabled or disabled individually, and running chat requests pick up the updated tool allow-list immediately
- Multimodal projector selection now stays attached to a compatible downloaded text model instead of behaving like a standalone model; selecting an `mmproj` can auto-pair to a matching downloaded LLM, incompatible projector selections are cleared when the base model changes, and startup now honors the resolved projector path when autoload is enabled
- Added simple built-in tool-calling support in `POST /v1/chat/completions` with a bounded execution loop for `date`, `location`, `web_search`, and `web_fetch`
- Wired Tauri IPC chat streaming (`chat_completions_ipc`) to the same tool-calling loop so the in-app chat window supports the same built-in tools
- IPC chat now emits incremental visible `delta` chunks while tool-calling runs, using a stream sanitizer that suppresses `[TOOL_CALL]...[/TOOL_CALL]` blocks from the UI
- Added tool schema injection and `[TOOL_CALL]...[/TOOL_CALL]` handling so models can call tools and continue generation with tool results
- Added basic external fetch/search integrations (`ipwho.is`, DuckDuckGo instant answer API, and HTTP(S) page fetch) with bounded payload truncation for safe prompt context

### Dependencies (0.0.17)

- `llama-cpp-4` `0.2.7` → `0.2.9` (with matching `llama-cpp-sys-4` lockfile update)

### Build / CI

- Windows release workflow stability fix: `.github/workflows/release-windows.yml` now generates the temporary Tauri `--config` JSON via PowerShell (`ConvertTo-Json`) instead of `bash` + `python3`, removing a fragile command-path dependency that could fail the post-compile build step with exit `127` on `windows-latest`
- Linux release artifact generation fixed: `scripts/tauri-build.js` now treats both `--bundle` and `--bundles` (including `--flag=value`) as explicit bundling requests, preventing accidental `--no-bundle` injection that skipped `.deb`/`.AppImage` outputs in CI
- Added explicit Linux bundle-flag guard steps in CI and release workflows to fail fast if `tauri:build:linux:x64` drops `--bundles deb,appimage` or if `scripts/tauri-build.js` stops recognizing `--bundles`
- Added post-build Linux bundle directory sanity checks in CI and release workflows to fail early when `bundle/deb` or `bundle/appimage` is missing

## [0.0.23] — 2026-03-12

### UI / Build (0.0.23)

- **Custom titlebar for all windows** — replaced native window decorations with a custom titlebar component (minimize, maximize, close buttons) for consistent cross-platform appearance on all windows including main, settings, help, search, history, calibration, chat, downloads, and more
- **Unified window close behavior across all platforms** — on all platforms including Linux, closing the main window now hides it instead of exiting. Users must select "Quit" from the tray menu to exit, which shows a confirmation dialog
- **Downloads window total size footer** — the standalone Downloads window now shows the combined size of all listed downloads in a bottom footer for quick storage visibility
- **Downloads footer visibility improved** — clarified the footer label to “Total download size”, added item count, and increased footer emphasis so the summary is easier to notice
- **Downloads status bar placement** — moved the total-size summary from the bottom footer to an always-visible status bar directly under the Downloads header
- **Custom titlebar controls centralized** — titlebar minimize/maximize/close now use a single shared Svelte handler path (no per-window DOM-id listener wiring), improving consistency across windows
- **All windows aligned to shared custom titlebar path** — added missing window-capability labels (`history`, `compare`, `downloads`, `whats-new`), routed shortcut-created Chat/History windows through shared open-window commands, and ensured recreated main window remains undecorated so custom drag/control behavior is uniform
- **Main window titlebar consolidation** — moved language picker, theme toggle, label, and history buttons from the main card header to the titlebar for a cleaner, more accessible layout; buttons remain icon-only and responsive
- **Titlebar spacing refinement** — action buttons (label, history, theme, language) now live on the left side with window controls (minimize, maximize, close) on the right side, utilizing flex layout for proper visual separation
- **Linux cross-target preflight guard** — `scripts/tauri-build.js` now fails fast when a Linux host attempts a non-native `*-unknown-linux-gnu` target (for example ARM host → x86_64) without explicit opt-in, and prints actionable guidance; this avoids long builds ending in `glib-sys` / `gobject-sys` `pkg-config` cross-compilation failures
- **Linux build docs updated for ARM hosts** — added `pkg-config` cross-compilation troubleshooting to `LINUX.md`, including native ARM build command guidance and recommended x86_64 release build strategy
- **Native ARM64 Linux build shortcut** — added `npm run tauri:build:linux:arm64` to run the correct local aarch64 target build (`deb` + `AppImage`, `llm-vulkan`) in one command
- **Explicit Linux x64 cross-build shortcut** — added `npm run tauri:build:linux:x64`, which sets `ALLOW_LINUX_CROSS=1` and then runs the x86_64 target build path; this keeps accidental cross-target builds blocked by default while allowing intentional ones
- **CI Linux build command aligned with npm scripts** — `.github/workflows/ci.yml` now runs `npm run tauri:build:linux:x64` for the Linux release bundle smoke test instead of an inline `npx tauri build ...` command, keeping CI and local build entrypoints consistent
- **Tagged Linux release workflow aligned with npm scripts** — `.github/workflows/release-linux.yml` now also runs `npm run tauri:build:linux:x64` (with existing signing/env vars), replacing the inline `npx tauri build ...` command so both CI and release workflows share the same build entrypoint
- **Workflow intent comments added** — both `.github/workflows/ci.yml` and `.github/workflows/release-linux.yml` now include inline comments noting that `tauri:build:linux:x64` intentionally sets `ALLOW_LINUX_CROSS=1`, reducing accidental regressions to implicit cross-build behavior

### Bug fixes (Linux)

- **Main window close/minimize/maximize buttons unresponsive** — on Linux
  (Wayland + GNOME/Mutter/KWin), window decoration buttons did nothing
  after the window was created with `visible(false)` and later shown;
  this is a known upstream issue (tauri-apps/tauri#11856); worked around
  by toggling fullscreen briefly after every `show()` call on the main
  window (`linux_fix_decorations()`), which forces the Wayland compositor
  to re-evaluate decoration state; applied in initial setup show,
  `show_and_recover_main()`, and `complete_onboarding()`
- **Window event diagnostic logging** — added `[window-event]` and
  `[run-event]` stderr logging for `CloseRequested`, `Destroyed`,
  `Focused`, `Moved`, `Resized`, `ScaleFactorChanged`, and
  `ExitRequested` events across all windows

### Onboarding (0.0.23)

- **Downloads complete success screen** — when all recommended models
  (Qwen3.5 4B, ZUNA encoder, NeuTTS, Kitten TTS) are downloaded, the
  onboarding done step now displays a prominent **green checkmark** with
  a success message and a clickable link to **settings** where users can
  download additional models or switch to alternatives
- **Downloads complete i18n** — added `onboarding.downloadsComplete`,
  `onboarding.downloadsCompleteBody`, and `onboarding.downloadMoreSettings`
  keys to all five locales (en, de, fr, he, uk)

## [0.0.17] — 2026-03-11

### UI / Build (0.0.17)

- **Tailwind Vite parser crash in MarkdownRenderer fixed** — resolved
  `[plugin:@tailwindcss/vite:generate:serve] Invalid declaration: Marked`
  by refactoring `src/lib/MarkdownRenderer.svelte` to use `marked.parse(...)`
  with a local renderer object and removing an empty local `<style>` block
- **MarkdownRenderer regression guard** — added
  `scripts/check-markdown-renderer.js` and wired it into `npm run check`
  so CI/local checks fail if `MarkdownRenderer.svelte` reintroduces
  `new Marked(...)` or a local `<style>` block
- **MarkdownRenderer guard now runs before dev startup** — `npm run dev`,
  `npm run build`, `npm run check:watch`, and `npm run tauri dev` now execute
  the MarkdownRenderer guard before Vite / SvelteKit startup so Tailwind
  parser regressions fail immediately instead of surfacing later from the
  Tailwind Vite pipeline

### Settings

- **Open `skill_dir` from Settings** — Data Directory now includes an
  **Open Folder** action that opens the fixed `~/.skill` directory in the
  system file manager

### Onboarding (0.0.17)

- **Recommended models quick setup** — onboarding now includes a one-click
  **Download Recommended Set** action that pulls the default local stack:
  **Qwen3.5 4B (Q4_K_M)**, **ZUNA encoder**, **NeuTTS**, and **Kitten TTS**
- **Qwen quant preference tightened** — when selecting the onboarding LLM
  target, the wizard now explicitly prefers **Q4_K_M** for Qwen3.5 4B
- **Staged background downloads** — onboarding now starts the recommended
  model downloads in sequence while the user continues setup: ZUNA →
  KittenTTS → NeuTTS → Qwen3.5 4B (`Q4_K_M` target)
- **Persistent footer model status** — all onboarding views now show a subtle
  footer line with staged model setup progress, and the onboarding window was
  enlarged slightly to keep spacing readable
- **Download order configured in Rust constants** — the onboarding queue no
  longer hardcodes download order in Svelte; it now reads the canonical
  sequence from `src-tauri/src/constants.rs`
- **Onboarding models i18n complete** — added the 16 missing
  `onboarding.step.models`, `onboarding.modelsHint`, `onboarding.modelsTitle`,
  `onboarding.modelsBody`, and `onboarding.models.*` keys to all four
  non-English locales (de, fr, he, uk)

### Tray / Downloads

- **LLM download progress in tray icon + menu** — while model files are
  downloading, the system tray now shows progress in the icon itself (a
  prominent circular ring around the tray icon) and in the tray menu
  (active download rows with filename, percent and live status text)
- **Standalone Downloads window** — added a dedicated downloads manager
  window (`/downloads`) that lists all model downloads at any time with
  per-item actions: pause, resume, cancel, and delete
- **Download initiated timestamp** — each download row now includes when it
  was started so long-running and resumed transfers are easier to track
- **Downloads i18n** — new downloads-window labels/status strings added to
  all shipped locales
- **Tray menu shortcut to Downloads** — added a direct **Downloads…** menu
  action in the tray, opening the standalone downloads window in one click

### Dependencies

- `llama-cpp-4` `0.2.6` → `0.2.7`

### CI / Build

- **Linux local `tauri build` segfault avoided** — `scripts/tauri-build.js`
  now injects `--no-bundle` by default for Linux `build` runs when the caller
  does not explicitly pass `--bundle` / `--no-bundle`; this avoids a native
  post-compile crash (status 139) in the Tauri CLI bundling/updater phase
  while still producing the release binary at
  `src-tauri/target/release/skill`

- **Windows release — wrong `link.exe`** — the GitHub-hosted `windows-latest`
  runner places `C:\Program Files\Git\usr\bin` (Git for Windows' Unix `link`
  utility) before the MSVC toolchain in `PATH`; Rust's MSVC backend resolved
  `link.exe` to that Unix binary, which rejected all MSVC linker flags with
  an "extra operand" error; fixed by adding a PowerShell step immediately
  after `ilammy/msvc-dev-cmd` in `release-windows.yml` that strips every
  `Git\usr\bin`-like entry from `PATH` via `$GITHUB_ENV`, ensuring the MSVC
  `link.exe` wins for all subsequent steps

---

## [0.0.16] — 2026-03-11

### EEG / Embeddings

- **Cross-day HNSW index** — similarity search is no longer scoped per-day;
  a persistent cross-day index (rolling 30-day merged index) is maintained
  under `~/.skill`; near-neighbours across months can be found in a single
  query
- **Label fuzzy semantic search** — label search now uses the vendored
  `fast-hnsw` label index for semantic matching in addition to plain-text
  filtering; queries like "find sessions where I felt anxious" surface
  nearest-neighbour label clusters rather than exact string hits

### LLM — Chat

- **Chat history persisted** — conversations are stored in SQLite at
  `~/.skill/chat_history.sqlite`; messages survive closing and reopening
  the chat window
- **Multi-conversation sidebar** — the chat window now has a sidebar listing
  named conversation threads; threads are persisted to disk and can be
  renamed or deleted
- **System prompt editor** — the system prompt is exposed as a text area in
  the chat settings panel so users can bias the model (e.g. "you are a
  neurofeedback coach") without recompiling
- **EEG context injection** — the current `eeg-bands` WebSocket event is
  automatically wired into the system prompt: "User's current focus: 72,
  relaxation: 58, SNR: 14 dB…" so the model can give contextualised advice
- **Prompt library** — a built-in set of neurofeedback prompt templates
  (e.g. "Summarise today's session", "Suggest a relaxation technique",
  "Explain what high theta means") is accessible from a `+` button in the
  chat input

### LLM — Downloads

- **Model download resumption** — interrupted downloads no longer restart
  from zero; the downloader uses `Content-Range` byte-range requests to
  resume from the last received byte

### UMAP Viewer

- **Export PNG / JSON** — "Export PNG" and "Export JSON" buttons added to
  the 3D scatter plot toolbar; PNG captures the current WebGL viewport,
  JSON exports the full point cloud with labels and timestamps

### Focus Timer

- **Session log** — a summary panel shows today's completed Pomodoro cycles,
  total focus time, and total break time; entries are labelled and persisted
  across restarts

### Onboarding (0.0.16)

- **Extended checklist** — onboarding now includes four additional steps:
  download an LLM model, run a similarity search, set a DND threshold, and
  try the REST API; previous four steps preserved

### UI / UX

- **Command Palette — fuzzy scoring** — the palette filter now uses an
  fzf-style scored fuzzy algorithm; partial matches are ranked by relevance
  instead of simple `includes()` containment
- **Theme — custom accent colour** — a primary-hue colour picker has been
  added to Settings → Appearance; the chosen accent is stored in
  `~/.skill/theme.json` and applied app-wide; dark / light / system mode
  selection is unaffected

---

## [0.0.15] — 2026-03-11

### Windows support

- **espeak-ng static build — Windows MSVC** — `scripts/build-espeak-static.ps1`
  builds `espeak-ng.lib` from source using CMake + MSVC on first run, then
  links it statically into the binary; subsequent builds are a no-op.
  Produces a single merged `.lib` (CMake + `lib.exe /OUT`) so that the linker
  sees no duplicate symbol conflicts
- **espeak-ng static build — Windows MinGW/GNU** — `scripts/build-espeak-static-mingw.sh`
  handles the `*-windows-gnu` target ABI (MSYS2 / cross-compile from
  Linux/macOS); output goes to `espeak-static-mingw/` to avoid collisions
  with the MSVC archive
- **`build.rs` — multi-platform espeak dispatch** — selects the correct
  build script and archive name based on `CARGO_CFG_TARGET_ENV`:
  `msvc` → PowerShell `.ps1`, `gnu` on Windows → MinGW `.sh`,
  macOS/Linux → Unix `.sh`; links `-lc++` on macOS, `-lstdc++` on Linux/MinGW,
  and omits the C++ flag on MSVC (runtime linked automatically)
- **`build.rs` — espeak data copy deferred to release** — the espeak-ng data
  directory is no longer copied during `cargo build` / `tauri dev`; the copy
  is skipped in debug builds to break the infinite rebuild loop where
  `build.rs` copies → Tauri watcher detects the change → `cargo run` → repeat.
  An empty placeholder directory is still created so Tauri's resource-path
  validation does not error at startup
- **`fast-hnsw` — vendored with Windows fix** — `memmap2::Mmap::advise()` and
  `memmap2::Advice` are `#[cfg(unix)]` and not available on Windows; the
  `fast_hnsw` crate's unconditional `advise(Advice::Random)` call caused a
  compile error on the MSVC target; patched locally via
  `src-tauri/vendor/fast-hnsw` with the `advise` call wrapped in
  `#[cfg(unix)]`; vendored until upstream releases a fix
- **`WINDOWS.md`** — updated prerequisites: Visual Studio Build Tools 2022
  (**Desktop development with C++** workload, provides `cl.exe`, `lib.exe`,
  Windows SDK) now listed as step 1; CMake doc note updated to cover
  espeak-ng's build system in addition to llama.cpp; added Git as a
  prerequisite for cloning the espeak-ng source; renumbered all steps

### Build / tooling

- **`scripts/tauri-build.js` refactored** — now a general Tauri wrapper that
  handles `dev`, `build`, and any other subcommand; non-`dev`/`build`
  subcommands (e.g. `tauri info`, `tauri signer`) pass straight through without
  triggering an espeak pre-build; platform detection now also covers
  `*-windows-gnu` (MinGW)
- **npm `tauri` script** — `"tauri": "node scripts/tauri-build.js"` routes all
  `npm run tauri …` invocations through the wrapper, so `npm run tauri info`,
  `npm run tauri dev`, `npm run tauri build -- --debug`, etc. all work
  consistently across platforms

### LLM — WebSocket / REST API

- **WebSocket commands** — `llm_status`, `llm_start`, `llm_stop`,
  `llm_catalog`, `llm_download`, `llm_cancel_download`, `llm_delete`,
  `llm_logs` added to the WebSocket command dispatcher (all behind the `llm`
  Cargo feature flag)
- **REST endpoints** — matching HTTP shortcuts added to the axum router in
  `api.rs`:
  - `GET  /llm/status` — running state, active model name, context size, vision flag
  - `POST /llm/start` — load the active model and start the inference server
  - `POST /llm/stop` — stop the inference server and free GPU/CPU resources
  - `GET  /llm/catalog` — model catalog with per-entry download states
  - `POST /llm/download` — start a background model download `{ "filename": "…" }`
  - `POST /llm/cancel_download` — cancel an in-progress download
  - `POST /llm/delete` — delete a locally-cached model file
  - `GET  /llm/logs` — last 500 LLM server log lines
  - `POST /llm/chat` — non-streaming chat completion; body: `{ message, images?, system?, temperature?, max_tokens? }`; returns `{ text, finish_reason, tokens }`
- **`LlmServerState::chat()`** — new method on the server-state actor handle;
  submits a generate request to the actor's channel and returns an
  `UnboundedReceiver<InferToken>` for streaming; returns `Err` immediately if
  the model is still loading or the actor has exited
- **`extract_images_from_messages()`** — helper that decodes all
  `data:<mime>;base64,…` data-URL image parts from an OpenAI-style messages
  array into raw `Vec<u8>` bytes; plain `https://…` URLs are silently skipped;
  call before passing messages to the actor so it receives pre-decoded bytes

### LLM — verbose logging

- **`LlmConfig.verbose`** (`bool`, default `false`) — when `false` (default),
  all internal llama.cpp / ggml and clip logs are silenced; set `true` to
  see raw tensor-load progress and other low-level detail
- **`mtmd_log_set` silence** — `clip_model_loader` uses a separate logger
  (`mtmd_log_set`) that is not affected by `llama_log_set`; the clip logger is
  now silenced via a no-op `extern "C"` callback when `verbose = false`,
  eliminating the tensor-load spam when loading a multimodal projector

### CLI (`cli.ts`)

- **`llm` subcommand group** added:
  - `llm status` — print LLM server status (stopped / loading / running)
  - `llm start` — load the active model and start the inference server
  - `llm stop` — stop the inference server, free GPU memory
  - `llm catalog` — list all catalog models with download states
  - `llm download <filename>` — start a background model download
  - `llm cancel <filename>` — cancel an in-progress download
  - `llm delete <filename>` — delete a cached model file
  - `llm logs` — print the last 500 LLM server log lines
  - `llm chat` — interactive multi-turn chat REPL (WebSocket streaming)
  - `llm chat "message"` — single-shot: send one message and stream the reply
  - `llm chat "message" --image a.jpg --image b.png` — vision: attach images
    (files are base64-encoded and embedded as `image_url` content parts;
    requires a vision-capable model with mmproj loaded)
- **`--image <path>`** — new flag (repeatable) for attaching image files to
  `llm chat` turns
- **`--system <prompt>`** — system prompt prepended as a `{ role: "system" }`
  message for `llm chat`
- **`--max-tokens <n>`** — maximum tokens to generate per turn
- **`--temperature <f>`** — sampling temperature (0 = deterministic, 1 = creative)

### Settings UI

- **Sidebar navigation** — the tab bar in Settings has been replaced with a
  persistent sidebar; each tab entry shows a 24 × 24 stroked SVG icon alongside
  the label and an active-indicator bar
- **Keyboard shortcuts** — `Cmd/Ctrl + 1–9` switch between the first nine
  settings tabs; tooltips on each sidebar item show the shortcut hint

### Help UI

- **Sidebar navigation + search** — the Help window now uses the same sidebar
  layout as Settings; a search box in the top bar filters across all help
  sections with keyboard-navigable results

### Internals

- **`SKILL_DIR` constant** — `src-tauri/src/constants.rs` now exports
  `pub const SKILL_DIR: &str = ".skill"` so the directory name is defined
  in one place; `default_skill_dir()` in `settings.rs` uses it
- **Data directory hardcoded** — the `data_dir` field has been removed from
  persisted settings; the skill directory is always `~/.skill` and is never
  configurable at runtime; `expand_tilde` helper and its tests removed

### Dependencies (0.0.15)

- `kittentts` `0.2.4` → `0.2.5`

---

## [0.0.13] — 2026-03-10

### Onboarding (0.0.13)

- **Recommended models quick setup** — onboarding now starts staged
  background downloads automatically while the user proceeds through steps,
  in this order: ZUNA → KittenTTS → NeuTTS → Qwen 3.5 4B (`Q4_K_M` target)
- **Persistent footer download status** — all onboarding views now show a
  subtle footer line with staged model setup progress (ZUNA, Kitten, NeuTTS,
  LLM), and the onboarding window size was increased to keep spacing readable
  with the always-visible footer indicator

### Dependencies (0.0.13)

- `llama-cpp-4` `0.2.3` → `0.2.5`
- `kittentts` `0.2.2` → `0.2.4`
- `neutts` `0.0.5` → `0.0.7`

### Bug fixes

- **Blank main window after long idle** — after a full day in the system
  tray with the window hidden, macOS can silently terminate WKWebView's
  web-content process under memory pressure, leaving a blank white page
  that only a full app restart could recover from
  - `+layout.svelte` sets `window.__skill_loaded = true` in `onMount` as
    a renderer-liveness sentinel
  - New `show_and_recover_main()` Rust helper checks the sentinel on every
    show via `eval()`; if the flag is absent it triggers `location.reload()`
    (renderer alive but content cleared), and falls back to `navigate()` if
    `eval()` itself returns `Err` (renderer process fully dead, WKWebView
    needs a fresh process spawned)
  - `RunEvent::Reopen` handler added — clicking the macOS Dock icon while
    all windows are hidden now shows the main window and runs the same
    two-layer recovery (previously a silent no-op)

- **Update loop — first check delayed by full interval** — the background
  updater slept `interval_secs` *before* the first check, so with the
  default 1-hour interval the first background check fired ~61 minutes after
  launch; pattern changed to check-then-sleep so the first check fires 30
  seconds after startup as intended

- **Update loop — update silently dropped on CDN race** — when the Rust
  background task emitted `update-available`, the frontend had to re-run
  `check()` to obtain a downloadable `Update` object; if `check()` returned
  `null` (latest.json not yet propagated to all CDN edge nodes), `available`
  was wiped and `phase` reverted to `"idle"` with no user feedback; fixed
  by threading the event payload as a `hint` through `checkAndDownload()` —
  the known version stays visible in the UI during the re-check, and a CDN
  race surfaces an actionable "Retry" error instead of a silent reset

- **What's New — dismiss race with uninitialised version** — `appVersion`
  started as the string `"…"` and was populated asynchronously via IPC;
  clicking "Got it" before the call resolved stored `"…"` in
  `last_seen_whats_new_version`, causing the window to reopen on every
  subsequent launch; fixed by seeding `appVersion` synchronously from the
  CHANGELOG version embedded at build time

- **What's New — markdown not rendered** — changelog entries containing
  `**bold**`, `` `code` `` spans, multi-line bullet continuations, and
  numbered sub-lists were all rendered as plain text; replaced the
  hand-rolled `parseChangelog` parser (which dropped any line not starting
  with `-` plus a trailing space) and the manual `{#each sections}` template with
  `MarkdownRenderer` (existing component backed by `marked` + GFM); scoped
  CSS overrides inside `.wn-body` preserve the compact window style without
  affecting the chat renderer

### Build / CI (0.0.13)

- **CI `cargo check --locked` failing on Linux** — `Cargo.lock` generated
  on macOS caused the Linux CI job to fail with "cannot update the lock file
  because --locked was passed"; added `cargo fetch --target
  x86_64-unknown-linux-gnu` before `cargo check --locked` to resolve
  platform-specific dependencies for Linux without touching the network
  during the check itself

- **Release — single notarization round trip** — the release workflow
  previously issued two separate `xcrun notarytool submit --wait` calls
  (one for the `.app` as a ZIP, one for the DMG), each waiting up to 20+
  minutes; consolidated to a single DMG submission — Apple's service
  registers notarization tickets for all signed content inside the container,
  so `xcrun stapler staple` succeeds on both the DMG and the `.app`
  afterward without a second submission; the updater tarball step is
  reordered to run after the DMG step so it always packages a stapled `.app`

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

### Build / CI (0.0.11)

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

### Dependencies (0.0.9)

- Migrated `llama-cpp-4` and `llama-cpp-sys-4` to local path via
  `[patch.crates-io]` (`../../../llama-cpp-rs/llama-cpp-4` and
  `../../../llama-cpp-rs/llama-cpp-sys-4`) — ensures the SIGILL fix
  (correct `CMAKE_OSX_ARCHITECTURES` / `CMAKE_CROSSCOMPILING` for Apple
  cross-arch builds) is always active; both the `llm` feature and neutts's
  backbone resolve to the same local crate, preserving the `links = "llama"`
  deduplication

### Build / CI (0.0.9)

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
