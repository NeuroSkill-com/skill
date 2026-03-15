#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
REPO="NeuroSkill-com/skill"
VERSION=""
SHA256=""
ASSET_NAME=""

usage() {
  cat <<'EOF'
Usage: bash scripts/generate-homebrew-cask.sh [options]

Options:
  --version <x.y.z>      Version to publish (default: from src-tauri/tauri.conf.json)
  --sha256 <hex>         SHA-256 for DMG (default: read from GitHub release asset digest)
  --repo <owner/name>    GitHub repo (default: NeuroSkill-com/skill)
  --asset <name>         DMG asset name (default: NeuroSkill_<version>_aarch64.dmg)
  --help                 Show this help

Examples:
  bash scripts/generate-homebrew-cask.sh
  bash scripts/generate-homebrew-cask.sh --version 0.0.37
  bash scripts/generate-homebrew-cask.sh --version 0.0.37 --sha256 <sha256>
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      VERSION="${2:-}"
      shift 2
      ;;
    --sha256)
      SHA256="${2:-}"
      shift 2
      ;;
    --repo)
      REPO="${2:-}"
      shift 2
      ;;
    --asset)
      ASSET_NAME="${2:-}"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ -z "$VERSION" ]]; then
  VERSION="$(python3 - <<'PY'
import json
from pathlib import Path
conf = Path('src-tauri/tauri.conf.json')
print(json.loads(conf.read_text(encoding='utf-8'))['version'])
PY
)"
fi

if [[ -z "$ASSET_NAME" ]]; then
  ASSET_NAME="NeuroSkill_${VERSION}_aarch64.dmg"
fi

if [[ -z "$SHA256" ]]; then
  SHA256="$(python3 - "$REPO" "$VERSION" "$ASSET_NAME" <<'PY'
import json
import sys
import urllib.request

repo, version, asset_name = sys.argv[1], sys.argv[2], sys.argv[3]
url = f"https://api.github.com/repos/{repo}/releases/tags/v{version}"

try:
    with urllib.request.urlopen(url, timeout=30) as response:
        release = json.load(response)
except Exception:
    print("")
    raise SystemExit(0)

sha = ""
for asset in release.get("assets", []):
    if asset.get("name") != asset_name:
        continue
    digest = asset.get("digest") or ""
    if digest.startswith("sha256:"):
        sha = digest.split(":", 1)[1].strip()
        break

print(sha)
PY
)"
fi

if [[ -z "$SHA256" ]]; then
  echo "Could not resolve SHA-256 automatically from GitHub release metadata." >&2
  echo "Pass it explicitly with --sha256 <hex>." >&2
  exit 1
fi

if [[ ! "$SHA256" =~ ^[0-9a-fA-F]{64}$ ]]; then
  echo "Invalid SHA-256: $SHA256" >&2
  exit 1
fi

mkdir -p "$ROOT_DIR/Casks"

cat > "$ROOT_DIR/Casks/neuroskill.rb" <<EOF
cask "neuroskill" do
  version "$VERSION"
  sha256 "$SHA256"

  url "https://github.com/$REPO/releases/download/v#{version}/NeuroSkill_#{version}_aarch64.dmg"
  name "NeuroSkill"
  desc "State of Mind brain-computer interface system"
  homepage "https://github.com/$REPO"

  depends_on arch: :arm64

  app "NeuroSkill.app"

  zap trash: [
    "~/Library/Application Support/com.neuroskill.skill",
    "~/Library/Caches/com.neuroskill.skill",
    "~/Library/Preferences/com.neuroskill.skill.plist",
    "~/Library/Saved Application State/com.neuroskill.skill.savedState"
  ]
end
EOF

rm -f "$ROOT_DIR/Casks/skill.rb"

echo "Updated Casks/neuroskill.rb"
echo "  version: $VERSION"
echo "  sha256 : $SHA256"
