# Contributing to NeuroSkill

## Prerequisites

- **Rust** (latest stable) via [rustup](https://rustup.rs/)
- **Node.js** ≥ 22.17 with npm (SvelteKit 3 and Vitest 5 need it; CI uses 22). `npm run setup` offers to install Node 22 into `~/.local` if it is missing or older
- **Tauri CLI**: installed via `npm ci` (workspace devDependency; `npm ci` follows the lockfile, as CI does)
- **Platform SDKs**: Xcode (macOS), Visual Studio Build Tools + LLVM (Windows), build-essential (Linux)
- **CMake** and **protoc** (`brew install cmake protobuf` on macOS); `npm run setup` checks for both
- **Vulkan SDK** (optional Linux tooling; LLM GPU backends use OS umbrellas `apple` / `linux` / `windows`)

## Quick Start

```bash
# Clone
git clone https://github.com/NeuroSkill-com/skill.git
cd skill

# Install all platform dependencies + JS deps (interactive)
npm run setup

# Ensure Rust is up to date (CI uses latest stable; clippy needs >= 1.95)
rustup update stable

# Default EEG model (Hugging Face CLI: https://huggingface.co/docs/huggingface_hub/guides/cli)
hf download Zyphra/ZUNA

# Run in development mode (starts Vite dev server + Tauri)
npm run tauri dev

# Or build a release
npm run tauri:build
```

`npm run setup` auto-detects your platform and installs everything needed
(protobuf, OpenMP, GNU ar, sccache, etc.).  Pass `--yes` to skip prompts.
`npm run setup:build-cache` adds optional build acceleration (see
[Build acceleration](#build-acceleration)). A local [RLX](https://github.com/MIT-RLX/rlx)
checkout is **not** needed; see [RLX](#rlx) if you work on rlx itself.

## Project Structure

```
├── crates/                  # Rust workspace crates (Tauri-independent)
│   ├── skill-daemon/        # Standalone HTTP daemon (sidecar)
│   ├── skill-daemon-routes/ # Extracted daemon HTTP routes (iroh first; more pending)
│   ├── skill-daemon-state/  # Daemon shared state + DB
│   ├── skill-daemon-common/ # Shared daemon types
│   ├── skill-eeg/           # EEG signal processing
│   ├── skill-exg/           # ExG (multi-modal biosignal) inference
│   ├── skill-gpu/           # GPU FFT / UMAP backends (MLX, CUDA, Vulkan)
│   ├── skill-llm/           # Local LLM inference (RLX runtime)
│   ├── skill-tools/         # LLM function-calling
│   ├── skill-vision/        # Screen content + camera vision
│   ├── skill-screenshots/   # Screenshot capture + encoding
│   ├── skill-history/       # Activity / conversation history
│   ├── skill-skills/        # Pluggable "skill" runtime
│   ├── skill-commands/      # Shell command tracking + categorization
│   ├── skill-tts/           # Text-to-speech (NeuTTS)
│   ├── skill-tray/          # System tray
│   ├── skill-headless/      # Headless daemon entrypoint
│   ├── skill-iroh/          # P2P sync (iroh)
│   ├── skill-lsl/           # Lab Streaming Layer
│   ├── skill-oura/          # Oura ring integration
│   ├── skill-calendar/      # Calendar/meeting sync
│   ├── skill-location/      # Location context
│   ├── skill-autostart/     # OS-level autostart
│   ├── skill-data/          # Data layer (DuckDB stores, embeddings)
│   ├── skill-db/            # DuckDB instance registry + one-time SQLite import
│   ├── skill-bids/          # BIDS-EEG export over a time range
│   ├── skill-router/        # Request router
│   ├── skill-jobs/          # Background job runner
│   ├── skill-health/        # Health checks
│   ├── skill-settings/      # Settings store + keychain
│   ├── skill-label-index/   # HNSW semantic label index
│   ├── skill-devices/       # EEG/biosignal device drivers
│   └── skill-constants/     # Shared constants
├── src/                     # SvelteKit frontend
├── src-tauri/               # Tauri app shell
├── neuroskill/              # `neuroskill` CLI (TypeScript)
├── neuroloop/               # `neuroloop` CLI (TypeScript)
├── extensions/              # External integrations
│   ├── vscode/              # VS Code / VSCodium / Cursor extension (submodule)
│   ├── browser/             # Browser extension
│   └── widgets/             # macOS WidgetKit extension
├── skills/                  # Pluggable skill definitions (markdown + manifests)
├── scripts/                 # Build, CI & test scripts (Node.js + bash, no Python)
│   ├── check.mjs            # The one list of checks (npm test, git hooks, CI)
│   ├── release.js           # npm run release rc|stable|status
│   ├── ci.mjs               # Cross-platform CI helpers
│   └── shell-hooks/         # zsh/bash/fish/PowerShell preexec hooks
├── docs/                    # Architecture + coverage docs
├── Casks/                   # Generated Homebrew cask
└── changes/                 # Changelog fragments
```

See `AGENTS.md` for the authoritative crate inventory and architecture rules.

## Development Workflow

### Checks and tests

Every check lives in one list, `scripts/check.mjs`. `npm test`, both git
hooks, `npm run bump -- --check` and CI's frontend job all run steps from it,
so a check added there is enforced everywhere.

```bash
npm test                        # fast tier: fmt, biome, tiny-text, types, vitest, ci, clippy, tier-1 Rust tests
npm test -- push                # what pre-push runs: everything, scoped to what your push changes
npm test -- full                # every check, unscoped (deny, shellcheck, ruff, a11y, catalogs, all Rust tests, …)
npm test -- vitest clippy       # individual steps, in the order given
npm test -- full --continue     # don't stop at the first failure
npm test -- --list              # every tier and step

npm test -- llm-e2e             # opt-in steps need models, builds or hardware:
npm test -- mlx-e2e             #   llm-e2e, mlx-e2e, smoke, daemon-packaging,
npm test -- widgets             #   daemon-e2e, widgets

npm run test:watch              # vitest in watch mode
npm run test:ui -- src/tests/chat-ui-e2e.spec.ts [--headed]   # Playwright UI specs
```

Steps whose tool is not installed (shellcheck, ruff, pwsh, cargo-deny) are
skipped with a note; CI runs them.

> **Tip:** Install [kache](https://github.com/kunobi-ninja/kache)
> (`brew install kunobi-ninja/kunobi/kache && kache init`) for faster clean rebuilds and
> cache reuse across worktrees. `npm test` and the build scripts prefer kache, then sccache.

See [docs/TEST-COVERAGE.md](docs/TEST-COVERAGE.md) for a detailed coverage analysis and gaps.

### Git hooks

`npm ci` points git at `.githooks/`:

- **pre-commit** — `npm test -- commit`: rlx pin guard, `llm_catalog.json`
  parses, i18n locales (when staged), `cargo fmt` (formats and re-stages staged `.rs`). Seconds.
- **pre-push** — `npm test -- push`: changelog fragments, plus fmt / biome /
  shellcheck / vitest / ci self-test / deny / clippy / cargo test for what the
  push changes. `PREPUSH_FULL=1 git push` runs the unscoped `full` tier;
  `git push --no-verify` skips it.

### Fixing and formatting

```bash
npm run lint:fix              # biome lint + fixes
npm run format                # biome format
cargo fmt
npm run sync:i18n:fix         # add missing translation keys
npm run audit:i18n            # unused / undefined keys
npm run sync:llm:catalog      # refresh GGUF file lists from the Hub
npm run sync:hf:downloads     # crawl Hub download counts (also refreshed weekly by CI)
npm run sync:readme:supported
npm run check:changelog:fix
```

The matching `--check` modes are all steps of `npm test -- full`.

### Faster local `cargo check`

```bash
cargo check $(bash scripts/changed-crates.sh origin/main)   # crates your branch affects
cargo check -p skill --lib                                   # just the app's lib target
```

### Build acceleration

```bash
npm run setup:build-cache
```

This offers [kache](https://github.com/kunobi-ninja/kache), the preferred compiler
cache, and falls back to sccache; on Linux it also offers mold. Toggles:
`SKILL_NO_KACHE=1` (use sccache), `SKILL_NO_SCCACHE=1`, `SKILL_NO_MOLD=1`,
`unset LLAMA_PREBUILT_DIR` (force a local llama.cpp build),
`SKILL_DAEMON_SERVICE_AUTOINSTALL=0` (no daemon service auto-install while testing).

### RLX

`rlx*` and `rlx-models*` come from GitHub (`MIT-RLX/rlx`, `MIT-RLX/rlx-models`,
`branch = "main"` in the root `Cargo.toml`), at the exact revisions pinned in the
committed `Cargo.lock`. Local builds, CI and release builds all use those pins.

```bash
npm run rlx                   # which source is active
npm run rlx -- local          # build against ../rlx + ../rlx-models (sibling checkouts)
npm run rlx -- deployed       # back to the Cargo.lock pins
npm run rlx -- bump           # move the pins to the tip of rlx / rlx-models main
```

`local` path-patches every rlx crate through the gitignored
`.cargo/rlx-local.toml` and hides the rewritten `Cargo.lock` from git
(skip-worktree), so the GitHub pins stay committed; the pre-commit hook refuses
a lock that points rlx at a path. With [direnv](https://direnv.net/), `.envrc`
switches to `local` automatically when the siblings exist, unless you chose
`deployed`. Other checkout locations: `rlx.path` / `rlx-models.path` (one line,
absolute path; see `rlx.path.example`) or `RLX_ROOT` / `RLX_MODELS_ROOT`.

**Picking up new rlx in skill** is a deliberate commit: `npm run rlx -- bump`,
build and test, commit `Cargo.lock`, then `npm run rlx -- local` to resume.
Until then CI and releases keep building the previous pins.

### Changelog fragments

Every feature or bugfix **must** include a changelog fragment:

1. `npm run changes:new -- <slug> --category <Category>`, or create a `.md` file
   in `changes/unreleased/` (e.g., `feat-my-change.md`)
2. Use the format:
   ```markdown
   ### Features

   - **Short title**: description of the change.
   ```
3. Valid categories: `Features`, `Performance`, `Bugfixes`, `Refactor`, `Build`, `CLI`, `UI`, `LLM`, `Server`, `i18n`, `Docs`, `Dependencies`

A release compiles every fragment in `changes/unreleased/` into
`changes/releases/<version>.md` and `CHANGELOG.md`.

## Releases

```bash
npm run release rc            # next RC:  0.1.3 → 0.1.4-rc.1,  0.1.4-rc.1 → 0.1.4-rc.2
npm run release stable        # stable:   0.1.4-rc.2 → 0.1.4   (no open RC: 0.1.3 → 0.1.4)
npm run release status        # VERSION, its tag, the release build and the GitHub release
```

Run it from `main` (or from a `release/*` branch to patch an older line). A cut:

1. bumps `VERSION` and the Cargo workspace version, compiles the changelog
   fragments (a stable cut after RCs also folds in the RCs' notes) and commits
   `<version>` (`scripts/bump.js`);
2. pushes the branch and the `v<version>` tag (`scripts/tag.js`).

The tag starts `.github/workflows/release.yml`: macOS, Windows and Linux build
and sign in parallel, and a single publish job creates the GitHub release with
every asset, `latest.json` and the LLM catalog **only when all three
succeeded**. A failed platform publishes nothing; re-run the failed job and
publishing follows. There is no release PR and no promotion step: an RC and
its stable version are separate builds of the same source, and the stable one
reports its bare version.

| Flag            | Meaning                                                     |
|-----------------|-------------------------------------------------------------|
| `--dry-run`     | Show what would happen; change nothing.                     |
| `--check`       | Run `npm test -- full` before bumping (off by default: CI runs the checks before anything publishes). |
| `--cross-check` | Type-check the Windows build first (`npm run check:cross`). |
| `--yes`         | `stable`: skip typing the version to confirm.               |
| `--force`       | Skip bump's "previous version is tagged and pushed" check.  |

`bump` and `tag` still work on their own (`npm run bump -- 1.2.0-rc.1`,
`npm run tag`) for recovery; `release` re-tags a bump commit whose tag push died.

### Versions and channels

`VERSION` is the source of truth. The only other copy is
`[workspace.package] version` in the root `Cargo.toml`, which `src-tauri`,
`skill-daemon` and `skill-tty` inherit; Tauri reads its version from
`src-tauri` (`tauri.conf.json` has none). `npm run bump` writes both and CI
fails if they drift.

| Version      | Published as | Update channel | Manifest URL                                          |
|--------------|--------------|----------------|-------------------------------------------------------|
| `x.y.z-rc.N` | pre-release  | rc             | `…/releases/download/rc-latest/latest.json`           |
| `x.y.z`      | latest       | stable         | `…/releases/latest/download/latest.json`              |

`rc-latest` is a long-lived release whose `latest.json` (and LLM catalog) every
build overwrites, RC or stable, so a stable release overtakes an in-flight RC
for rc users. Users opt in via **Settings → Updates → Receive pre-releases**
(`<app_local_data>/update-channel.txt`, read by the background poll live and by
the plugin's static endpoint at startup). The About page shows `v0.5.1` or
`v0.5.1 (RC · abc1234)`; the commit comes from `BUILD_INFO_COMMIT`
(`src-tauri/build.rs`).

### Moving parts

| File / workflow                                   | Role                                                          |
|---------------------------------------------------|---------------------------------------------------------------|
| `scripts/release.js`                              | `rc` / `stable` / `status`: plan the version, bump, push, tag |
| `scripts/bump.js`                                 | VERSION + Cargo version, changelog, `<version>` commit        |
| `scripts/tag.js`                                  | `git tag v${VERSION}` + push to every remote                  |
| `.github/workflows/release.yml`                   | Tag → build all platforms → publish once                      |
| `.github/workflows/release-{mac,linux,windows}.yml` | Reusable per-platform build + sign; stage assets, publish nothing |
| `scripts/ci.mjs stage-release-assets`             | A build job's assets + updater entry → workflow artifact      |
| `scripts/ci.mjs publish-release`                  | Draft with every asset + `latest.json` + catalog → publish; mirror to `rc-latest` |
| `.github/actions/release-setup`                   | Shared Rust + rust-cache + Node (+ optional frontend)         |
| `scripts/compile-product.mjs`                     | Shared daemon (OS umbrella) → tty → app (`custom-protocol`)   |
| `.github/workflows/homebrew-cask.yml`             | After a release, pins the cask to the latest stable           |

### Builds without a release

```bash
npm run ci:dry-run [-- --skip-compile]   # local: frontend + Rust + .app + changelog
gh workflow run "Build - Mac"            # CI: signed build, artifacts kept 14 days
gh workflow run "Build - Linux"
gh workflow run "Build - Windows"        # dry_run (unsigned) by default
```

Label a PR `build` (or run "Preview Build") for preview artifacts of a branch.

## Secrets & Keychain

In **release builds**, API tokens and device credentials are stored in the
system keychain (macOS Keychain, Windows Credential Manager, Linux Secret
Service) and stripped from `settings.json`.

In **debug builds** (`tauri dev` / `cargo run`), the keychain is **skipped
entirely**.  Secrets stay in `settings.json` so they persist across rebuilds.
This avoids macOS prompting for keychain access on every launch — the dev
binary has a different code signature on every build, which triggers a new
authorization dialog each time.

> **No password prompts during development.** If you're seeing keychain
> dialogs, make sure you're running a debug build (`npm run tauri dev`),
> not a release build.

## Architecture Notes

- **Thin client + daemon**: The Tauri app is a UI shell. All persistent business logic
  (EEG, LLM, history, embeddings, HTTP API) runs in **`skill-daemon`**, a standalone
  binary shipped as a Tauri sidecar and also installable as a system service.
- All Rust crates are **Tauri-independent** — they can be tested and used standalone.
- The workspace shares a single `target/` directory (configured in `.cargo/config.toml`).
- Frontend uses **SvelteKit** in SPA mode with **Tailwind CSS v4**.
- EEG processing uses the device's actual sample rate — never hardcode 256 Hz.
- GPU backends (CUDA / Metal / MLX / wgpu / Burn) are feature-gated. Product
  builds use `scripts/compile-product.mjs` with OS umbrellas `apple` /
  `linux` / `windows`. Linux and Windows compile CUDA **and** wgpu so the
  daemon can fall back at runtime (CUDA → wgpu → CPU) when no NVIDIA driver
  is present. CUDA compiles without a local toolkit (cudarc dynamic-loading).
- See `AGENTS.md` for comprehensive rules on encoding, accent colors, session files,
  multi-device DSP, and crate boundaries.

### Daemon

```bash
# Run the daemon manually (dev)
npm run daemon                 # tsx scripts/daemon.ts

# Stop any running daemon
npm run stop                   # pkill -f skill-daemon

# Audit daemon HTTP routes vs frontend invokes (the invoke guard is part of `npm run check`)
npm run audit:daemon-routes

# Data health check
npm run health                 # or: SKILL_DIR=/path/to/.skill npm run health
```

On a fresh install (any OS), the installer kills any old daemon, installs the new
binary, and starts it — never leave two daemons running side by side.

## CLI Tools

Two TypeScript CLIs ship alongside the desktop app:

- **`neuroskill`** — query the daemon for activity, brain state, dev loops, terminal
  impact, and install shell hooks / VS Code extensions. Top-level subcommands include
  `activity`, `brain`, `terminal`, and `vscode`.
- **`neuroloop`** — focus loop / pomodoro CLI driven by EEG state.

```bash
npm run skill                  # Run neuroskill from source (tsx)
npm run neuroskill             # Build + run neuroskill
npm run neuroloop              # Build + run neuroloop
```

## Extensions

Located under `extensions/`:

- **`extensions/vscode/`** — VS Code / VSCodium / Cursor extension (separate
  repo, included as a git submodule). Tracks 50+ event types and renders the brain
  dashboard in the activity bar. Use `git submodule update --init --recursive` after
  cloning, or run `npm run pull`.
- **`extensions/browser/`** — browser extension.
- **`extensions/widgets/`** — macOS WidgetKit extension (built/tested via the
  `widgets` test suite on macOS).

## Packaging

Local release builds go through `scripts/tauri-build.js`:

```bash
npm run tauri:build                    # host target (add -- --target <triple> --bundles appimage for Linux AppImages)
npm run dist:mac                       # .app + .dmg
npm run dist:win                       # NSIS installer (.exe)
npm run dist:linux                     # portable tarball   (-- --target <triple>)
npm run dist:linux:system              # .deb + .rpm         (-- --target <triple>)
npm run brew:cask:generate             # Homebrew cask (CI does this after a stable release)

npm test -- daemon-packaging           # verify the packaged daemon sidecar (auto-detects OS)
bash scripts/test-daemon-packaging.sh --os macos --build   # build + verify in one step
```

See [docs/LINUX.md](docs/LINUX.md), [docs/WINDOWS.md](docs/WINDOWS.md) and
[docs/RASPBERRY_PI.md](docs/RASPBERRY_PI.md) for platform details.

### Scripts you run by hand

| Script | What it does |
|---|---|
| `scripts/build-macos.sh [--debug]` | Builds the WidgetKit widgets, then the app, and embeds the widget `.appex` in the bundle |
| `scripts/e2e-neuroskill-daemon.sh` | Starts a fresh daemon with the virtual EEG device and runs every `neuroskill` CLI command against it |
| `scripts/e2e-neuroloop-daemon.sh` | Same for neuroloop → neuroskill → daemon, including labels and LLM calls |
| `scripts/test-upgrade-linux-docker.sh [A\|B\|both]` | Runs the daemon-upgrade end-to-end tests in a Linux container (`Dockerfile.upgrade-test`) |
| `scripts/test-skill-tty-linux.sh` | Compile-checks `skill-tty` for Linux in a `rust:bookworm` container |
| `scripts/create-windows-guest.sh [iso] [disk]` | Boots a QEMU Windows guest from an installer ISO for testing the Windows release build |
| `scripts/generate-icons.js <image>` | Regenerates every Tauri app icon (PNG, `.ico`, `.icns`) from one source image |
| `scripts/git-push-all.sh [-n] "message"` | Commits and pushes one change across the superproject and every submodule (`-n` previews) |
| `scripts/tts-generate-wavs.sh` | Writes TTS samples to `output/tts-validate/`, one engine at a time to limit memory use |
| `scripts/whisper_validate_tts.py` | Transcribes those samples with openai-whisper and scores how much of the text came through |
| `scripts/orpheus-compare-quants.sh` | Runs Orpheus TTS with Whisper validation for each quantization in turn |
| `scripts/prepare-daemon-sidecar.js` | Builds the daemon and stages it in `src-tauri/binaries/` as the Tauri sidecar |

## Encoding

- All source files and CI artifacts use **UTF-8 without BOM**.
- No literal non-ASCII in CI scripts — use language escapes instead.
- See `AGENTS.md` § "CI / Shared Artifact Encoding Rule" for details.
