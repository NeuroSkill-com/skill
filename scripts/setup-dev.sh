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
#     - sccache         (compilation cache, ~50% faster clean rebuilds;
#                        skipped when kache, the preferred cache, is installed)
#
#   Linux:
#     - protobuf-compiler, libprotobuf-dev
#     - build-essential, cmake, pkg-config
#     - libssl-dev, libopenblas-dev (CPU BLAS/linalg), libgtk-3-dev, libwebkit2gtk-4.1-dev, libjavascriptcoregtk-4.1-dev
#     - libappindicator3-dev (tray icon)
#     - sccache (skipped when kache is installed), mold, clang (build acceleration)
#
#   All platforms:
#     - Rust stable toolchain (via rustup)
#     - Node.js ≥ 22.17 (SvelteKit 3 and Vitest 5 need it). If it is missing or
#       older, offers to install the latest Node 22 LTS from nodejs.org into
#       ~/.local (checksum-verified, no sudo), linked into ~/.local/bin
#
# After running this script, optionally run:
#   npm run setup:build-cache    # kache (preferred) or sccache, plus mold

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
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

# Keep in sync with "engines" in package.json.
NODE_MIN_MAJOR=22
NODE_MIN_MINOR=17
# Release line installed when Node is missing or too old (CI uses 22 too).
NODE_INSTALL_LINE=22
NODE_HOME_BIN="$HOME/.local/bin"

node_is_new_enough() {
  command_exists node || return 1
  local ver major minor
  ver="$(node --version)"
  major="$(echo "${ver#v}" | cut -d. -f1)"
  minor="$(echo "${ver#v}" | cut -d. -f2)"
  (( major > NODE_MIN_MAJOR || (major == NODE_MIN_MAJOR && minor >= NODE_MIN_MINOR) ))
}

# Install the latest Node ${NODE_INSTALL_LINE} LTS from nodejs.org into ~/.local:
# distro packages are often too old (Ubuntu and Debian ship 18), and this needs
# no sudo. The tarball is checked against the release's SHASUMS256.txt.
install_node() {
  local os arch base shasums file version dest tmp sum
  case "$OS" in
    Darwin) os="darwin" ;;
    Linux)  os="linux" ;;
    *) err "Installing Node.js is not supported on $OS"; return 1 ;;
  esac
  case "$ARCH" in
    x86_64|amd64)  arch="x64" ;;
    arm64|aarch64) arch="arm64" ;;
    armv7l)        arch="armv7l" ;;
    *) err "No Node.js build for architecture $ARCH"; return 1 ;;
  esac
  if ! command_exists curl; then
    err "curl is needed to download Node.js"
    return 1
  fi

  base="https://nodejs.org/dist/latest-v${NODE_INSTALL_LINE}.x"
  shasums="$(curl -fsSL "$base/SHASUMS256.txt")" || { err "Could not reach $base"; return 1; }
  file="$(echo "$shasums" | awk -v want="-${os}-${arch}.tar.gz" '$2 ~ want"$" {print $2; exit}')"
  [[ -n "$file" ]] || { err "No ${os}-${arch} build listed in $base/SHASUMS256.txt"; return 1; }
  version="${file#node-}"; version="${version%-${os}-${arch}.tar.gz}"
  dest="$HOME/.local/opt/node-${version}-${os}-${arch}"

  info "Installing Node.js ${version} into ${dest}"
  tmp="$(mktemp -d)"
  if ! curl -fSL --progress-bar "$base/$file" -o "$tmp/$file"; then
    rm -rf "$tmp"; err "Download failed: $base/$file"; return 1
  fi
  if command_exists sha256sum; then
    sum="$(sha256sum "$tmp/$file" | awk '{print $1}')"
  else
    sum="$(shasum -a 256 "$tmp/$file" | awk '{print $1}')"
  fi
  if ! echo "$shasums" | grep -q "^${sum}  ${file}\$"; then
    rm -rf "$tmp"; err "Checksum mismatch for $file; not installing"; return 1
  fi

  mkdir -p "$HOME/.local/opt" "$NODE_HOME_BIN"
  rm -rf "$dest"
  tar -xzf "$tmp/$file" -C "$HOME/.local/opt"
  rm -rf "$tmp"
  for bin in node npm npx; do
    ln -sf "$dest/bin/$bin" "$NODE_HOME_BIN/$bin"
  done
  # Use it for the rest of this run.
  export PATH="$NODE_HOME_BIN:$PATH"
  hash -r
  INSTALLED+=("node ${version}")

  if ! node_is_new_enough; then
    err "Installed Node.js ${version}, but \`node\` still resolves to $(command -v node) ($(node --version))"
    return 1
  fi
  case ":${PATH_BEFORE_NODE}:" in
    *":$NODE_HOME_BIN:"*) ;;
    *) warn "Add ~/.local/bin to your PATH so new shells find this Node.js:"
       info "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.${SHELL##*/}rc" ;;
  esac
}

PATH_BEFORE_NODE="$PATH"
if node_is_new_enough; then
  ok "Node.js: $(node --version)"
else
  if command_exists node; then
    warn "Node.js $(node --version) is too old: SvelteKit 3 and Vitest 5 need ${NODE_MIN_MAJOR}.${NODE_MIN_MINOR} or newer"
  else
    warn "Node.js not found (need ${NODE_MIN_MAJOR}.${NODE_MIN_MINOR} or newer)"
  fi
  if confirm "Install the latest Node.js ${NODE_INSTALL_LINE} LTS from nodejs.org into ~/.local?"; then
    install_node || exit 1
    ok "Node.js: $(node --version)"
  else
    err "Install Node.js ${NODE_MIN_MAJOR}.${NODE_MIN_MINOR} or newer from https://nodejs.org/, then rerun"
    exit 1
  fi
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
    # kache is the preferred compiler cache; don't push sccache next to it.
    if [[ "$pkg" == sccache ]] && command_exists kache; then
      ok "Compilation cache: kache"
      continue
    fi
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
cmake|cmake|CMake (builds sentencepiece-sys and highs-sys)
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

  # ── Verify AR ──────────────────────────────────────────────────────────────

  echo ""
  if [[ "${AR:-}" == *gar ]]; then
    warn "AR points at GNU ar (${AR}); DuckDB fails to link with it on macOS"
    info "Remove 'export AR=...gar' from your shell profile (Apple's ar is fine)"
    echo ""
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
      libssl-dev libopenblas-dev
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

  # Build acceleration (sccache, mold, clang). kache, the preferred compiler
  # cache, is not in distro repos; `npm run setup:build-cache` installs it.
  echo ""
  info "Build acceleration tools:"
  ACCEL_MISSING=()

  for tool in sccache mold clang; do
    if [[ "$tool" == sccache ]] && command_exists kache; then
      ok "compiler cache: kache"
    elif command_exists "$tool"; then
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
  # `npm ci` installs exactly what package-lock.json records, as CI does.
  # A fresh `npm install` re-resolves and can fail on peer ranges the lockfile
  # already settled (#81).
  if [[ -f "package-lock.json" ]]; then
    NPM_INSTALL=(npm ci)
  else
    NPM_INSTALL=(npm install)
  fi
  if confirm "Run ${NPM_INSTALL[*]}?"; then
    "${NPM_INSTALL[@]}"
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
echo "  npm run setup:build-cache      # kache (preferred) or sccache, plus mold"
echo ""
info "Start developing:"
echo "  npm run tauri dev"
echo ""
