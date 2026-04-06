#!/usr/bin/env bash
# ── Development environment setup ──────────────────────────────────────────────
#
# Installs all platform-specific build dependencies so that `npm run tauri dev`
# and `npm run tauri build` work out of the box.
#
# Usage:
#   npm run setup              # interactive — prompts before installing
#   npm run setup -- --yes     # non-interactive — installs everything
#
# What it installs:
#
#   macOS:
#     - protobuf        (protoc compiler for gRPC / protobuf codegen)
#     - libomp          (OpenMP runtime for llama.cpp)
#     - binutils        (GNU ar — avoids "illegal option -- D" warnings)
#     - sccache         (compilation cache, ~50% faster clean rebuilds)
#
#   Linux:
#     - protobuf-compiler, libprotobuf-dev
#     - build-essential, cmake, pkg-config
#     - libssl-dev, libgtk-3-dev, libwebkit2gtk-4.1-dev, libjavascriptcoregtk-4.1-dev
#     - libappindicator3-dev (tray icon)
#     - sccache, mold, clang (build acceleration)
#
#   All platforms:
#     - Rust stable toolchain (via rustup)
#     - Node.js ≥ 18 (checked, not installed)
#
# After running this script, optionally run:
#   npm run setup:build-cache    # detailed sccache + mold setup
#   npm run setup:llama-prebuilt # skip llama.cpp cmake rebuild

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
DIM='\033[2m'
NC='\033[0m'

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
MISSING=()
INSTALLED=()
SKIPPED=()

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  NeuroSkill — Development Environment Setup"
echo "═══════════════════════════════════════════════════════════════"
echo ""
info "Platform: $OS $ARCH"
echo ""

# ── Check basic prerequisites ──────────────────────────────────────────────────

if command_exists rustc; then
  ok "Rust: $(rustc --version)"
else
  err "Rust not found. Install from https://rustup.rs/"
  exit 1
fi

if command_exists node; then
  NODE_VER="$(node --version)"
  ok "Node.js: $NODE_VER"
else
  err "Node.js not found. Install from https://nodejs.org/ (≥ 18)"
  exit 1
fi

if command_exists npm; then
  ok "npm: $(npm --version)"
else
  err "npm not found"
  exit 1
fi

echo ""

# ── macOS dependencies ─────────────────────────────────────────────────────────

if [[ "$OS" == "Darwin" ]]; then
  if ! command_exists brew; then
    err "Homebrew not found. Install from https://brew.sh/"
    exit 1
  fi
  ok "Homebrew: $(brew --version | head -1)"
  echo ""

  TO_INSTALL=()

  # Check each macOS dependency: package | check | description
  # Check is a command name or an absolute path.
  while IFS='|' read -r pkg check desc; do
    found=false
    if [[ "$check" == /* ]]; then
      [[ -e "$check" ]] && found=true
      [[ -e "${check/opt\/homebrew/usr\/local}" ]] && found=true
    else
      command_exists "$check" && found=true
    fi

    if $found; then
      ok "$desc"
    else
      warn "Missing: $desc"
      TO_INSTALL+=("$pkg")
    fi
  done <<'DEPS'
protobuf|protoc|Protocol Buffers compiler (protoc)
libomp|/opt/homebrew/opt/libomp/lib/libomp.dylib|OpenMP runtime (required by llama.cpp)
binutils|/opt/homebrew/opt/binutils/bin/gar|GNU ar (fixes 'illegal option -- D' warnings)
sccache|sccache|Compilation cache (~50% faster clean rebuilds)
DEPS

  if [[ ${#TO_INSTALL[@]} -gt 0 ]]; then
    echo ""
    PKGS_STR="${TO_INSTALL[*]}"
    if confirm "Install missing packages via Homebrew? (${PKGS_STR})"; then
      info "Running: brew install ${PKGS_STR}"
      brew install "${TO_INSTALL[@]}"
      for pkg in "${TO_INSTALL[@]}"; do
        INSTALLED+=("$pkg")
      done
      echo ""
    else
      for pkg in "${TO_INSTALL[@]}"; do
        SKIPPED+=("$pkg")
      done
    fi
  fi

  # ── Verify .envrc / AR setup ───────────────────────────────────────────────

  echo ""
  GAR="$(brew --prefix binutils 2>/dev/null)/bin/gar"
  if [[ -x "$GAR" ]]; then
    if [[ "${AR:-}" == *gar* || "${AR:-}" == *llvm-ar* ]]; then
      ok "AR env var is set correctly: ${AR}"
    else
      warn "AR env var is not set in this shell"
      if command_exists direnv; then
        info "Run 'direnv allow' to activate .envrc (sets AR automatically)"
      else
        info "Tip: install direnv to auto-set AR and other env vars:"
        echo "    brew install direnv"
        echo '    # Add to ~/.zshrc:  eval "$(direnv hook zsh)"'
        echo "    direnv allow"
        echo ""
        info "Or add to your shell profile:"
        echo "    export AR=$GAR"
      fi
      echo ""
    fi
  fi

# ── Linux dependencies ────────────────────────────────────────────────────────

elif [[ "$OS" == "Linux" ]]; then
  if command_exists apt-get; then
    PKG_MGR="apt"
  elif command_exists dnf; then
    PKG_MGR="dnf"
  elif command_exists pacman; then
    PKG_MGR="pacman"
  else
    err "No supported package manager found (apt/dnf/pacman)"
    exit 1
  fi
  ok "Package manager: $PKG_MGR"
  echo ""

  # Core build deps — check via dpkg/command
  if [[ "$PKG_MGR" == "apt" ]]; then
    APT_DEPS=(
      build-essential cmake pkg-config
      protobuf-compiler libprotobuf-dev
      libssl-dev
      libgtk-3-dev libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev
      libayatana-appindicator3-dev
      curl wget file
    )

    APT_MISSING=()
    for pkg in "${APT_DEPS[@]}"; do
      if dpkg -s "$pkg" &>/dev/null; then
        ok "$pkg"
      else
        warn "Missing: $pkg"
        APT_MISSING+=("$pkg")
      fi
    done

    if [[ ${#APT_MISSING[@]} -gt 0 ]]; then
      echo ""
      if confirm "Install ${#APT_MISSING[@]} missing packages via apt?"; then
        info "Running: sudo apt-get install -y ${APT_MISSING[*]}"
        sudo apt-get update -qq
        sudo apt-get install -y "${APT_MISSING[@]}"
        INSTALLED+=("${APT_MISSING[@]}")
      else
        SKIPPED+=("${APT_MISSING[@]}")
      fi
    fi
  fi

  # Build acceleration (sccache, mold, clang)
  echo ""
  info "Build acceleration tools:"
  ACCEL_MISSING=()

  for tool in sccache mold clang; do
    if command_exists "$tool"; then
      ok "$tool: $("$tool" --version 2>&1 | head -1)"
    else
      warn "Missing: $tool"
      ACCEL_MISSING+=("$tool")
    fi
  done

  if [[ ${#ACCEL_MISSING[@]} -gt 0 ]]; then
    echo ""
    if confirm "Install build acceleration tools? (${ACCEL_MISSING[*]})"; then
      if [[ "$PKG_MGR" == "apt" ]]; then
        sudo apt-get install -y "${ACCEL_MISSING[@]}"
      elif [[ "$PKG_MGR" == "dnf" ]]; then
        sudo dnf install -y "${ACCEL_MISSING[@]}"
      elif [[ "$PKG_MGR" == "pacman" ]]; then
        sudo pacman -S --noconfirm "${ACCEL_MISSING[@]}"
      fi
      INSTALLED+=("${ACCEL_MISSING[@]}")
    else
      SKIPPED+=("${ACCEL_MISSING[@]}")
    fi
  fi

else
  warn "Unsupported OS: $OS"
  info "See docs/WINDOWS.md for Windows setup instructions."
  exit 0
fi

# ── npm install ────────────────────────────────────────────────────────────────

echo ""
if [[ -d "node_modules" ]]; then
  ok "node_modules exists"
else
  if confirm "Run npm install?"; then
    npm install
    INSTALLED+=("node_modules")
  fi
fi

# ── Summary ────────────────────────────────────────────────────────────────────

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  Summary"
echo "═══════════════════════════════════════════════════════════════"
echo ""

if [[ ${#INSTALLED[@]} -gt 0 ]]; then
  ok "Installed: ${INSTALLED[*]}"
fi
if [[ ${#SKIPPED[@]} -gt 0 ]]; then
  warn "Skipped: ${SKIPPED[*]}"
fi
if [[ ${#INSTALLED[@]} -eq 0 && ${#SKIPPED[@]} -eq 0 ]]; then
  ok "All dependencies already installed!"
fi

echo ""
info "Optional next steps:"
echo "  npm run setup:build-cache      # detailed sccache / mold setup"
echo "  npm run setup:llama-prebuilt   # skip llama.cpp cmake rebuild (~5 min → 3s)"
echo ""
info "Start developing:"
echo "  npm run tauri dev"
echo ""
