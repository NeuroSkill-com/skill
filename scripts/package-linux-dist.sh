#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

target=""
skip_build=0
features="llm-vulkan"
output_root=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      target="${2:-}"
      shift 2
      ;;
    --features)
      features="${2:-}"
      shift 2
      ;;
    --skip-build)
      skip_build=1
      shift
      ;;
    --output)
      output_root="${2:-}"
      shift 2
      ;;
    *)
      echo "Unknown argument: $1" >&2
      echo "Usage: $0 [--target <triple>] [--features <cargo-features>] [--skip-build] [--output <dir>]" >&2
      exit 1
      ;;
  esac
done

if [[ -z "$target" ]]; then
  case "$(uname -m)" in
    x86_64)  target="x86_64-unknown-linux-gnu" ;;
    aarch64) target="aarch64-unknown-linux-gnu" ;;
    *)
      echo "Unsupported host arch: $(uname -m). Please pass --target explicitly." >&2
      exit 1
      ;;
  esac
fi

if [[ -z "$output_root" ]]; then
  output_root="$ROOT_DIR/dist/linux/$target"
fi

version="$(node -p "JSON.parse(require('fs').readFileSync('$ROOT_DIR/package.json','utf8')).version")"
binary_path="$ROOT_DIR/src-tauri/target/$target/release/skill"
resources_dir="$ROOT_DIR/src-tauri/resources"

echo "→ Linux portable package target: $target"
echo "→ Version: $version"

if [[ "$skip_build" -eq 0 ]]; then
  echo "→ Building release binary without Tauri bundling"
  node "$ROOT_DIR/scripts/tauri-build.js" build \
    --target "$target" \
    --features "$features" \
    --no-bundle
fi

if [[ ! -f "$binary_path" ]]; then
  echo "Expected release binary not found: $binary_path" >&2
  exit 1
fi

if [[ ! -d "$resources_dir/neutts-samples" ]]; then
  echo "Missing resources/neutts-samples." >&2
  exit 1
fi

package_root="$output_root/NeuroSkill"
archive_name="NeuroSkill_${version}_${target}_linux-portable.tar.gz"
archive_path="$output_root/$archive_name"

rm -rf "$package_root"
mkdir -p "$package_root/resources"

cp "$binary_path" "$package_root/skill"
chmod +x "$package_root/skill"

# ── Bundle ONNX Runtime shared library ───────────────────────────────────────
# ort-sys downloads libonnxruntime.so into Cargo's OUT_DIR at build time.
# The binary links against it dynamically (DT_NEEDED: libonnxruntime.so.1).
# Without bundling it the binary will fail to start on users' machines.
# We place it next to the binary and patch the rpath to $ORIGIN so the
# dynamic linker finds it regardless of system-installed ORT.
ORT_SO="$(find "$ROOT_DIR/src-tauri/target/$target/release/build" \
  -name "libonnxruntime.so.*" ! -name "*.sig" 2>/dev/null | head -1)"
if [[ -n "$ORT_SO" ]]; then
  ORT_SONAME="$(basename "$ORT_SO")"
  cp "$ORT_SO" "$package_root/$ORT_SONAME"
  # Create the soname symlink (libonnxruntime.so.1 → libonnxruntime.so.1.x.y)
  SONAME_SHORT="$(echo "$ORT_SONAME" | grep -oE 'libonnxruntime\.so\.[0-9]+')"
  if [[ -n "$SONAME_SHORT" && "$SONAME_SHORT" != "$ORT_SONAME" ]]; then
    ln -sf "$ORT_SONAME" "$package_root/$SONAME_SHORT"
  fi
  patchelf --add-rpath '$ORIGIN' "$package_root/skill"
  echo "✓ Bundled ONNX Runtime: $ORT_SONAME"
else
  echo "⚠ ONNX Runtime shared library not found in build output — binary may fail to start" >&2
fi

cp -R "$resources_dir/neutts-samples" "$package_root/resources/"

cp "$ROOT_DIR/LICENSE" "$package_root/"
cp "$ROOT_DIR/docs/LINUX.md" "$package_root/"

cat > "$package_root/neuroskill" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export NEUTTS_SAMPLES_DIR="$APP_DIR/resources/neutts-samples"

cd "$APP_DIR"
exec "$APP_DIR/skill" "$@"
EOF

chmod +x "$package_root/neuroskill"

cp "$ROOT_DIR/src-tauri/icons/128x128.png" "$package_root/icon.png"

cat > "$package_root/neuroskill.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=NeuroSkill
Comment=Neurofeedback and local AI assistant
Exec=neuroskill
Icon=icon
Terminal=false
Categories=Education;Science;
EOF

mkdir -p "$output_root"
rm -f "$archive_path"
tar -czf "$archive_path" -C "$output_root" NeuroSkill

echo "✓ Portable package directory: $package_root"
echo "✓ Portable archive: $archive_path"
