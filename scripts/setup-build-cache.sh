#!/usr/bin/env bash
# ── Setup sccache + mold for faster builds ─────────────────────────────────────
#
# This script installs sccache (compilation cache) and mold (fast linker,
# Linux only) to speed up Rust/C++ builds by ~50%.
#
# Usage:
#   bash scripts/setup-build-cache.sh          # interactive — prompts before installing
#   bash scripts/setup-build-cache.sh --yes    # non-interactive — installs everything
#
# Platform support:
#   macOS  — installs sccache (via brew or cargo); mold not needed
#   Linux  — installs sccache + mold + clang (via apt/dnf/pacman or cargo)
#   Windows — see scripts/setup-build-cache.ps1 or run:
#             scoop install sccache
#             cargo install sccache
#
# No configuration files are modified. The build wrapper (scripts/tauri-build.js)
# auto-detects sccache and mold at build time and sets the appropriate env vars.

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

AUTO_YES=false
[[ "${1:-}" == "--yes" || "${1:-}" == "-y" ]] && AUTO_YES=true

info()  { echo -e "${CYAN}ℹ${NC}  $*"; }
ok()    { echo -e "${GREEN}✔${NC}  $*"; }
warn()  { echo -e "${YELLOW}⚠${NC}  $*"; }
err()   { echo -e "${RED}✖${NC}  $*"; }

confirm() {
  if $AUTO_YES; then return 0; fi
  local msg="$1"
  read -rp "$(echo -e "${CYAN}?${NC}  ${msg} [Y/n] ")" answer
  [[ -z "$answer" || "$answer" =~ ^[Yy] ]]
}

command_exists() { command -v "$1" &>/dev/null; }

OS="$(uname -s)"
ARCH="$(uname -m)"

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  Build Cache Setup — sccache + mold"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# ── sccache ────────────────────────────────────────────────────────────────────

if command_exists sccache; then
  ok "sccache already installed: $(sccache --version)"
else
  warn "sccache not found"
  echo ""
  echo "  sccache caches Rust and C/C++ compilation outputs."
  echo "  Clean rebuilds become ~50% faster after the first build."
  echo ""

  if confirm "Install sccache?"; then
    if [[ "$OS" == "Darwin" ]] && command_exists brew; then
      info "Installing via Homebrew..."
      brew install sccache
    elif command_exists cargo; then
      info "Installing via cargo (this may take a minute)..."
      cargo install sccache
    elif command_exists apt-get; then
      info "Installing via apt..."
      sudo apt-get update -qq && sudo apt-get install -y sccache
    elif command_exists dnf; then
      info "Installing via dnf..."
      sudo dnf install -y sccache
    elif command_exists pacman; then
      info "Installing via pacman..."
      sudo pacman -S --noconfirm sccache
    else
      err "No supported package manager found. Install manually:"
      echo "  cargo install sccache"
      echo "  or: brew install sccache"
    fi

    if command_exists sccache; then
      ok "sccache installed: $(sccache --version)"
    else
      err "sccache installation failed"
    fi
  else
    info "Skipping sccache"
  fi
fi

# ── mold (Linux only) ─────────────────────────────────────────────────────────

if [[ "$OS" == "Linux" ]]; then
  echo ""
  NEED_MOLD=false
  NEED_CLANG=false

  if command_exists mold; then
    ok "mold already installed: $(mold --version 2>&1 | head -1)"
  else
    NEED_MOLD=true
  fi

  if command_exists clang; then
    ok "clang already installed: $(clang --version 2>&1 | head -1)"
  else
    NEED_CLANG=true
  fi

  if $NEED_MOLD || $NEED_CLANG; then
    PKGS=""
    $NEED_MOLD && PKGS="$PKGS mold"
    $NEED_CLANG && PKGS="$PKGS clang"
    PKGS="${PKGS# }"

    warn "Missing: $PKGS"
    echo ""
    echo "  mold is a fast linker that speeds up the final link step."
    echo "  clang is needed to pass -fuse-ld=mold to the linker."
    echo ""

    if confirm "Install $PKGS?"; then
      if command_exists apt-get; then
        sudo apt-get update -qq && sudo apt-get install -y $PKGS
      elif command_exists dnf; then
        sudo dnf install -y $PKGS
      elif command_exists pacman; then
        sudo pacman -S --noconfirm $PKGS
      elif command_exists zypper; then
        sudo zypper install -y $PKGS
      else
        err "No supported package manager found. Install manually:"
        echo "  sudo apt install $PKGS"
      fi

      command_exists mold && ok "mold installed: $(mold --version 2>&1 | head -1)"
      command_exists clang && ok "clang installed: $(clang --version 2>&1 | head -1)"
    else
      info "Skipping mold/clang"
    fi
  fi
elif [[ "$OS" == "Darwin" ]]; then
  echo ""
  info "macOS detected — mold is not needed (Apple's linker is already fast)"
fi

# ── Summary ────────────────────────────────────────────────────────────────────

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  Summary"
echo "═══════════════════════════════════════════════════════════════"
echo ""

if command_exists sccache; then
  ok "sccache: enabled (auto-detected by npm run tauri dev/build)"
  info "  Cache location: $(sccache --show-stats 2>/dev/null | grep 'Cache location' | sed 's/.*: *//' || echo '~/.cache/sccache')"
else
  warn "sccache: not installed (builds will be slower on clean rebuilds)"
fi

if [[ "$OS" == "Linux" ]]; then
  if command_exists mold && command_exists clang; then
    ok "mold: enabled (auto-detected by npm run tauri dev/build)"
  else
    warn "mold: not installed (linking will use default linker)"
  fi
fi

echo ""
info "No config files were modified. The build wrapper auto-detects"
info "these tools at build time. Just run: npm run tauri dev"
echo ""

# ── Opt-out reminder ───────────────────────────────────────────────────────────
info "To disable at build time:"
echo "  SKILL_NO_SCCACHE=1 npm run tauri build   # skip sccache"
echo "  SKILL_NO_MOLD=1 npm run tauri build       # skip mold (Linux)"
echo ""
