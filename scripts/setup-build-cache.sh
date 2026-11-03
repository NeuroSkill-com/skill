#!/usr/bin/env bash
# ── Setup kache (or sccache) + mold for faster builds ─────────────────────────
#
# This script installs a compiler cache and mold (fast linker, Linux only) to
# speed up Rust/C++ builds by ~50%.  kache is preferred: it shares one store
# across worktrees and sibling repos (../rlx, ../rlx-models) and keeps
# incremental builds for the crates being edited.  sccache is offered as the
# fallback when kache is declined or cannot be installed.
#
# Usage:
#   bash scripts/setup-build-cache.sh          # interactive — prompts before installing
#   bash scripts/setup-build-cache.sh --yes    # non-interactive — installs everything
#
# Platform support:
#   macOS  — installs kache (Homebrew or cargo) or sccache; mold not needed
#   Linux  — installs kache (cargo) or sccache, plus mold + clang (apt/dnf/pacman)
#   Windows — see scripts/setup-build-cache.ps1
#
# No configuration files are modified. The build wrapper (scripts/tauri-build.js)
# auto-detects kache, sccache and mold at build time and sets the env vars.
# `kache init` (suggested, not run) additionally enables kache for plain `cargo`.

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

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  Build Cache Setup — kache/sccache + mold"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# ── Compiler cache: kache (preferred) or sccache ──────────────────────────────

# kache needs Rust 1.95+ to build from source; Homebrew ships a binary.
rustc_at_least() {
  local v
  v="$(rustc --version 2>/dev/null | awk '{print $2}')" || return 1
  [[ -n "$v" ]] || return 1
  local major minor
  IFS=. read -r major minor _ <<<"$v"
  (( major > $1 || (major == $1 && minor >= $2) ))
}

install_kache() {
  _install_kache || return 1
  command_exists kache || { err "kache installation failed"; return 1; }
  ok "kache installed: $(kache --version 2>&1 | head -1)"
}

_install_kache() {
  if [[ "$OS" == "Darwin" ]] && command_exists brew; then
    info "Installing via Homebrew..."
    brew install kunobi-ninja/kunobi/kache
  elif command_exists cargo && rustc_at_least 1 95; then
    info "Installing via cargo (this takes a few minutes)..."
    cargo install kache --locked
  else
    err "kache needs Homebrew (macOS) or Rust 1.95+ for cargo install."
    echo "  Other packages (APT, AUR, Windows, mise, Nix):"
    echo "  https://kunobi.ninja/docs/kache/getting-started/installation"
    return 1
  fi
}

install_sccache() {
  if [[ "$OS" == "Darwin" ]] && command_exists brew; then
    info "Installing via Homebrew..."
    brew install sccache
  elif command_exists cargo; then
    info "Installing via cargo (this may take a minute)..."
    cargo install sccache --locked
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
    return 1
  fi
}

if command_exists kache; then
  ok "kache already installed: $(kache --version 2>&1 | head -1)"
elif command_exists sccache && ! $AUTO_YES; then
  ok "sccache already installed: $(sccache --version)"
  echo ""
  echo "  kache is preferred over sccache when both are installed: it shares one"
  echo "  cache across worktrees and sibling repos and keeps incremental builds."
  echo ""
  if confirm "Also install kache?"; then
    install_kache || warn "kache not installed; builds keep using sccache"
  else
    info "Keeping sccache"
  fi
else
  warn "No compiler cache found"
  echo ""
  echo "  kache caches Rust and C/C++ compilation outputs in one store shared"
  echo "  across worktrees. Clean rebuilds become ~50% faster after the first build."
  echo ""

  if confirm "Install kache?"; then
    install_kache || true
  fi

  if ! command_exists kache && ! command_exists sccache; then
    if confirm "Install sccache instead?"; then
      install_sccache || true
      if command_exists sccache; then
        ok "sccache installed: $(sccache --version)"
      else
        err "sccache installation failed"
      fi
    else
      info "Skipping the compiler cache"
    fi
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

if command_exists kache; then
  ok "kache: enabled (auto-detected by npm run tauri dev/build, scripts/test-all.sh and the pre-push hook)"
  if ! grep -qs kache "$HOME/.cargo/config.toml"; then
    info "  Run \`kache init\` to use it for plain \`cargo\` commands too"
    info "  (it edits ~/.cargo/config.toml and your shell profile and shows the changes first)"
  fi
  command_exists sccache && info "  sccache is installed too; kache takes precedence"
elif command_exists sccache; then
  ok "sccache: enabled (auto-detected by npm run tauri dev/build)"
  info "  Cache location: $(sccache --show-stats 2>/dev/null | grep 'Cache location' | sed 's/.*: *//' || echo "$HOME/.cache/sccache")"
else
  warn "No compiler cache installed (builds will be slower on clean rebuilds)"
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
echo "  SKILL_NO_KACHE=1 npm run tauri build     # use sccache instead of kache"
echo "  SKILL_NO_SCCACHE=1 npm run tauri build   # skip sccache"
echo "  SKILL_NO_MOLD=1 npm run tauri build       # skip mold (Linux)"
echo ""
