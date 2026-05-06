#!/usr/bin/env bash
# ── Verify skill-tty compiles on Linux via docker ─────────────────────────
#
# Cross-compiling from macOS to Linux is fragile because of system deps
# (dbus, udev, etc.); this script runs the compile check inside a clean
# rust:bookworm container.
#
# Disk-safety notes:
#   • Source is mounted READ-ONLY (/skill:ro) so the host's working tree
#     can't be polluted from inside the container.
#   • CARGO_TARGET_DIR is redirected to /tmp/cargo-target (tmpfs) so the
#     ~10 GB of Linux build artifacts disappear when the container exits
#     instead of accumulating in src-tauri/target/.
#   • Cargo registry/git are cached in named volumes so repeat runs reuse
#     downloads without consuming new layer space each time.
#
# Usage:
#   bash scripts/test-skill-tty-linux.sh           # quick: cargo check
#   bash scripts/test-skill-tty-linux.sh --build   # heavier: cargo build --release

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

mode="${1:---check}"
case "$mode" in
  --check) cargo_cmd="check" ;;
  --build) cargo_cmd="build --release" ;;
  *)
    echo "usage: $0 [--check|--build]" >&2
    exit 2
    ;;
esac

echo "→ skill-tty Linux $cargo_cmd via docker (rust:1-bookworm)"
echo "  source mount: $ROOT (ro)"
echo "  cargo target: tmpfs inside container (no host-disk impact)"

docker run --rm \
  -v "$ROOT:/skill:ro" \
  -v skill-cargo-registry:/usr/local/cargo/registry \
  -v skill-cargo-git:/usr/local/cargo/git \
  --tmpfs /tmp/cargo-target:size=20g,exec \
  -w /skill \
  -e CARGO_TARGET_DIR=/tmp/cargo-target \
  rust:1-bookworm \
  sh -c "
    set -e
    apt-get update -qq
    apt-get install -y -qq pkg-config libssl-dev libdbus-1-dev libudev-dev cmake clang >/dev/null
    cargo $cargo_cmd -p skill-tty
    echo
    echo '✓ skill-tty Linux $cargo_cmd succeeded'
    ls -la /tmp/cargo-target/*/skill-tty 2>/dev/null || true
  "
