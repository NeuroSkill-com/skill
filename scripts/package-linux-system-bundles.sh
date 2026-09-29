#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

target=""
skip_build=0
features="custom-protocol"

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

version="$(tr -d '[:space:]' < "$ROOT_DIR/VERSION")"

# RPM forbids '-' in Version (it separates Version from Release), so an RC like
# 0.0.131-rc.30 must be rewritten. The replacement has to be a TILDE: rpm sorts
# '~' *below* the bare version, which is exactly what marks a pre-release, so
#   0.0.131~rc.30  <  0.0.131
# Any other separator sorts the other way (0.0.131.rc.30 > 0.0.131), which would
# make an RC look newer than the release it precedes.
#
# The tilde is passed via a variable on purpose. `${version//-/~}` is wrong:
# bash applies tilde expansion to the replacement string, so on Linux that
# yields `0.0.131/rootrc.30`. `${version//-/\~}` is right on bash 5 but leaves a
# literal backslash on bash 3.2 (macOS). Substituting an already-expanded
# variable is unambiguous on both.
rpm_tilde='~'
rpm_version="${version//-/$rpm_tilde}"

# Fail loudly rather than let rpmbuild silently rewrite an invalid Version.
# Allowlist the shape rather than blocklisting known slips, so a future quoting
# mistake cannot get through: rpm permits alphanumerics plus '.' '~' '^' '_' '+'
# in a Version, and notably NOT '-'. This also catches `/`, which is what a
# tilde-expanded replacement produces.
case "$rpm_version" in
  '' | *[!0-9A-Za-z.~^_+]*)
    echo "Refusing to build: computed RPM version '$rpm_version' is not a valid rpm Version." >&2
    echo "Allowed: alphanumerics and . ~ ^ _ + — never '-', '/' or a backslash." >&2
    echo "rpmbuild would rewrite it and could invert upgrade ordering (see comment above)." >&2
    exit 1
    ;;
esac

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

# ── Bundle skill-daemon sidecar ──────────────────────────────────────────────
# The daemon binary sits next to the app binary. At runtime the Tauri app
# starts it and calls /service/install which self-registers a systemd --user
# service. No system-level unit file is needed in the package.
daemon_path="$ROOT_DIR/src-tauri/target/$target/release/skill-daemon"
if [[ -f "$daemon_path" ]]; then
  cp "$daemon_path" "$stage_root/opt/neuroskill/skill-daemon"
  chmod +x "$stage_root/opt/neuroskill/skill-daemon"
  echo "✓ Bundled skill-daemon sidecar"
else
  echo "ERROR: skill-daemon not found at $daemon_path" >&2
  echo "Build with: node scripts/compile-product.mjs --target $target --release" >&2
  exit 1
fi

# ── Bundle skill-tty sidecar ─────────────────────────────────────────────────
# The PTY proxy that wraps the user's shell for terminal-session recording.
# Lives next to skill-daemon so the shell hook (and the daemon's tty exec-shim)
# can find it via current_exe()'s parent directory. Splitting it out means
# blanket process-name kills of skill-daemon don't sweep up active recorded
# shells.
tty_path="$ROOT_DIR/src-tauri/target/$target/release/skill-tty"
if [[ -f "$tty_path" ]]; then
  cp "$tty_path" "$stage_root/opt/neuroskill/skill-tty"
  chmod +x "$stage_root/opt/neuroskill/skill-tty"
  echo "✓ Bundled skill-tty sidecar"
else
  echo "ERROR: skill-tty not found at $tty_path" >&2
  echo "Build with: node scripts/compile-product.mjs --target $target --release" >&2
  exit 1
fi

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

# ── Derive Depends from the binaries, do not hand-list them ──────────────────
#
# `dpkg-deb --build` is a raw packer: unlike `rpmbuild`, which reads each ELF and
# emits a soname requirement per NEEDED entry, it declares exactly what this
# control file says and nothing more.
#
# This used to say `Depends: libopenblas0` and stop there. That single line was
# correct but drastically incomplete: the shipped binary also needs the whole
# GTK/WebKit stack (webkit2gtk-4.1, gtk-3, javascriptcoregtk-4.1, soup-3, cairo,
# gdk-pixbuf, glib), plus asound, dbus, udev, ssl and the C/C++ runtimes — 13
# packages in total against the rc.31 build. `apt install` therefore succeeded on
# a machine missing any of them and the app then failed to start, which is a far
# worse failure than refusing to install.
#
# `dpkg-shlibdeps` is the tool for this: it resolves each NEEDED soname to the
# package that provides it, with a minimum version, using the local dpkg
# database. It needs a `debian/control` relative to its working directory, hence
# the throwaway shim.
compute_deb_depends() {
  # Keep the old hand-written value as the floor. If the tooling is missing we
  # ship what we shipped before rather than a package with no Depends at all.
  local fallback="libopenblas0"

  command -v dpkg-shlibdeps >/dev/null 2>&1 || {
    echo "WARNING: dpkg-shlibdeps not found (install dpkg-dev)." >&2
    echo "         Falling back to '$fallback' alone, which UNDER-DECLARES the" >&2
    echo "         package: apt will install it and the app will fail to start." >&2
    echo "$fallback"
    return 0
  }

  local shim
  shim="$(mktemp -d)"
  mkdir -p "$shim/debian"
  printf 'Source: neuroskill\n\nPackage: neuroskill\nArchitecture: %s\nDescription: placeholder\n placeholder\n' \
    "$deb_arch" > "$shim/debian/control"

  local out=""
  out="$(
    cd "$shim" && dpkg-shlibdeps -O --ignore-missing-info \
      "$deb_build_root/opt/neuroskill/skill" \
      "$deb_build_root/opt/neuroskill/skill-daemon" \
      "$deb_build_root/opt/neuroskill/skill-tty" 2>/dev/null \
      | sed -n 's/^shlibs:Depends=//p' || true
  )"
  rm -rf "$shim"

  if [[ -n "$out" ]]; then
    echo "$out"
  else
    # Loud on purpose. A silent fallback to the single hand-written dependency is
    # precisely how the .deb shipped for so long missing the entire GTK/WebKit
    # stack: it built, it installed, and only the user saw the failure.
    echo "WARNING: dpkg-shlibdeps produced no dependencies." >&2
    echo "         Are the -dev packages for the linked libraries installed?" >&2
    echo "         Falling back to '$fallback' alone, which UNDER-DECLARES the package." >&2
    echo "$fallback"
  fi
}

deb_depends="$(compute_deb_depends)"
echo "→ deb Depends: $deb_depends"

cat > "$deb_build_root/DEBIAN/control" <<EOF
Package: neuroskill
Version: $version
Section: utils
Priority: optional
Architecture: $deb_arch
Depends: $deb_depends
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
Version:        $rpm_version
Release:        1
Summary:        Neurofeedback and local AI assistant
License:        GPL-3.0-only
BuildArch:      $rpm_arch
# No manual `Requires: openblas` here.
#
# rpmbuild's automatic dependency generator already reads the shipped ELF and
# emits `libopenblas.so.0()(64bit)` -- verified against the released rpm, which
# carried both that and the redundant manual line. The soname is the portable
# form: the package providing it is named differently on each distro this rpm
# targets (openblas-serial / -threads / -openmp on Fedora and RHEL, libopenblas0
# on openSUSE), so pinning the literal name `openblas` adds nothing on Fedora and
# makes the package refuse to install where no such package exists.
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
* $(date '+%a %b %d %Y') NeuroSkill CI <ci@neuroskill.com> - $rpm_version-1
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