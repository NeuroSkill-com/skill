# Changelog

All notable changes to NeuroSkill™ are documented here.

---

## [0.0.27]

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
