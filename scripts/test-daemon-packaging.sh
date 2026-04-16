#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

os=""
build=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --os)
      os="${2:-}"
      shift 2
      ;;
    --build)
      build=1
      shift
      ;;
    *)
      echo "Unknown arg: $1" >&2
      echo "Usage: $0 [--os macos|linux] [--build]" >&2
      exit 1
      ;;
  esac
done

if [[ -z "$os" ]]; then
  case "$(uname -s)" in
    Darwin) os="macos" ;;
    Linux) os="linux" ;;
    *)
      echo "Unsupported host OS. Pass --os explicitly." >&2
      exit 1
      ;;
  esac
fi

fail() {
  echo "❌ $*" >&2
  exit 1
}

pass() {
  echo "✅ $*"
}

check_macos() {
  if [[ "$build" -eq 1 ]]; then
    (cd "$ROOT_DIR" && npm run tauri:build:mac:dmg)
  fi

  local dmg
  dmg="$(ls -t "$ROOT_DIR"/src-tauri/target/*/release/bundle/dmg/*.dmg 2>/dev/null | head -1 || true)"
  [[ -n "$dmg" ]] || fail "No DMG found. Build one first (npm run tauri:build:mac:dmg)."

  local mnt
  mnt="$(mktemp -d /tmp/neuroskill-dmg.XXXXXX)"
  TEST_DAEMON_MOUNTPOINT="$mnt"
  trap 'if [[ -n "${TEST_DAEMON_MOUNTPOINT:-}" ]]; then hdiutil detach "$TEST_DAEMON_MOUNTPOINT" >/dev/null 2>&1 || true; rmdir "$TEST_DAEMON_MOUNTPOINT" >/dev/null 2>&1 || true; fi' EXIT

  hdiutil attach "$dmg" -mountpoint "$mnt" -nobrowse >/dev/null

  local app_dir app_bin daemon_bin
  app_dir="$mnt/NeuroSkill.app"
  app_bin="$app_dir/Contents/MacOS/NeuroSkill"

  # Daemon may be a nested .app bundle or a flat binary
  if [[ -f "$app_dir/Contents/MacOS/skill-daemon.app/Contents/MacOS/skill-daemon" ]]; then
    daemon_bin="$app_dir/Contents/MacOS/skill-daemon.app/Contents/MacOS/skill-daemon"
  else
    daemon_bin="$app_dir/Contents/MacOS/skill-daemon"
  fi

  [[ -f "$app_bin" ]] || fail "App binary missing in DMG: $app_bin"
  [[ -f "$daemon_bin" ]] || fail "Daemon sidecar missing in DMG: $daemon_bin"
  [[ -x "$daemon_bin" ]] || fail "Daemon sidecar is not executable: $daemon_bin"

  pass "DMG bundles skill-daemon correctly ($dmg)"
}

check_linux() {
  if [[ "$build" -eq 1 ]]; then
    (cd "$ROOT_DIR" && npm run package:linux:portable)
  fi

  local pkg_dir
  pkg_dir="$(ls -dt "$ROOT_DIR"/dist/linux/*/NeuroSkill 2>/dev/null | head -1 || true)"
  [[ -n "$pkg_dir" ]] || fail "No portable package dir found under dist/linux/*/NeuroSkill"

  [[ -f "$pkg_dir/skill" ]] || fail "Missing app binary: $pkg_dir/skill"
  [[ -f "$pkg_dir/skill-daemon" ]] || fail "Missing daemon sidecar: $pkg_dir/skill-daemon"
  [[ -x "$pkg_dir/skill-daemon" ]] || fail "Daemon sidecar is not executable: $pkg_dir/skill-daemon"

  local archive
  archive="$(ls -t "$ROOT_DIR"/dist/linux/*/NeuroSkill_*_linux-portable.tar.gz 2>/dev/null | head -1 || true)"
  [[ -n "$archive" ]] || fail "No portable archive found under dist/linux"
  tar -tf "$archive" | rg -q '^NeuroSkill/skill-daemon$' || fail "Archive does not contain NeuroSkill/skill-daemon: $archive"

  pass "Linux portable package bundles skill-daemon correctly ($archive)"
}

case "$os" in
  macos) check_macos ;;
  linux) check_linux ;;
  *) fail "Unsupported --os value: $os" ;;
esac
