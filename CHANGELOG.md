# Changelog

All notable changes to NeuroSkill™ are documented here.
Pending changes live as fragments in [`changes/unreleased/`](changes/unreleased/).
Past releases are archived in [`changes/releases/`](changes/releases/).

---

## [Unreleased]

## [0.0.131-rc.31] — 2026-09-28

### Features

- fix the session drop issue
- improve cli

## [0.0.131-rc.30] — 2026-09-28

### Refactor

- **One `CBCentralManager` per process, by construction.** `btleplug` is now
  replaced wholesale by `patches/btleplug-0.11.9` — its public API
  reimplemented over `webbluetooth` — via a single `[patch.crates-io]` entry.
  The five device crates still written against btleplug (`awear`, `idun`,
  `mendi`, `mw75`, `openbci`) compile against it **unchanged**, and every BLE
  operation in the process now funnels through one
  `webbluetooth::Bluetooth::shared()` session.

  This replaces the `eugenehp/btleplug` fork, whose only purpose was a macOS
  scanning fix.

  Upstream btleplug's CoreBluetooth backend has no singleton of any kind — no
  `OnceLock`, `OnceCell`, `lazy_static` or `static` — so every
  `Manager::new().adapters()` allocated a fresh `CBCentralManager` on a detached
  thread running `loop { wait_for_message().await }` with no exit condition.
  Each call permanently leaked a thread, a tokio runtime, a dispatch queue, a
  delegate and a manager. Those calls were not one-time init either: in `mw75`
  they sit inside `scan_all()` and `connect()`, so it was one more manager per
  scan and per connect attempt, and on macOS a second manager cannot discover
  peripherals while another is scanning.

  Migrating the five crates one at a time could not have fixed this — it relies
  on each crate being correct and leaves a second manager live throughout. The
  shim makes a second session *unconstructible*: `Bluetooth::new()` and
  `with_chooser()` appear nowhere in it and have no btleplug concept that maps
  to them. `platform::tests::every_adapter_is_the_same_session` asserts it.

  `src/api/` and `src/common/` are byte-identical copies of upstream 0.11.8, so
  the consuming crates see the same types and a future re-sync stays a straight
  copy; only the backend (`src/platform.rs`) is new. Two places the mapping is
  not 1:1, both handled there: **notification fan-in** (btleplug hands out one
  merged stream per peripheral, valid before any connection and across
  reconnects, while webbluetooth subscribes per characteristic — so each
  `subscribe()` spawns a pump that tags values with their characteristic UUID
  into a per-peripheral broadcast channel) and **scan refcounting** (an
  `Adapter` is a shared handle, so dropping one must not tear down a scan
  another holder still wants).

- **Remove the Hermes V1 driver.** The `hermes-ble` dependency,
  `skill-devices/src/session/hermes.rs`, `connect_hermes`, the
  `ConnectRoute::Hermes` route and Hermes BLE-name recognition in the scanner
  are all gone.

  `hermes-ble` was the one crate pinned to `btleplug ^0.12.0`, which the
  workspace's 0.11 patch could not satisfy — so it resolved unpatched from the
  registry and put a second, entirely separate btleplug stack in the graph. One
  `[patch.crates-io]` entry supplies one version, so removing it is what let the
  webbluetooth shim cover every remaining btleplug user at once.

  A device paired on an older build still carries its name into routing, and
  `select_connect_route` ends in a `ConnectRoute::Muse` catch-all — so a Hermes
  headset would have been silently handed the Muse protocol. Connecting one now
  fails with an explicit message instead, guarded by
  `removed_hermes_does_not_fall_back_to_muse`.

  Note the Hermes *device kind* is still listed in the app's supported-devices
  catalogue (`DeviceKind::Hermes`, its i18n strings and device image), which
  carries no btleplug dependency. Delisting it from the UI is a separate change.

### Build

- **Fix the Windows release link failure** that broke rc.26 through rc.29. The
  BLE migration pulled in `webbluetooth-windows` 0.0.1, which declared
  `#[link(name = "combase")]`; the Windows SDK ships no `combase.lib`, so every
  Windows build died at the final link of `skill-daemon` with `LNK1181: cannot
  open input file 'combase.lib'`. The WinRT string and activation exports it
  wants (`WindowsCreateString`, `RoGetActivationFactory`, and four more) live in
  `combase.dll` at run time but are imported from `runtimeobject.lib`. Fixed
  upstream in `webbluetooth` 0.0.2 and picked up here.

  The earlier `SKILL_NO_LLD` / forced-`link.exe` workaround in
  `release-windows.yml` was treating a symptom — both linkers were correctly
  reporting a library that does not exist — and can be removed separately.

### Dependencies

- Bump `webbluetooth` 0.0.1 → 0.0.3 and `muse-rs` 0.2.0 → 0.2.1. **Both had to
  move together**: Cargo treats 0.0.x releases as mutually incompatible, so
  bumping only one resolved to two copies of `webbluetooth` — which kept the
  unfixed 0.0.1 Windows backend in the graph *and* meant two
  `Bluetooth::shared()` `OnceLock`s, so two `CBCentralManager`s and devices that
  would not connect. The invariant is now recorded in the workspace root
  `Cargo.toml`: `cargo tree -p skill-daemon -i webbluetooth` must print exactly
  one node.

## [0.0.131-rc.29] — 2026-09-22

### Features

- MSVC vs LLD

## [0.0.131-rc.28] — 2026-09-22

### Features

- updated release windows action

## [0.0.131-rc.27] — 2026-09-22

### Features

- fixed windows build

## [0.0.131-rc.26] — 2026-09-22

### Bugfixes

- **Paired devices survive the BLE backend change**: webbluetooth reports Apple device ids as uppercase `NSUUID` strings where btleplug reported lowercase UUIDs, which would have made every entry in `paired_devices.json` stop matching after upgrade — the headset would reappear as unpaired and auto-reconnect would never fire. BLE ids are now canonicalised to one spelling (`skill_daemon_common::ble_id`) at every write, compared case-insensitively at every lookup, and migrated in place on first start.
- **"Forget device" no longer silently fails**: the paired-list removal compared ids with exact string equality, so a case difference left the device paired.
- **Duplicate scanner entries**: a paired device whose stored id differed in case from the scanner's was pushed into the device list a second time, listing the same headset twice.
- **Daemon can request Bluetooth under launchd**: `skill-daemon.app`'s generated `Info.plist` was missing `NSBluetoothAlwaysUsageDescription`. Spawned by the Tauri app the outer bundle's key covered it, but started by launchd (`RunAtLoad`) the daemon is its own responsible process and was denied Bluetooth with no prompt — presenting as a scan that silently found nothing.

### Refactor

- **BLE scanner moved to webbluetooth**: the daemon's advertisement scanner now runs on the process-wide `Bluetooth::shared()` session instead of its own btleplug `CBCentralManager`, so it shares one radio session with muse-rs rather than competing with it. The session coexists with an in-flight connect by design, which removes the need to tear the scanner down and rebuild it around every BLE connect attempt.

### Docs

- **`docs/webbluetooth-migration.md`**: what moved to webbluetooth, what is still on btleplug, and what has to happen before the `[patch.crates-io] btleplug` override can be dropped.

### Dependencies

- **muse-rs 0.1 → 0.2**: switches the Muse BLE backend from btleplug to webbluetooth. The `MuseClient` / `MuseClientConfig` / `MuseDevice` / `MuseHandle` / `MuseEvent` API is unchanged, so the adapter and connect paths needed no edits.
- **webbluetooth 0.0.1** added as a direct dependency of `skill-daemon`, replacing its direct `btleplug` dependency. Its backends have no native dependencies — no `bluer`, no `dbus`, no `windows` crates — which drops a runtime `libdbus` requirement from the Linux packages.

## [0.0.131-rc.25] — 2026-09-18

### Features

- **`exg_auto_download_weights` setting** (Settings → EEG Model, off by default): when an EEG session starts without encoder weights on disk, fetch them automatically instead of recording metrics-only. Off by default because the default ZUNA encoder is a 380M-parameter model. Reuses the existing download machinery, including progress events and cancellation.

### Performance

- **Screenshot backfill: one paged query instead of three full table scans.** `rows_needing_backfill` replaces issuing `rows_without_ocr` + `rows_without_embedding` + `rows_without_ocr_embedding` and merging them into a map of every pending row; it carries the per-row flags plus `ocr_text` and `timestamp`, which also removes two per-row lookups. Rows are walked newest-first via an `id` cursor, so an interrupted pass has covered the most recently captured screenshots rather than an arbitrary hash-order subset.
- Backfill progress now reports to the UI on the existing `screenshot-reembed-progress` event (`done`/`total`/`elapsed_secs`/`eta_secs`, plus `embedded`/`skipped`/`complete`), and logs an embedded/skipped/elapsed tally on completion. Previously a multi-minute pass was silent and unreadable files were skipped without being counted.
- **An empty screenshot HNSW index could never be loaded again.** `fast-hnsw` serializes a zero-node labeled index but rejects it on read ("file contains no payload section"), so an empty index that reached disk failed to load on every subsequent boot, was rebuilt — still empty — and written again. Empty indexes are no longer written, and a stale empty file is removed so the next load takes the clean path.
- **EEG sessions recorded with no embeddings gave no actionable signal.** The weights resolver only probes the HuggingFace cache and never downloads, so a machine without encoder weights recorded every session metrics-only behind a single ERROR line. `EmbedWorkerStatus` now reports a `reason` (`weights_missing` vs `encoder_unavailable`) and the worker emits `ExgWeightsMissing`.
- **Auto-connect attempted a BLE device before it was discoverable.** The first connect fired 900 ms after daemon boot against the cached preferred device, burning a full scan timeout per attempt — typically 7 failed attempts and ~70 s of ERROR lines before a headband appeared, out of a 12-attempt budget. BLE targets now wait for the scanner to see the device (60 s cap, then attempt anyway); wired and manual targets are unaffected. Retryable failures log at WARN until the retry budget is exhausted.
- **ANT Neuro SDK was re-initialized every 10 seconds.** The scanner built a fresh `AntNeuroSdk` on every other tick, and its `NativeBackend` logs its version unconditionally — one INFO line every 10 s for the life of the daemon, on machines with no ANT Neuro hardware. The handle is now built once per process.

### Bugfixes

- **Screenshot capture was silently disabled on default settings.** The daemon's `ScreenshotContext::is_session_active()` returned a hardcoded `false` — a stub left by the thin-client migration (`ba3f07d4`), where the Tauri implementation had checked `session_start_utc.is_some()`. Since `session_only` defaults to `true`, the capture loop's gate was permanently shut: the worker spawned, logged "worker spawned", then spun on a 1-second no-op forever, never capturing, embedding, or backfilling. It now reads the live `session_handle`, matching how the rest of the daemon defines an active session.
- **Screenshot backfill was decided once at boot and effectively never ran.** The gate was read a single time when the embed worker spawned — before any session exists — so with the default `session_only = true` the historical catch-up was skipped on every boot and never re-evaluated. The backfill now runs from the embed loop's idle time, re-reading the gate every pass.
- **The screenshot backfill starved live capture.** It walked every owed row in one uninterruptible pass on the embed thread; since the job channel is `bounded(4)` with `try_send`, the capture thread *dropped* new screenshots for the whole window — manufacturing the debt the backfill was paying down, recoverable only on a later daemon start. It now runs in bounded 25-row chunks during idle ticks, so live jobs always take priority, a mid-pass disable stops it, and completed work survives a restart (indexes are saved per chunk).

### Server

- **Canonical `unix_ms` column on the `embeddings` table.** `timestamp` carries three historical encodings (Unix ms, `YYYYMMDDHHmmss`, and `YYYYMMDDHHmmss × 1000`), so every range query had to go through `DualTimestampRange` and match all three — with an explicit warning never to write a raw `WHERE timestamp >= ?`. Day stores now gain a `unix_ms` column, backfilled from whichever format each row uses, plus an index.

  The migration is deliberately **additive: `timestamp` is never rewritten**, because older builds of the app read it directly and expect the format they wrote — normalising it in place would corrupt their view of existing recordings. This is the expand half of expand/contract; `timestamp` can only be retired once no old build remains in the wild. Applied on open by both writers (the day store and the session pipeline's `EpochStore`, which share the table), idempotent, and resumable if interrupted part-way.

  Verified against a real 296-row recording containing both encodings: all rows backfilled, every `timestamp` byte-for-byte unchanged, every `unix_ms` a plausible instant, and a re-run a no-op. An `#[ignore]`d fixture test (`SKILL_MIGRATION_FIXTURE`) re-runs that check against any real day store before a release.

### Dependencies

- Refresh `rlx` / `rlx-models` git pins from the stale 0.2.14 lock to current `main` (0.2.16).
- **Clear all outstanding `cargo deny` advisories.** `rustls` 0.23.42 → 0.23.45 (RUSTSEC-2026-0285, TLS 1.3 handshake messages accepted across encryption-level boundaries), `h2` 0.4.15 → 0.4.19 and `chacha20` 0.10.1 → 0.10.2 (yanked).
- **Fork `oura-api` 0.1.2 to `reqwest` 0.12** (`patches/oura-api-0.1.2`, vendored; the only change from crates.io is the dependency version). Upstream's newest release pins `reqwest` 0.11, which pulled the entire hyper 0.14 stack including `h2` 0.3.x — the branch RUSTSEC-2026-0258 is unpatched on, fixed in `h2` >= 0.4.16 only. This removes the last advisory and drops a duplicate HTTP stack: the tree now resolves a single `hyper` (1.11) and a single `h2` (0.4.19), where it previously carried both 0.14/1.x and 0.3/0.4.
- Remove dead `deny.toml` entries that made `cargo deny` fail or warn: the `RUSTSEC-2024-0415` gtk ignore (unreachable — `unmaintained = "workspace"` already scopes that lint to our own crates, and an ignore that never matches is a hard error), the `winreg@0.55.0` duplicate skip (deduplicated by dropping `reqwest` 0.11), and re-pin the `winnow` skip 1.0.3 → 1.0.4.

## [0.0.131-rc.24] — 2026-08-12

### Features

- added i18n

## [0.0.131-rc.23] — 2026-08-09

### Features

- updated inference

## [0.0.131-rc.22] — 2026-08-04

### Features

- lock

## [0.0.131-rc.21] — 2026-08-04

### Features

- new distributed

## [0.0.131-rc.20] — 2026-07-31

### Features

- updated deps
- vendored gpu allocator
- updated cache

## [0.0.131-rc.19] — 2026-07-28

### Features

- Minor updates and improvements

## [0.0.131-rc.18] — 2026-07-28

### Bugfixes

- **Chat cancel handler shadowing**: Fix recursive symbol shadowing so chat tool cancel calls the daemon client correctly.
- **Tokens copy action typing**: Update Tokens tab Copy behavior to use typed token access and `common.copy` label wiring.

### Build

- **Workspace cleanup**: Remove duplicate `Cargo.toml` workspace members (`skill-daemon`, `skill-daemon-common`).

- **Cache freshness automation**: Add a weekly GitHub Action to refresh HF download counts and open a PR; add CI validation that the committed cache covers the current catalog.

- **Desktop CSP hardening**: Replace `csp: null` with a localhost/daemon-aware Content-Security-Policy in `tauri.conf.json`.

- **VERSION as release source of truth**: Add repo-root `VERSION` and make bump/tag/release/CI/packaging flows read it, then sync `package.json`, `tauri.conf.json`, and `Cargo.toml` from that value.
- **Shared compile-product pipeline**: Introduce `scripts/compile-product.mjs` to build daemon (OS umbrella) -> tty -> app (`custom-protocol`) and use it across release workflows, dry-run, daemon-sidecar prep, and tauri-build.
- **compile-product hardening**: Tighten target-specific tty gating, strict OS-feature mapping, `CARGO_TARGET_DIR` plus `cargo metadata` staging, daemon-only `--timings`, fail-closed local daemon builds, and Linux `skill-tty` packaging.
- **Product build follow-ups**: Include `product` in umbrellas, report `VERSION` from daemon, ship CUDA+wgpu with runtime fallback (CUDA -> wgpu -> CPU) on Linux/Windows, enforce macOS tty hard-fail, and improve Linux daemon path discovery.

### CLI

- **Typed daemon client checks**: Migrate tokens, chat rename/cancel-tool, and device prefer/forget/retry/status call sites off `daemonInvoke`; fix `tokens.ts` ACL wire types to snake_case; add `npm run check:typed-daemon-clients`.

### UI

- **Primary app shell navigation**: Add a Live / Find / Ask / History destinations bar across main feature windows while keeping Add Label in the main titlebar.
- **Settings information architecture**: Group tabs by Signal, Intelligence, Capture & Privacy, Automation, and App, with a collapsible Advanced section and Devices as the default landing tab.
- **Live dashboard view modes**: Add Waveform / Physiology / State switching with progressive disclosure based on connected modality support.
- **Onboarding flow simplification**: Reduce core first-run path to connect -> fit -> tray -> done, require research-use acknowledgement, and move calibration/models/permissions to optional setup from Done.
- **Getting Started deep links**: Make checklist items open Devices, Calibration, Goals, Downloads, Search, and API targets directly.
- **Find UX updates**: Add example chips, prioritize Interactive query + Search action, and move pipeline/filters under Advanced with titlebar source labeling.
- **Command palette IA**: Add a Primary section for Live / Find / Ask / History and separate Add Label from Browse Labels actions.
- **Labels discoverability alignment**: Standardize Browse Labels naming for window title and `labels.openLabels`; change History toggle wording to "Show labels"; localize titlebar Help/Reload labels.
- **Shell discoverability improvements**: Show accelerator hints for Find / Ask / History and include shortcuts in tooltips.
- **Shell interaction fix**: Render primary nav inside `#main-content` so it is not blocked by the draggable titlebar region.
- **Chart accessibility**: Add hatch patterns plus active scheme colors on band-power tiles so differentiation does not rely on color alone.
- **Window-title consistency**: Open labels window as "Browse Labels" from Rust side and align `labels.title` text.

### LLM

- **HF downloads cache for model ranking**: Add `scripts/update-hf-downloads-cache.mjs` to crawl Hub download counts for every catalog repo plus top mlx-community models, writing both `src-tauri/hf_downloads_cache.json` and `src/lib/generated/hf-downloads-cache.json`.
- **Family sort by popularity**: Sort the LLM family picker by Hub downloads descending by default and show `↓ N` download hints in the dropdown.
- **MLX search UX**: Add an MLX tab in HF search (also sorted by downloads) and support importing an entire mlx-community snapshot via "Add pack".

- **Mistral VL runner support**: Promote `mistral3`/`mistral4` out of unimplemented paths in `rlx-models`, wire `MistralRunner` as `LmRunner`, add `rlx-mistral-vl` for Pixtral ViT/projector on Metal/CUDA/CPU, and route mistral+mmproj via `auto_runner_with_mmproj`.
- **NeuroSkill loader integration**: Add `try_mistral_vl_runner` in `load_with_mmproj` and document Ministral as supported.

- **MLX community model path**: Wire huggingface.co/mlx-community packs through `rlx-models` (`MlxLoader` / `Qwen3Runner::from_mlx_packed`).
- **Catalog entries for MLX Qwen3**: Add Qwen3 0.6B / 1.7B / 4B 4-bit entries and snapshot download handling for `tags: ["mlx"]`.
- **Discovery + loader behavior**: Surface safetensors snapshot directories (not GGUF-only) in local discovery and prefer the MLX device when loading compatible packs.
- **Feature-gated rollout**: Keep discovery and `from_mlx_packed` behind `llm-model-discovery` until the `rlx-models` pin is bumped.

- **Model download integrity checks**: SHA-256 verify HF LFS blobs before promoting `.incomplete` to final blob, re-verify existing blobs, and delete mismatched files.

- **Feature-flag cleanup**: Remove stale `llm-vulkan` / `llm-metal` references from scripts, docs, CONTRIBUTING, skill-llm README, and i18n copy in favor of OS umbrellas (`apple`, `linux`, `windows`).

- **VL mmproj runner wiring**: In `rlx-models`, add multimodal support via `GemmaRunner.mmproj` and `LmRunner` multimodal handling, add combined `Qwen3VlRunner` and `LfmVlRunner`, and make `auto_runner_with_mmproj` attach mmproj for supported families while erroring clearly for unsupported families instead of silently ignoring mmproj.
- **Qwen3 VL promotion**: Promote `qwen3vl*` out of unimplemented paths.
- **Loader routing in app**: Update NeuroSkill `load_with_mmproj` to prefer Qwen3-VL / LFM-VL / Gemma+mmproj before plain Qwen3 and return explicit errors when mmproj is set on non-multimodal runners.

- **Windows KittenTTS enablement**: Enable `rlx-kittentts` on Windows (CUDA + GPU features) and activate `tts_kitten_active` across desktop OSes so the default voice engine is no longer a no-op.

### Server

- **Daemon routes extraction**: Move iroh HTTP routes into `skill-daemon-routes` as the first non-stub extracted module, and document remaining extraction blockers in the crate and `docs/architecture.md`.

- **Tool safety defaults**: Default `require_bash_edit` to `true`, expand bash denylist coverage (`curl|bash`, interpreters), and require approval for secret home paths (`.ssh`, `.aws`, `.env`, `auth.token`) and Windows system directories.
- **`web_fetch` SSRF guardrails**: Refuse loopback, private, link-local, and cloud-metadata URLs.
- **LAN bind safety**: Refuse non-loopback `SKILL_DAEMON_ADDR` unless `SKILL_DAEMON_ALLOW_LAN=1`, warn on `0.0.0.0` WebSocket host usage, and document CORS plus loopback defaults.

- **Tool-safety fail-closed behavior**: Register native approval hooks in `skill-daemon` and deny bash-edit execution when no hook is present instead of executing unmodified scripts.

### i18n

- **Locale completion for new IA strings**: Translate shell, settings-group, live-view, onboarding, Find, and command-palette additions across de, es, fr, he, ja, ko, uk, zh and replace auto-synced English fallbacks.

### Docs

- **LLM and hooks ownership docs**: Rewrite `docs/LLM.md` and `docs/HOOKS.md` for RLX-in-daemon ownership, correct the `docs/AI.md` llama.cpp claim, and align Node prerequisite docs to >=20 (`docs/DEVELOPMENT.md`, `package.json` engines).

- **Release/CI documentation alignment**: Document release-setup composite options (including optional `build-frontend`), standardize rust-cache guidance as the only compile cache, clarify PR vs release CI behavior, and refresh skill-daemon feature docs.

### Dependencies

- **Windows Vulkan SDK removal**: Drop Vulkan SDK requirements from Windows PR clippy/release paths where no longer needed.

## [0.0.131-rc.17] — 2026-07-24

### Features

- Minor updates and improvements

## [0.0.131-rc.16] — 2026-07-24

### Features

- updates skill daemono# Please enter the commit message for your changes. Lines starting
- Gate LLM e2e tests on llm-rlx so default cargo test skips them.
- Feature unification from skill-daemon was enabling the llm marker without a backend, which made pre-push fail.
- Allow GPL for new rlx TTS crates and ignore protobuf advisory.
- rlx-tiny-tts pulled onnx/protobuf back into the graph; extend cargo-deny exceptions so pre-push stays green.
- Fix release CI: patch remaining crates.io rlx models and drop Windows wgpu.
- rc.15 failed on Mac/Linux from registry rlx-qwen3 vs git rlx-flow, and on Windows from wgpu-hal/windows-rs 0.62 vs gpu-allocator on 0.61. Patch the leftover model crates and keep the Windows umbrella on CUDA until gpu-allocator aligns.
- Simplify release CI setup and pin rlx git patches.
- Fold Linux Vulkan/apt into a shared release-setup action, drop sccache from CI, scope daemon features per OS, and keep Cargo.lock on GitHub rlx main.

## [0.0.131-rc.15] — 2026-07-24

### Features

- rlx core models

## [0.0.131-rc.14] — 2026-07-24

### Features

- updated CI

## [0.0.131-rc.13] — 2026-07-24

### Features

- fixed CI
- simplified CI

## [0.0.131-rc.12] — 2026-07-24

### Features

- update deps

## Earlier releases

The 140 releases before this point are kept in full under [`changes/releases/`](changes/releases/), one file each.

- [0.0.131-rc.11](changes/releases/0.0.131-rc.11.md)
- [0.0.131-rc.10](changes/releases/0.0.131-rc.10.md)
- [0.0.131-rc.9](changes/releases/0.0.131-rc.9.md)
- [0.0.131-rc.8](changes/releases/0.0.131-rc.8.md)
- [0.0.131-rc.7](changes/releases/0.0.131-rc.7.md)
- [0.0.131-rc.6](changes/releases/0.0.131-rc.6.md)
- [0.0.131-rc.5](changes/releases/0.0.131-rc.5.md)
- [0.0.131-rc.4](changes/releases/0.0.131-rc.4.md)
- [0.0.131-rc.3](changes/releases/0.0.131-rc.3.md)
- [0.0.131-rc.2](changes/releases/0.0.131-rc.2.md)
- [0.0.130-rc.31](changes/releases/0.0.130-rc.31.md)
- [0.0.130-rc.30](changes/releases/0.0.130-rc.30.md)
- [0.0.130-rc.29](changes/releases/0.0.130-rc.29.md)
- [0.0.130-rc.28](changes/releases/0.0.130-rc.28.md)
- [0.0.130-rc.27](changes/releases/0.0.130-rc.27.md)
- [0.0.130-rc.26](changes/releases/0.0.130-rc.26.md)
- [0.0.130-rc.25](changes/releases/0.0.130-rc.25.md)
- [0.0.130-rc.24](changes/releases/0.0.130-rc.24.md)
- [0.0.130-rc.23](changes/releases/0.0.130-rc.23.md)
- [0.0.130-rc.22](changes/releases/0.0.130-rc.22.md)
- [0.0.130-rc.21](changes/releases/0.0.130-rc.21.md)
- [0.0.130-rc.20](changes/releases/0.0.130-rc.20.md)
- [0.0.130-rc.19](changes/releases/0.0.130-rc.19.md)
- [0.0.130-rc.18](changes/releases/0.0.130-rc.18.md)
- [0.0.130-rc.17](changes/releases/0.0.130-rc.17.md)
- [0.0.130-rc.16](changes/releases/0.0.130-rc.16.md)
- [0.0.130-rc.15](changes/releases/0.0.130-rc.15.md)
- [0.0.130-rc.14](changes/releases/0.0.130-rc.14.md)
- [0.0.130-rc.13](changes/releases/0.0.130-rc.13.md)
- [0.0.130-rc.12](changes/releases/0.0.130-rc.12.md)
- [0.0.130-rc.11](changes/releases/0.0.130-rc.11.md)
- [0.0.130-rc.10](changes/releases/0.0.130-rc.10.md)
- [0.0.130-rc.9](changes/releases/0.0.130-rc.9.md)
- [0.0.130-rc.8](changes/releases/0.0.130-rc.8.md)
- [0.0.130-rc.7](changes/releases/0.0.130-rc.7.md)
- [0.0.130-rc.6](changes/releases/0.0.130-rc.6.md)
- [0.0.130-rc.5](changes/releases/0.0.130-rc.5.md)
- [0.0.130-rc.4](changes/releases/0.0.130-rc.4.md)
- [0.0.130-rc.3](changes/releases/0.0.130-rc.3.md)
- [0.0.130-rc.2](changes/releases/0.0.130-rc.2.md)
- [0.0.130-rc.1](changes/releases/0.0.130-rc.1.md)
- [0.0.129](changes/releases/0.0.129.md)
- [0.0.128](changes/releases/0.0.128.md)
- [0.0.127](changes/releases/0.0.127.md)
- [0.0.126](changes/releases/0.0.126.md)
- [0.0.125](changes/releases/0.0.125.md)
- [0.0.124](changes/releases/0.0.124.md)
- [0.0.123](changes/releases/0.0.123.md)
- [0.0.122](changes/releases/0.0.122.md)
- [0.0.121](changes/releases/0.0.121.md)
- [0.0.120](changes/releases/0.0.120.md)
- [0.0.119](changes/releases/0.0.119.md)
- [0.0.118](changes/releases/0.0.118.md)
- [0.0.117](changes/releases/0.0.117.md)
- [0.0.116](changes/releases/0.0.116.md)
- [0.0.115](changes/releases/0.0.115.md)
- [0.0.114](changes/releases/0.0.114.md)
- [0.0.113](changes/releases/0.0.113.md)
- [0.0.112](changes/releases/0.0.112.md)
- [0.0.111](changes/releases/0.0.111.md)
- [0.0.110](changes/releases/0.0.110.md)
- [0.0.109](changes/releases/0.0.109.md)
- [0.0.106](changes/releases/0.0.106.md)
- [0.0.104](changes/releases/0.0.104.md)
- [0.0.103](changes/releases/0.0.103.md)
- [0.0.102](changes/releases/0.0.102.md)
- [0.0.101](changes/releases/0.0.101.md)
- [0.0.100](changes/releases/0.0.100.md)
- [0.0.99](changes/releases/0.0.99.md)
- [0.0.98](changes/releases/0.0.98.md)
- [0.0.97](changes/releases/0.0.97.md)
- [0.0.96](changes/releases/0.0.96.md)
- [0.0.95](changes/releases/0.0.95.md)
- [0.0.94](changes/releases/0.0.94.md)
- [0.0.93](changes/releases/0.0.93.md)
- [0.0.88](changes/releases/0.0.88.md)
- [0.0.87](changes/releases/0.0.87.md)
- [0.0.86](changes/releases/0.0.86.md)
- [0.0.85](changes/releases/0.0.85.md)
- [0.0.84](changes/releases/0.0.84.md)
- [0.0.83](changes/releases/0.0.83.md)
- [0.0.82](changes/releases/0.0.82.md)
- [0.0.81](changes/releases/0.0.81.md)
- [0.0.80](changes/releases/0.0.80.md)
- [0.0.79](changes/releases/0.0.79.md)
- [0.0.78](changes/releases/0.0.78.md)
- [0.0.77](changes/releases/0.0.77.md)
- [0.0.76](changes/releases/0.0.76.md)
- [0.0.75](changes/releases/0.0.75.md)
- [0.0.72](changes/releases/0.0.72.md)
- [0.0.71](changes/releases/0.0.71.md)
- [0.0.70](changes/releases/0.0.70.md)
- [0.0.69](changes/releases/0.0.69.md)
- [0.0.68](changes/releases/0.0.68.md)
- [0.0.67](changes/releases/0.0.67.md)
- [0.0.63](changes/releases/0.0.63.md)
- [0.0.62](changes/releases/0.0.62.md)
- [0.0.61](changes/releases/0.0.61.md)
- [0.0.60](changes/releases/0.0.60.md)
- [0.0.59](changes/releases/0.0.59.md)
- [0.0.58](changes/releases/0.0.58.md)
- [0.0.57](changes/releases/0.0.57.md)
- [0.0.56](changes/releases/0.0.56.md)
- [0.0.55](changes/releases/0.0.55.md)
- [0.0.54](changes/releases/0.0.54.md)
- [0.0.53](changes/releases/0.0.53.md)
- [0.0.52](changes/releases/0.0.52.md)
- [0.0.51](changes/releases/0.0.51.md)
- [0.0.50](changes/releases/0.0.50.md)
- [0.0.49](changes/releases/0.0.49.md)
- [0.0.47](changes/releases/0.0.47.md)
- [0.0.46](changes/releases/0.0.46.md)
- [0.0.45](changes/releases/0.0.45.md)
- [0.0.44](changes/releases/0.0.44.md)
- [0.0.43](changes/releases/0.0.43.md)
- [0.0.42](changes/releases/0.0.42.md)
- [0.0.41](changes/releases/0.0.41.md)
- [0.0.40](changes/releases/0.0.40.md)
- [0.0.39](changes/releases/0.0.39.md)
- [0.0.38](changes/releases/0.0.38.md)
- [0.0.37](changes/releases/0.0.37.md)
- [0.0.36](changes/releases/0.0.36.md)
- [0.0.35](changes/releases/0.0.35.md)
- [0.0.34](changes/releases/0.0.34.md)
- [0.0.33](changes/releases/0.0.33.md)
- [0.0.32](changes/releases/0.0.32.md)
- [0.0.31](changes/releases/0.0.31.md)
- [0.0.30](changes/releases/0.0.30.md)
- [0.0.29](changes/releases/0.0.29.md)
- [0.0.27](changes/releases/0.0.27.md)
- [0.0.24](changes/releases/0.0.24.md)
- [0.0.23](changes/releases/0.0.23.md)
- [0.0.17](changes/releases/0.0.17.md)
- [0.0.16](changes/releases/0.0.16.md)
- [0.0.15](changes/releases/0.0.15.md)
- [0.0.13](changes/releases/0.0.13.md)
- [0.0.11](changes/releases/0.0.11.md)
- [0.0.9](changes/releases/0.0.9.md)
- [0.0.6](changes/releases/0.0.6.md)
- [0.0.3](changes/releases/0.0.3.md)
