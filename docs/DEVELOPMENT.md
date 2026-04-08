# Development

## Prerequisites

- Rust (stable)
- Node.js 18+
- Tauri CLI v2
- Python 3 (for model download helper)
- Platform-specific build tools:
  - macOS: Xcode Command Line Tools
  - Linux: see [LINUX.md](./LINUX.md)
  - Windows: see [WINDOWS.md](./WINDOWS.md)

## Setup

```bash
npm run setup -- --yes
python3 -c "from huggingface_hub import snapshot_download; snapshot_download('Zyphra/ZUNA')"
npm run tauri dev
```

## Build

```bash
npm run tauri build
```

## Optional build acceleration

```bash
npm run setup:build-cache
npm run setup:llama-prebuilt
```

Environment toggles:

- `SKILL_NO_SCCACHE=1`
- `SKILL_NO_MOLD=1`
- `unset LLAMA_PREBUILT_DIR` (force local llama.cpp build)

## Data health check

```bash
npm run health
# or
SKILL_DIR=/path/to/.skill npm run health
```

## Docs/README sync helpers

```bash
npm run sync:readme:supported
npm run sync:readme:supported:check
```

## Pre-commit checks

- `cargo clippy --all-targets --all-features -- -D warnings` (in `src-tauri`)
- `npm run check`

Emergency bypass:

```bash
git commit --no-verify
```

## Versioning

```bash
npm run bump
npm run bump 1.2.0
```

This syncs versions across app manifests and compiles changelog fragments.

## Release (local)

```bash
act push
bash release.sh --dry-run
SKIP_UPLOAD=1 bash release.sh
```
