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

## Daemon packaging checks

Validate that release artifacts include the `skill-daemon` sidecar:

```bash
# macOS/Linux auto-detect host OS
npm run test:daemon-packaging

# explicit targets
npm run test:daemon-packaging:mac
npm run test:daemon-packaging:linux
npm run test:daemon-packaging:win
```

Build + verify in one step:

```bash
bash scripts/test-daemon-packaging.sh --os macos --build
bash scripts/test-daemon-packaging.sh --os linux --build
powershell -ExecutionPolicy Bypass -File scripts/test-daemon-packaging.ps1 -Build
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
- `SKILL_DAEMON_SERVICE_AUTOINSTALL=0` (disable daemon background-service auto-install for local testing)

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

**Important**: The `bump` command now includes safety checks to prevent accidental multiple bumps:
- It verifies that the current version has a git tag (`vX.X.X`) locally
- It verifies that the tag has been pushed to a remote
- If either check fails, the bump will be aborted with instructions

To bypass these checks (use with caution):
```bash
npm run bump --force
```

After a successful bump, create and push the version tag:
```bash
npm run tag
```

Or manually:
```bash
git tag vX.X.X
git push --tags
```

## Release

### Local dry-run

Test the full release pipeline locally without pushing or signing:

```bash
npm run ci:dry-run             # Full build + bundle + changelog
npm run ci:dry-run:fast        # Skip compile (reuse existing binaries)
```

### On-demand CI build

All release workflows support `workflow_dispatch` — trigger from GitHub Actions UI
or CLI. On-demand runs upload artifacts (14-day retention) without touching Releases:

```bash
gh workflow run "Release - Mac"
gh workflow run "Release — Linux"
gh workflow run "Release - Windows"
```

### Tagged release

```bash
npm run bump          # Bump version + compile changelog
npm run tag           # Create + push git tag
# CI picks up the tag and publishes to GitHub Releases automatically
```

### CI script validation

```bash
npm run ci:test       # Verify ci.py commands + workflow references
```
