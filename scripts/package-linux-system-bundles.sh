#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

target=""
skip_build=0
features="llm-vulkan"

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
    *)
      echo "Unknown argument: $1" >&2
      echo "Usage: $0 [--target <triple>] [--features <cargo-features>] [--skip-build]" >&2
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

case "$target" in
  x86_64-unknown-linux-gnu)
    deb_arch="amd64"
    rpm_arch="x86_64"
    ;;
  aarch64-unknown-linux-gnu)
    deb_arch="arm64"
    rpm_arch="aarch64"
    ;;
  *)
    echo "Unsupported target for system package bundling: $target" >&2
    exit 1
    ;;
esac

if ! command -v dpkg-deb >/dev/null 2>&1; then
  echo "dpkg-deb is required to build .deb packages." >&2
  exit 1
fi

if ! command -v rpmbuild >/dev/null 2>&1; then
  echo "rpmbuild is required to build .rpm packages (install rpm tooling)." >&2
  exit 1
fi

version="$(node -p "JSON.parse(require('fs').readFileSync('$ROOT_DIR/package.json','utf8')).version")"
binary_path="$ROOT_DIR/src-tauri/target/$target/release/skill"
resources_dir="$ROOT_DIR/src-tauri/resources"

bundle_root="$ROOT_DIR/src-tauri/target/$target/release/bundle"
deb_out_dir="$bundle_root/deb"
rpm_out_dir="$bundle_root/rpm"

work_root="$ROOT_DIR/dist/linux/$target/system-bundles"
stage_root="$work_root/neuroskill-root"

echo "→ Linux system package target: $target"
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
  echo "Missing resources/neutts-samples. Build likely incomplete." >&2
  exit 1
fi

rm -rf "$stage_root"
mkdir -p \
  "$stage_root/opt/neuroskill/resources" \
  "$stage_root/usr/bin" \
  "$stage_root/usr/share/applications" \
  "$stage_root/usr/share/pixmaps"

cp "$binary_path" "$stage_root/opt/neuroskill/skill"
chmod +x "$stage_root/opt/neuroskill/skill"

# ── Bundle ONNX Runtime shared library ───────────────────────────────────────
# ort-sys downloads libonnxruntime.so into Cargo's OUT_DIR at build time.
# The binary links against it dynamically (DT_NEEDED: libonnxruntime.so.1).
# Without bundling it the binary will fail to start on users' machines.
# The library goes next to the skill binary in /opt/neuroskill/; patchelf
# adds $ORIGIN to the rpath so the dynamic linker finds it at runtime.
ORT_SO="$(find "$ROOT_DIR/src-tauri/target/$target/release/build" \
  -name "libonnxruntime.so.*" ! -name "*.sig" 2>/dev/null | head -1)"
if [[ -n "$ORT_SO" ]]; then
  ORT_SONAME="$(basename "$ORT_SO")"
  cp "$ORT_SO" "$stage_root/opt/neuroskill/$ORT_SONAME"
  SONAME_SHORT="$(echo "$ORT_SONAME" | grep -oE 'libonnxruntime\.so\.[0-9]+')"
  if [[ -n "$SONAME_SHORT" && "$SONAME_SHORT" != "$ORT_SONAME" ]]; then
    ln -sf "$ORT_SONAME" "$stage_root/opt/neuroskill/$SONAME_SHORT"
  fi
  patchelf --add-rpath '$ORIGIN' "$stage_root/opt/neuroskill/skill"
  echo "✓ Bundled ONNX Runtime: $ORT_SONAME"
else
  echo "⚠ ONNX Runtime shared library not found in build output — binary may fail to start" >&2
fi

cp -R "$resources_dir/neutts-samples" "$stage_root/opt/neuroskill/resources/"
cp "$ROOT_DIR/LICENSE" "$stage_root/opt/neuroskill/LICENSE"
cp "$ROOT_DIR/docs/LINUX.md" "$stage_root/opt/neuroskill/LINUX.md"
cp "$ROOT_DIR/src-tauri/icons/128x128.png" "$stage_root/usr/share/pixmaps/neuroskill.png"

cat > "$stage_root/usr/bin/neuroskill" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

APP_DIR="/opt/neuroskill"
export NEUTTS_SAMPLES_DIR="$APP_DIR/resources/neutts-samples"

exec "$APP_DIR/skill" "$@"
EOF
chmod +x "$stage_root/usr/bin/neuroskill"

cat > "$stage_root/usr/share/applications/neuroskill.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=NeuroSkill
Comment=Neurofeedback and local AI assistant
Exec=neuroskill
Icon=neuroskill
Terminal=false
Categories=Education;Science;
EOF

mkdir -p "$deb_out_dir" "$rpm_out_dir"

deb_pkg_name="neuroskill_${version}_${deb_arch}.deb"
deb_build_root="$work_root/deb-root"
rm -rf "$deb_build_root"
mkdir -p "$deb_build_root/DEBIAN"
cp -a "$stage_root/." "$deb_build_root/"

installed_size="$(du -sk "$deb_build_root/opt/neuroskill" | awk '{print $1}')"
cat > "$deb_build_root/DEBIAN/control" <<EOF
Package: neuroskill
Version: $version
Section: utils
Priority: optional
Architecture: $deb_arch
Maintainer: NeuroSkill <support@neuroskill.com>
Installed-Size: $installed_size
Description: Neurofeedback and local AI assistant
EOF

dpkg-deb --build --root-owner-group "$deb_build_root" "$deb_out_dir/$deb_pkg_name"

rpm_top="$work_root/rpmbuild"
rm -rf "$rpm_top"
mkdir -p "$rpm_top/BUILD" "$rpm_top/BUILDROOT" "$rpm_top/RPMS" "$rpm_top/SOURCES" "$rpm_top/SPECS" "$rpm_top/SRPMS"

tar -czf "$rpm_top/SOURCES/neuroskill-root.tar.gz" -C "$work_root" "$(basename "$stage_root")"

cat > "$rpm_top/SPECS/neuroskill.spec" <<EOF
Name:           neuroskill
Version:        $version
Release:        1
Summary:        Neurofeedback and local AI assistant
License:        GPL-3.0-only
BuildArch:      $rpm_arch
Source0:        neuroskill-root.tar.gz

%description
NeuroSkill local desktop application with EEG tooling and local AI features.

%prep
%setup -q -n neuroskill-root

%build

%install
mkdir -p %{buildroot}
cp -a . %{buildroot}/

%files
/opt/neuroskill
/usr/bin/neuroskill
/usr/share/applications/neuroskill.desktop
/usr/share/pixmaps/neuroskill.png

%changelog
* $(date '+%a %b %d %Y') NeuroSkill CI <ci@neuroskill.com> - $version-1
- CI system-tool Linux package build
EOF

rpmbuild -bb "$rpm_top/SPECS/neuroskill.spec" --define "_topdir $rpm_top" --target "$rpm_arch"

rpm_file="$(find "$rpm_top/RPMS" -type f -name "neuroskill-*.rpm" | head -1 || true)"
if [[ -z "$rpm_file" ]]; then
  echo "RPM build finished but no rpm artifact was found under $rpm_top/RPMS" >&2
  exit 1
fi

cp "$rpm_file" "$rpm_out_dir/"

echo "✓ System-built .deb: $deb_out_dir/$deb_pkg_name"
echo "✓ System-built .rpm: $rpm_out_dir/$(basename "$rpm_file")"