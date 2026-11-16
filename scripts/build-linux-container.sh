#!/usr/bin/env bash
# Build NeuroSkill's Linux packages inside a Debian container, so the glibc they
# need is the container's rather than the build host's.
#
# Why: a binary needs at least the glibc it was linked against. Built on Ubuntu
# 24.04 it needs 2.39, which rules out Raspberry Pi OS Bookworm (2.36). Built in
# `debian:bookworm` it needs at most 2.36 and runs on Bookworm, Trixie and
# Ubuntu 22.04+. The .deb's Depends come from dpkg-shlibdeps against the same
# image, so they name versions that exist there too.
#
# Steps, all inside the container: system packages, Rust + Node, frontend,
# product compile (scripts/compile-product.mjs, the release recipe), glibc
# floor report, ISA portability check (rlx's isa_portability.py: no ungated
# above-ARMv8.0 code, plus `--version` under an emulated Cortex-A72 = Pi 4),
# deb + rpm. Packages land in dist/linux-container/<triple>/.
#
# Usage:
#   scripts/build-linux-container.sh [--target <triple>] [--image debian:bookworm]
#                                    [--scope app|daemon] [--features linux]
#                                    [--volumes | --no-volumes] [--no-isa-check]
#
#   --scope daemon   build only skill-daemon (packaged in the app's place; quick)
#   CARGO_BUILD_JOBS limits parallel compiles (a Docker VM can run out of RAM);
#   ISA_STRICT=1 makes ISA-scan findings fail the build.
#   --volumes        keep node_modules and src-tauri/target in Docker volumes,
#                    not the checkout. Default on macOS, where the checkout's
#                    node_modules and target hold macOS builds.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target=""
image="debian:bookworm"
scope="app"
features="linux"
volumes=""
isa_check=1
in_container=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) target="$2"; shift 2 ;;
    --image) image="$2"; shift 2 ;;
    --scope) scope="$2"; shift 2 ;;
    --features) features="$2"; shift 2 ;;
    --volumes) volumes=1; shift ;;
    --no-volumes) volumes=0; shift ;;
    --no-isa-check) isa_check=0; shift ;;
    --in-container) in_container=1; shift ;;
    -h|--help) sed -n '2,/^set -euo/p' "$0" | sed '$d'; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [[ -z "$target" ]]; then
  case "$(uname -m)" in
    arm64|aarch64) target="aarch64-unknown-linux-gnu" ;;
    x86_64|amd64) target="x86_64-unknown-linux-gnu" ;;
    *) echo "pass --target" >&2; exit 2 ;;
  esac
fi
case "$target" in
  aarch64-unknown-linux-gnu) platform="linux/arm64"; node_arch="arm64" ;;
  x86_64-unknown-linux-gnu) platform="linux/amd64"; node_arch="x64" ;;
  *) echo "unsupported target: $target" >&2; exit 2 ;;
esac
[[ "$scope" == "app" || "$scope" == "daemon" ]] || { echo "--scope app|daemon" >&2; exit 2; }

# ── Host side: start the container and re-enter this script inside it ────────
if [[ "$in_container" == 0 ]]; then
  [[ -n "$volumes" ]] || { [[ "$(uname -s)" == "Darwin" ]] && volumes=1 || volumes=0; }
  tag="$(echo "$image" | tr ':/' '--')-${target%%-*}"
  mounts=(-v "$ROOT_DIR:/src" -v "skill-linux-${tag}-cargo:/root/.cargo-cache")
  if [[ "$volumes" == 1 ]]; then
    mounts+=(-v "skill-linux-${tag}-target:/src/src-tauri/target"
             -v "skill-linux-${tag}-node_modules:/src/node_modules")
  fi
  extra=()
  [[ "$isa_check" == 1 ]] || extra+=(--no-isa-check)
  exec docker run --rm --platform "$platform" "${mounts[@]}" -w /src \
    -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
    -e GITHUB_ACTIONS="${GITHUB_ACTIONS:-}" -e CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-}" \
    -e ISA_STRICT="${ISA_STRICT:-0}" \
    "$image" bash /src/scripts/build-linux-container.sh --in-container \
    --target "$target" --image "$image" --scope "$scope" --features "$features" ${extra[@]+"${extra[@]}"}
fi

# ── Container side ─────────────────────────────────────────────────────────────
cd /src
step() { echo; echo "── $* ──"; }
export DEBIAN_FRONTEND=noninteractive
[[ -n "${CARGO_BUILD_JOBS:-}" ]] || unset CARGO_BUILD_JOBS
# Cargo's registry and git checkouts persist in a volume; the toolchain does not.
export CARGO_HOME=/root/.cargo-cache RUSTUP_HOME=/root/.rustup PATH="/root/.cargo-cache/bin:/opt/node/bin:$PATH"
release_dir="src-tauri/target/$target/release"
out_dir="dist/linux-container/$target"

step "system packages ($(grep -oP '^PRETTY_NAME="\K[^"]*' /etc/os-release))"
mapfile -t pkgs < <(grep -v '^[[:space:]]*#' .github/actions/release-setup/linux-apt-packages.txt | sed '/^[[:space:]]*$/d')
apt-get update -qq
apt-get install -y -qq --no-install-recommends build-essential ca-certificates curl git python3 file xz-utils \
  qemu-user-static "${pkgs[@]}" >/dev/null
git config --global --add safe.directory '*'

step "Rust"
if ! command -v rustup >/dev/null; then
  curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable --no-modify-path >/dev/null
fi
# rustup itself may come from the cached CARGO_HOME while its toolchains
# (RUSTUP_HOME) do not, so always select one.
rustup default stable >/dev/null
rustup target add "$target" >/dev/null
rustc --version

if [[ "$scope" == "app" ]]; then
  step "Node $(cat .nvmrc)"
  base="https://nodejs.org/dist/latest-v$(cat .nvmrc).x"
  sums="$(curl -fsSL "$base/SHASUMS256.txt")"
  file="$(awk -v a="linux-$node_arch.tar.xz" '$2 ~ a"$" {print $2}' <<<"$sums")"
  curl -fsSL -o "/tmp/$file" "$base/$file"
  (cd /tmp && grep " $file\$" <<<"$sums" | sha256sum -c - >/dev/null)
  mkdir -p /opt/node && tar -xJf "/tmp/$file" -C /opt/node --strip-components=1
  # Same as release-setup: the lock is written by current npm, and Node 22's
  # bundled npm 10 resolves peer dependencies differently and rejects it.
  npm install -g npm@latest --no-audit --no-fund >/dev/null
  echo "node $(node --version), npm $(npm --version)"

  step "frontend"
  npm ci --no-audit --no-fund
  npm run build
fi

step "compile ($scope, $target)"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=clang CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C link-arg=-fuse-ld=mold"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C link-arg=-fuse-ld=mold"
isa_scan=0
[[ "$isa_check" == 1 && "$target" == aarch64-* ]] && isa_scan=1
if [[ "$isa_scan" == 1 ]]; then
  # Keep the symbol table for the ISA scan, which attributes each instruction
  # to its function. With mold, rustc's default release `strip = "debuginfo"`
  # drops .symtab entirely, and every finding then lands on whichever exported
  # symbol happens to precede it. Stripped below, after the scan, so the
  # packages match a normal build.
  export CARGO_PROFILE_RELEASE_STRIP=none
fi
if [[ "$scope" == "app" ]]; then
  node scripts/compile-product.mjs --target "$target" --release --locked
else
  cargo build -p skill-daemon --features "$features" --release --locked --target "$target"
  cp "$release_dir/skill-daemon" "$release_dir/skill"
  echo "note: packaged the daemon binary as a stand-in for the app"
fi
bins=()
for b in skill skill-daemon skill-tty; do [[ -f "$release_dir/$b" ]] && bins+=("$release_dir/$b"); done

step "glibc floor"
glibc_here="$(ldd --version | head -1 | grep -o '[0-9.]*$')"
for b in "${bins[@]}"; do
  echo "$(basename "$b") needs $(objdump -T "$b" | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1) (image has $glibc_here)"
done

if [[ "$isa_scan" == 1 ]]; then
  # rlx's own gate: every above-ARMv8.0 instruction (dotprod, fp16, i8mm, sve,
  # crypto) must sit in a function reached through a runtime CPU check, or a
  # Pi 4 (Cortex-A72, no dotprod/fp16) dies with SIGILL. The script lives in
  # the rlx-cpu crate, so take it from whichever rlx this build resolved.
  step "ISA portability (Raspberry Pi 4 = ARMv8.0)"
  rlx_cpu_dir="$(cargo metadata --format-version 1 --locked 2>/dev/null | python3 -c '
import json, os, sys
for p in json.load(sys.stdin)["packages"]:
    if p["name"] == "rlx-cpu":
        print(os.path.dirname(p["manifest_path"])); break')"
  isa="$rlx_cpu_dir/tools/isa_portability.py"
  if [[ -f "$isa" ]]; then
    python3 "$isa" scan --baseline aarch64-v80 "${bins[@]}" || {
      echo "ISA scan: findings above (symbols with ungated above-ARMv8.0 instructions)"
      [[ "${ISA_STRICT:-0}" == 1 ]] && exit 1
    }
  else
    echo "isa_portability.py not found under ${rlx_cpu_dir:-<rlx-cpu not resolved>}; scan skipped"
  fi
  for b in "${bins[@]}"; do
    [[ "$(basename "$b")" == skill ]] && continue # the app needs a display
    for cpu in cortex-a72 cortex-a76; do
      if qemu-aarch64-static -cpu "$cpu" "$b" --version >/tmp/qemu.out 2>&1; then
        echo "ok   $(basename "$b") --version on emulated $cpu: $(head -1 /tmp/qemu.out)"
      else
        cat /tmp/qemu.out
        echo "FAIL $(basename "$b") on emulated $cpu"
        exit 1
      fi
    done
  done
fi

if [[ "$isa_scan" == 1 ]]; then
  strip --strip-all "${bins[@]}"
fi

step "deb + rpm"
bash scripts/package-linux-system-bundles.sh --target "$target" --skip-build
mkdir -p "$out_dir"
cp "$release_dir"/bundle/deb/*.deb "$release_dir"/bundle/rpm/*.rpm "$out_dir"/
for b in "${bins[@]}"; do cp "$b" "$out_dir/"; done
ls -la "$out_dir"

if [[ -n "${HOST_UID:-}" && "${HOST_UID}" != 0 ]]; then
  chown -R "$HOST_UID:${HOST_GID:-$HOST_UID}" dist src-tauri/target node_modules build .svelte-kit 2>/dev/null || true
fi
