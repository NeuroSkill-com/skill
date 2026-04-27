# Contributing to NeuroSkill

## Prerequisites

- **Rust** (latest stable) via [rustup](https://rustup.rs/)
- **Node.js** ≥ 20 with npm
- **Tauri CLI**: installed via `npm install` (workspace devDependency)
- **Platform SDKs**: Xcode (macOS), Visual Studio Build Tools + LLVM (Windows), build-essential (Linux)
- **Vulkan SDK** (Linux/Windows, for `llm-vulkan` feature)

## Quick Start

```bash
# Clone
git clone https://github.com/NeuroSkill-com/skill.git
cd skill

# Install all platform dependencies + JS deps (interactive)
npm run setup

# Ensure Rust is up to date (CI uses latest stable)
rustup update stable

# Run in development mode (starts Vite dev server + Tauri)
npm run tauri dev

# Or build a release
npm run tauri:build
```

`npm run setup` auto-detects your platform and installs everything needed
(protobuf, OpenMP, GNU ar, sccache, etc.).  Pass `--yes` to skip prompts.
See also `npm run setup:build-cache` and `npm run setup:llama-prebuilt`
for optional build acceleration.

## Project Structure

```
├── crates/                  # Rust workspace crates (Tauri-independent)
│   ├── skill-daemon/        # Standalone HTTP daemon (sidecar)
│   ├── skill-daemon-routes/ # Daemon HTTP routes (/v1/...)
│   ├── skill-daemon-state/  # Daemon shared state + DB
│   ├── skill-daemon-common/ # Shared daemon types
│   ├── skill-eeg/           # EEG signal processing
│   ├── skill-exg/           # ExG (multi-modal biosignal) inference
│   ├── skill-gpu/           # GPU FFT / UMAP backends (MLX, CUDA, Vulkan)
│   ├── skill-llm/           # Local LLM inference (llama.cpp, MLX, Burn)
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
│   ├── skill-data/          # Data layer (SQLite, FTS5, embeddings)
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
│   ├── ci.mjs               # Cross-platform CI helpers
│   ├── test-picker.mjs      # Interactive test suite picker
│   ├── test-all.sh          # Suite runner (fmt, lint, clippy, daemon, e2e, …)
│   ├── test-fast.sh         # Tiered Rust unit tests
│   └── shell-hooks/         # zsh/bash/fish/PowerShell preexec hooks
├── docs/                    # Architecture + coverage docs
├── Casks/                   # Generated Homebrew cask
└── changes/                 # Changelog fragments
```

See `AGENTS.md` for the authoritative crate inventory and architecture rules.

## Development Workflow

### Running Tests

```bash
# ── Interactive picker (shows all options) ────────────────────────
npm test                       # Arrow keys + space to pick, enter to run

# ── Quick shortcuts ───────────────────────────────────────────────
npm run test:fast              # fmt + lint + clippy + vitest + rust + ci + types
npm run test:all               # everything (deny, smoke, daemon, e2e, mlx-e2e, widgets, …)

# ── Individual suites ─────────────────────────────────────────────
npm run test:fmt               # Rust + frontend formatting
npm run test:lint              # Frontend lint (biome)
npm run test:clippy            # Rust clippy
npm run test:deny              # Dependency audit (cargo deny)
npm run test:vitest            # Frontend unit tests (vitest)
npm run test:types             # TypeScript/Svelte type checking
npm run test:rust              # Tier 1 Rust tests (~5 s warm)
npm run test:rust:all          # All Rust tiers (~65 s clean)
npm run test:ci                # CI script self-test
npm run test:a11y              # Accessibility audit
npm run test:i18n              # i18n key validation
npm run test:changelog         # Changelog fragment check
npm run test:smoke             # Build verification
npm run test:e2e               # LLM E2E (downloads model, ~15 s cached)
npm run test:mlx-e2e           # MLX backend E2E — UMAP + FFT (macOS only)
npm run test:llm:e2e           # Direct cargo LLM E2E (no shell wrapper)

# ── Daemon packaging (sidecar install/upgrade) ────────────────────
npm run test:daemon-packaging        # Auto-detects OS
npm run test:daemon-packaging:mac    # macOS-specific
npm run test:daemon-packaging:linux  # Linux-specific
npm run test:daemon-packaging:win    # Windows (PowerShell)

# ── Git hook checks ──────────────────────────────────────────────
npm run test:hooks             # pre-commit + pre-push (full)
npm run test:pre-commit        # Just pre-commit checks
npm run test:pre-push          # Full pre-push suite

# ── Mix and match ─────────────────────────────────────────────────
bash scripts/test-all.sh clippy vitest ci      # Run specific suites
bash scripts/test-all.sh --continue all        # Don't stop on first failure
bash scripts/test-all.sh --list                # Show available suites
```

#### Rust Test Tiers

Tests are split into tiers by compilation cost so you get fast feedback:

| Tier | What it tests | Clean build | Warm cache | Tests |
|------|---------------|-------------|------------|-------|
| **1** | Core logic: eeg, tools, jobs, constants, exg, gpu | ~27 s | ~5 s | ~350 |
| **2** | + data, settings, history, health, router, tts, labels, skills, commands | ~53 s | ~8 s | ~550 |
| **3** | + screenshots, devices, llm (adds ML, TLS, heavy native deps) | ~65 s | ~15 s | ~650 |
| **E2E** | LLM download → inference → tool calling (manual/CI dispatch) | ~15 s | ~12 s | 1 |

> **Tip:** Install [sccache](https://github.com/mozilla/sccache) (`brew install sccache`) and
> set `RUSTC_WRAPPER=sccache` for faster clean rebuilds.

See [docs/TEST-COVERAGE.md](docs/TEST-COVERAGE.md) for a detailed coverage analysis and gaps.

### Code Quality

```bash
# Frontend lint & format
npm run lint          # Check
npm run lint:fix      # Auto-fix
npm run format        # Format only
npm run format:check  # Check formatting

# Rust
cargo fmt             # Format
cargo fmt --check     # Check formatting
cargo clippy --workspace  # Lint

# Type checking
npm run check         # Svelte + TypeScript

# i18n
npm run sync:i18n:check   # Verify translation keys
npm run sync:i18n:fix     # Auto-fix missing keys
npm run audit:i18n        # Find unused / undefined keys
npm run check:i18n:critical  # Critical-locale coverage

# Catalog + README sync
npm run sync:llm:catalog:check     # LLM model catalog drift
npm run sync:readme:supported:check

# Accessibility
npm run audit:a11y:check

# Dependency audit
cargo audit
cargo deny check -A no-license-field -A parse-error -A license-not-encountered
```

### Faster local `cargo check`

`cargo check -p skill` can be slow on a cold cache because it compiles many heavy deps.
Use these faster options during day-to-day work:

```bash
# Check only crates affected by your branch (same idea as CI)
BASE=origin/main
FLAGS=$(bash scripts/changed-crates.sh "$BASE")
cargo check $FLAGS

# Or check only core app lib target
cargo check -p skill --lib

# Optional: speed up rebuilds with sccache
export RUSTC_WRAPPER=sccache
```

### Commit Checklist

The **pre-commit** hook automatically checks:
- i18n key synchronisation (when i18n files are staged)
- Frontend formatting via Biome
- Rust formatting via `cargo fmt`

The **pre-push** hook runs scoped checks based on changed files:
- Frontend: `biome check` + `vitest related` on changed files
- Rust: `cargo clippy` + `cargo test` on affected crates
- CI scripts: `node scripts/ci.mjs self-test` when `ci.mjs` or workflows change
- Daemon guard: `vitest run daemon-client.test.ts` when daemon proxy or scripts change

### CI & Releases

All CI logic lives in `scripts/ci.mjs` — a single Node.js file with subcommands that
runs on macOS, Linux, and Windows. Workflows call it instead of inline bash/PowerShell.

```bash
# Validate ci.mjs and workflow references
npm run ci:test

# Local release dry-run (builds frontend + Rust + .app bundle + changelog)
npm run ci:dry-run

# Same but skip compile (reuse existing binaries — fast iteration)
npm run ci:dry-run:fast
```

**Available `ci.mjs` commands:**

| Command | What it does |
|---------|-------------|
| `resolve-version` | Read version from tauri.conf.json, validate against git tag |
| `verify-secrets NAME...` | Check that env vars are non-empty (no values printed) |
| `prepare-changelog VER OUT [RANGE]` | Extract changelog + contributors to markdown |
| `update-latest-json --platform P ...` | Merge platform entry into Tauri updater manifest |
| `discord-notify --status S ...` | Send Discord webhook notification |
| `download-llama PLAT TARGET FEAT` | Download + validate prebuilt llama.cpp libs |
| `import-apple-cert` | Import .p12 certificate into temporary keychain (macOS) |
| `validate-notarization` | Check Apple notarization credentials pre-flight (macOS) |
| `free-disk-space` | Remove unused toolchains on Linux runners |
| `install-protoc-windows` | Install protoc via Chocolatey or direct download |
| `self-test` | Validate all commands + workflow references |
| `dry-run-release [--target T] [--skip-compile]` | Full local release pipeline |

**Release workflows** (all support on-demand via `workflow_dispatch`):
- `release-mac.yml` — macOS aarch64 (.app + .dmg)
- `release-linux.yml` — Linux x86_64 (.deb + .rpm + portable tar)
- `release-windows.yml` — Windows x64 (.exe NSIS installer)

Tag-triggered runs publish to GitHub Releases. On-demand runs upload artifacts (14-day retention).

### Changelog Fragments

Every feature or bugfix **must** include a changelog fragment:

1. Create a `.md` file in `changes/unreleased/` (e.g., `feat-my-change.md`)
2. Use the format:
   ```markdown
   ### Features

   - **Short title**: description of the change.
   ```
3. Valid categories: `Features`, `Performance`, `Bugfixes`, `Refactor`, `Build`, `CLI`, `UI`, `LLM`, `Server`, `i18n`, `Docs`, `Dependencies`

At release time, `npm run bump` compiles fragments into versioned release notes.

**Note**: The `bump` command includes safety checks to prevent accidental multiple bumps. It verifies that the current version has been properly tagged and pushed to the remote. If you need to bypass these checks (e.g., during recovery), use:

```bash
npm run bump --force
```

After bumping, always create and push the version tag:

```bash
npm run tag
```

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
- GPU backends (CUDA / Vulkan / Metal / MLX / Burn) are feature-gated. The
  `llm-vulkan` feature is the default for Linux/Windows release builds.
- See `AGENTS.md` for comprehensive rules on encoding, accent colors, session files,
  multi-device DSP, and crate boundaries.

### Daemon

```bash
# Run the daemon manually (dev)
npm run daemon                 # tsx scripts/daemon.ts

# Stop any running daemon
npm run stop                   # pkill -f skill-daemon

# Audit daemon HTTP routes vs frontend invokes
npm run audit:daemon-routes
npm run check:daemon-invokes

# Health check
npm run health
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

Cross-platform release builds are produced via `scripts/tauri-build.js`:

```bash
# macOS
npm run tauri:build:mac:app           # .app bundle
npm run tauri:build:mac:dmg           # .app + .dmg

# Linux (native arch)
npm run tauri:build:linux:arm64       # aarch64 AppImage + Vulkan LLM
npm run tauri:build:linux:x64:native  # x86_64 AppImage + Vulkan LLM
npm run package:linux:portable        # Portable tarball
npm run package:linux:system:arm64:native  # .deb / .rpm
npm run package:linux:system:x64:native    # .deb / .rpm

# Windows
npm run tauri:build:win:nsis          # NSIS installer (.exe)

# Homebrew cask (post-release)
npm run brew:cask:generate
```

## Encoding

- All source files and CI artifacts use **UTF-8 without BOM**.
- No literal non-ASCII in CI scripts — use language escapes instead.
- See `AGENTS.md` § "CI / Shared Artifact Encoding Rule" for details.
