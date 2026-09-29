#!/usr/bin/env bash
# Verify a NeuroSkill .rpm installs and works on a real Fedora, per architecture.
#
#   scripts/verify-rpm-docker.sh                       # both arches, from the latest release
#   scripts/verify-rpm-docker.sh --arch x86_64         # one arch
#   scripts/verify-rpm-docker.sh --rpm path/to.rpm --arch aarch64
#   scripts/verify-rpm-docker.sh --synthetic --arch aarch64
#
# We build .rpm on Ubuntu but ship it to Fedora/RHEL/openSUSE, so the failures
# that matter are distro-side: whether `dnf` can satisfy the dependencies
# rpmbuild generated, and whether the binary's NEEDED libraries exist under those
# package names. Only installing on the target distro answers that.
#
# `--synthetic` builds a throwaway package with a tiny OpenBLAS-linked binary
# using the same spec the release script generates. That covers an architecture
# for which no real .rpm exists yet (today: aarch64, since Release — Linux builds
# x86_64 only) and still exercises version handling, auto-generated dependencies,
# install, and library resolution.
#
# Image: Dockerfile.rpm-verify.  Checks: scripts/lib/verify-rpm.sh (mounted, so
# editing a check needs no rebuild).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
IMAGE_BASE="neuroskill-rpm-verify:fedora41"

arches=()
rpm_path=""
synthetic=0
release_script=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --arch)      arches+=("${2:?--arch needs a value}"); shift 2 ;;
    --rpm)       rpm_path="${2:?--rpm needs a path}"; shift 2 ;;
    --synthetic) synthetic=1; shift ;;
    --use-release-script) release_script=1; synthetic=1; shift ;;
    -h|--help)   sed -n '2,20p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *)           echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

[[ ${#arches[@]} -eq 0 ]] && arches=(x86_64 aarch64)

# Metadata checks need no container, so run them first and always. Only the
# install phase (dnf resolving dependencies on a real Fedora) needs Docker, and
# that is exactly the part that is unavailable on a macOS dev box or when the
# daemon is wedged. Reporting "metadata verified, install not attempted" is more
# useful than refusing to do anything.
# A *bounded* probe. `docker info` does not time out on its own: when the daemon
# is wedged (a stuck container, a VM that lost its way) it blocks forever, and an
# unbounded guard would hang the very script whose job is to degrade gracefully.
# macOS has no coreutils `timeout`, so poll a background probe against a deadline.
docker_responsive() {
  local deadline=${1:-20} probe_out
  probe_out="$(mktemp)"
  ( docker info >"$probe_out" 2>&1; echo "$?" >>"$probe_out.rc" ) &
  local probe_pid=$!
  local waited=0
  while [[ $waited -lt $deadline ]]; do
    if ! kill -0 "$probe_pid" 2>/dev/null; then
      local rc
      rc="$(cat "$probe_out.rc" 2>/dev/null || echo 1)"
      rm -f "$probe_out" "$probe_out.rc"
      return "${rc:-1}"
    fi
    sleep 1
    waited=$((waited + 1))
  done
  # Still blocked past the deadline: treat as unavailable and stop waiting on it.
  # `disown` before the kill so bash does not print its own "Terminated" notice
  # for a job we are deliberately abandoning.
  disown "$probe_pid" 2>/dev/null || true
  kill -TERM "$probe_pid" 2>/dev/null || true
  rm -f "$probe_out" "$probe_out.rc"
  return 1
}

docker_ok=1
if ! docker_responsive "${DOCKER_PROBE_SECONDS:-20}"; then
  docker_ok=0
  echo "note: docker did not respond within ${DOCKER_PROBE_SECONDS:-20}s —"
  echo "      metadata will be verified natively; the install-on-Fedora phase is skipped."
fi

platform_for() {
  case "$1" in
    x86_64)  echo "linux/amd64" ;;
    aarch64) echo "linux/arm64" ;;
    *)       echo "unsupported arch: $1" >&2; return 1 ;;
  esac
}

# The image must be built for the SAME platform it will be run on. Building once
# for the host arch and then running with `--platform linux/amd64` makes docker
# look for an amd64 image it does not have, and it tries to pull our local-only
# tag from a registry. So: one tagged build per architecture.
build_image_for() {
  local platform="$1" tag="$2"
  docker build -q --platform "$platform" \
    -f "$ROOT_DIR/Dockerfile.rpm-verify" -t "$tag" "$ROOT_DIR" >/dev/null
}

# Find a local .rpm for an arch: an explicit --rpm, else whatever the release
# packaging script produced under src-tauri/target.
find_local_rpm() {
  local arch="$1" triple
  case "$arch" in
    x86_64)  triple="x86_64-unknown-linux-gnu" ;;
    aarch64) triple="aarch64-unknown-linux-gnu" ;;
  esac
  find "$ROOT_DIR/src-tauri/target/$triple/release/bundle/rpm" -maxdepth 1 -name '*.rpm' 2>/dev/null | head -1
}

# Pull the published .rpm for an arch from the newest GitHub release, if one
# exists. Release — Linux is x86_64-only today, so aarch64 finds nothing.
fetch_release_rpm() {
  local arch="$1" dest="$2"
  command -v gh >/dev/null 2>&1 || return 1
  local tag
  tag="$(gh release list --limit 1 --json tagName -q '.[0].tagName' 2>/dev/null)" || return 1
  [[ -n "$tag" ]] || return 1
  mkdir -p "$dest"
  gh release download "$tag" -p "*.$arch.rpm" -D "$dest" --clobber >/dev/null 2>&1 || return 1
  find "$dest" -maxdepth 1 -name "*.$arch.rpm" | head -1
}

# Build a throwaway package from the same spec shape the release script emits.
# Runs inside the container so rpmbuild's arch matches the target.
build_synthetic() {
  local arch="$1" platform="$2" out="$3" image="$4"
  mkdir -p "$out"
  docker run --rm --platform "$platform" \
    -v "$out:/out" \
    -v "$ROOT_DIR/VERSION:/work/VERSION:ro" \
    "$image" -c '
      set -euo pipefail
      dnf -y install gcc openblas-devel >/dev/null 2>&1
      version="$(tr -d "[:space:]" < /work/VERSION)"
      # Same transformation as scripts/package-linux-system-bundles.sh. The
      # tilde comes from a variable because bash tilde-expands a literal one in
      # a substitution replacement.
      rpm_tilde="~"
      rpm_version="${version//-/$rpm_tilde}"
      arch="$(rpm --eval %_arch)"

      top=/tmp/rb
      mkdir -p "$top"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
      stage=/tmp/neuroskill-root
      mkdir -p "$stage/opt/neuroskill" "$stage/usr/bin" \
               "$stage/usr/share/applications" "$stage/usr/share/pixmaps"

      # A real ELF linked against OpenBLAS, so rpmbuild has something to derive
      # the soname dependency from and ldd has something to resolve.
      printf "%s\n" \
        "extern double cblas_ddot(int,const double*,int,const double*,int);" \
        "int main(void){ double a=1,b=2; return (int)cblas_ddot(1,&a,1,&b,1); }" > /tmp/t.c
      # Mirror the real package layout: the ELFs live in /opt/neuroskill and
      # /usr/bin/neuroskill is a bash launcher that execs into them. A fixture
      # that puts the binary straight in /usr/bin would not exercise the same
      # checks as the shipped package.
      gcc /tmp/t.c -o "$stage/opt/neuroskill/skill" -lopenblas
      cp "$stage/opt/neuroskill/skill" "$stage/opt/neuroskill/skill-daemon"
      cp "$stage/opt/neuroskill/skill" "$stage/opt/neuroskill/skill-tty"
      cat > "$stage/usr/bin/neuroskill" <<'"'"'LAUNCH'"'"'
#!/usr/bin/env bash
set -euo pipefail
APP_DIR="/opt/neuroskill"
exec "$APP_DIR/skill" "$@"
LAUNCH
      chmod +x "$stage/usr/bin/neuroskill"
      printf "[Desktop Entry]\nName=NeuroSkill\nExec=/usr/bin/neuroskill\nType=Application\n" \
        > "$stage/usr/share/applications/neuroskill.desktop"
      : > "$stage/usr/share/pixmaps/neuroskill.png"
      tar -czf "$top/SOURCES/neuroskill-root.tar.gz" -C /tmp neuroskill-root

      cat > "$top/SPECS/neuroskill.spec" <<SPEC
Name:           neuroskill
Version:        $rpm_version
Release:        1
Summary:        Neurofeedback and local AI assistant
License:        GPL-3.0-only
BuildArch:      $arch
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
* $(date "+%a %b %d %Y") NeuroSkill CI <ci@neuroskill.com> - $rpm_version-1
- synthetic package for scripts/verify-rpm-docker.sh
SPEC

      # debuginfo extraction has nothing to work with for a one-file test binary
      # and fails the build on an empty debugsourcefiles.list.
      rpmbuild -bb "$top/SPECS/neuroskill.spec" \
        --define "_topdir $top" --define "debug_package %{nil}" >/dev/null
      cp "$(find "$top/RPMS" -name "*.rpm" | head -1)" /out/
    '
  find "$out" -maxdepth 1 -name '*.rpm' | head -1
}

# Build via the ACTUAL release packaging script, inside the container.
#
# `build_synthetic` reproduces the spec by hand, which proves the spec *shape* is
# sound but not that scripts/package-linux-system-bundles.sh emits it — the two
# could drift. This mode runs the real script with a stand-in binary, so what
# gets verified is the artifact the release pipeline would actually produce.
build_with_release_script() {
  local arch="$1" platform="$2" out="$3" image="$4" triple
  case "$arch" in
    x86_64)  triple="x86_64-unknown-linux-gnu" ;;
    aarch64) triple="aarch64-unknown-linux-gnu" ;;
  esac
  mkdir -p "$out"
  docker run --rm --platform "$platform" \
    -v "$ROOT_DIR:/src:ro" \
    -v "$out:/out" \
    -e "TRIPLE=$triple" \
    "$image" -c '
      set -euo pipefail
      # dpkg-deb is a Debian tool but packaged for Fedora; the script builds both
      # a .deb and a .rpm and refuses to start without it.
      dnf -y install dpkg gcc openblas-devel tar gzip patchelf >/dev/null 2>&1

      # The repo is mounted read-only (never write into the developer tree), so
      # work from a copy of just what the script reads.
      mkdir -p /build
      # Everything the script reads from the repo root: scripts/, VERSION, the
      # licence and the Linux readme it bundles into /opt/neuroskill.
      cp -R /src/scripts /build/scripts
      cp /src/VERSION /build/VERSION
      cp /src/LICENSE /build/LICENSE
      mkdir -p /build/docs && cp /src/docs/LINUX.md /build/docs/LINUX.md
      mkdir -p "/build/src-tauri/target/$TRIPLE/release" /build/src-tauri/resources
      cp -R /src/src-tauri/resources/. /build/src-tauri/resources/ 2>/dev/null || true
      mkdir -p /build/src-tauri/resources/neutts-samples
      # Desktop icon the script installs to /usr/share/pixmaps.
      mkdir -p /build/src-tauri/icons
      cp -R /src/src-tauri/icons/. /build/src-tauri/icons/ 2>/dev/null || true

      # Stand-in for the app binary: a real ELF linked against OpenBLAS, so
      # rpmbuild derives the same soname dependency it would from the real one.
      printf "%s\n" \
        "extern double cblas_ddot(int,const double*,int,const double*,int);" \
        "int main(void){ double a=1,b=2; return (int)cblas_ddot(1,&a,1,&b,1); }" > /tmp/t.c
      # The script bundles three sidecars and refuses to continue without
      # any of them: the app, skill-daemon, and skill-tty.
      rel="/build/src-tauri/target/$TRIPLE/release"
      gcc /tmp/t.c -o "$rel/skill" -lopenblas
      cp "$rel/skill" "$rel/skill-daemon"
      cp "$rel/skill" "$rel/skill-tty"

      # Fedora enables debuginfo extraction by default; the Ubuntu runner where
      # the release actually builds does not. Our stand-in binaries are tiny gcc
      # programs with no separable sources, so extraction fails on an empty
      # debugsourcefiles.list. Disable it with a macro rather than touching the
      # release script, which is the thing under test.
      # (No apostrophes in this block: it lives inside a single-quoted string.)
      printf "%%debug_package %%{nil}\n" > /root/.rpmmacros

      cd /build
      bash scripts/package-linux-system-bundles.sh --target "$TRIPLE" --skip-build >/tmp/pkg.log 2>&1 || {
        echo "release packaging script FAILED:" >&2; tail -30 /tmp/pkg.log >&2; exit 1; }

      cp "$(find "/build/src-tauri/target/$TRIPLE/release/bundle/rpm" -name "*.rpm" | head -1)" /out/
    ' >/dev/null
  find "$out" -maxdepth 1 -name '*.rpm' | head -1
}

overall=0

for arch in "${arches[@]}"; do
  platform="$(platform_for "$arch")"
  echo
  echo "════════════════════════════════════════════════════════════════"
  echo "  $arch  ($platform)"
  echo "════════════════════════════════════════════════════════════════"

  work="$ROOT_DIR/dist/rpm-verify/$arch"
  image="$IMAGE_BASE-$arch"
  pkg=""
  origin=""

  if [[ "$docker_ok" -eq 1 ]]; then
    echo "building $image for $platform"
    if ! build_image_for "$platform" "$image"; then
      echo "  could not build the verify image for $platform" >&2
      overall=1
      continue
    fi
  fi

  if [[ -n "$rpm_path" ]]; then
    pkg="$(cd "$(dirname "$rpm_path")" && pwd)/$(basename "$rpm_path")"
    origin="--rpm"
  elif [[ "$synthetic" -eq 0 ]]; then
    pkg="$(find_local_rpm "$arch" || true)"
    [[ -n "$pkg" ]] && origin="local build"
    if [[ -z "$pkg" ]]; then
      pkg="$(fetch_release_rpm "$arch" "$work/download" || true)"
      [[ -n "$pkg" ]] && origin="published release"
    fi
  fi

  if [[ -z "$pkg" && "$release_script" -eq 1 && "$docker_ok" -eq 1 ]]; then
    echo "building via scripts/package-linux-system-bundles.sh (stand-in binary)"
    pkg="$(build_with_release_script "$arch" "$platform" "$work/release-script" "$image" || true)"
    origin="release packaging script"
  elif [[ -z "$pkg" && "$docker_ok" -eq 1 ]]; then
    echo "no real .rpm for $arch — building a synthetic one from the release spec"
    pkg="$(build_synthetic "$arch" "$platform" "$work/synthetic" "$image" || true)"
    origin="synthetic"
  elif [[ -z "$pkg" ]]; then
    echo "  no .rpm for $arch and docker is unavailable to build a synthetic one" >&2
    overall=1
    continue
  fi

  if [[ -z "$pkg" || ! -f "$pkg" ]]; then
    echo "  could not obtain an rpm for $arch" >&2
    overall=1
    continue
  fi

  echo "package : $(basename "$pkg")"
  echo "origin  : $origin"
  echo

  # ── Phase 1: metadata, no container required ──────────────────────────
  echo "-- metadata (native) --"
  if python3 "$ROOT_DIR/scripts/lib/rpm_meta.py" "$pkg" --check "$arch"; then
    meta_ok=1
  else
    meta_ok=0
    overall=1
  fi

  # ── Phase 2: install on a real Fedora ─────────────────────────────────
  if [[ "$docker_ok" -eq 0 ]]; then
    echo
    echo "-- install (skipped: docker unavailable) --"
    [[ "$meta_ok" -eq 1 ]] && echo "  => $arch metadata OK, install unverified"
    continue
  fi

  echo
  echo "-- install on Fedora ($platform) --"
  if docker run --rm --platform "$platform" \
      -v "$(dirname "$pkg"):/work/pkg:ro" \
      -v "$ROOT_DIR/scripts/lib/verify-rpm.sh:/work/verify-rpm.sh:ro" \
      "$image" /work/verify-rpm.sh "/work/pkg/$(basename "$pkg")" "$arch"; then
    echo "  => $arch OK"
  else
    echo "  => $arch FAILED" >&2
    overall=1
  fi
done

echo
[[ "$overall" -eq 0 ]] && echo "All requested architectures verified." || echo "One or more architectures failed." >&2
exit "$overall"
