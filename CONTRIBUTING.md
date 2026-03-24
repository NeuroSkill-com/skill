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
git clone http://192.168.99.99:3000/NeuroSkill-com/skill.git
cd skill

# Install JS dependencies & configure git hooks
npm install

# Run in development mode (starts Vite dev server + Tauri)
npm run tauri dev

# Or build a release
npm run tauri:build
```

## Project Structure

```
├── crates/              # Rust workspace crates (no Tauri deps)
│   ├── skill-eeg/       # EEG signal processing
│   ├── skill-llm/       # Local LLM inference
│   ├── skill-tools/     # LLM function-calling
│   └── ...              # See AGENTS.md for full list
├── src/                 # SvelteKit frontend
│   ├── lib/             # Shared components & utilities
│   ├── routes/          # Page routes
│   └── tests/           # Frontend tests
├── src-tauri/           # Tauri app shell
├── scripts/             # Build & CI scripts
└── changes/             # Changelog fragments
```

## Development Workflow

### Running Tests

```bash
# Frontend tests (Vitest)
npm test

# Rust tests (all crates)
cargo test --workspace

# LLM end-to-end test (requires model download)
npm run test:llm:e2e

# Smoke test (build verification)
npm run test:smoke
```

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

# Dependency audit
cargo audit
cargo deny check
```

### Commit Checklist

The pre-commit hook automatically checks:
- i18n key synchronisation (when i18n files are staged)
- Frontend formatting via Biome
- Rust formatting via `cargo fmt`

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

## Architecture Notes

- All Rust crates are **Tauri-independent** — they can be tested and used standalone.
- The workspace shares a single `target/` directory (configured in `.cargo/config.toml`).
- Frontend uses **SvelteKit** in SPA mode with **Tailwind CSS v4**.
- EEG processing uses the device's actual sample rate — never hardcode 256 Hz.
- See `AGENTS.md` for comprehensive rules on encoding, accent colors, session files, and multi-device DSP.

## Encoding

- All source files and CI artifacts use **UTF-8 without BOM**.
- No literal non-ASCII in CI scripts — use language escapes instead.
- See `AGENTS.md` § "CI / Shared Artifact Encoding Rule" for details.
