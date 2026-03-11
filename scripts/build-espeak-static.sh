#!/usr/bin/env bash
# build-espeak-static.sh
#
# Build a self-contained libespeak-ng.a (static library) from source and cache
# the result in src-tauri/espeak-static/.
#
# ── Output ────────────────────────────────────────────────────────────────────
#   src-tauri/espeak-static/lib/libespeak-ng.a   (merged, self-contained)
#   src-tauri/espeak-static/include/espeak-ng/
#   src-tauri/espeak-static/share/espeak-ng-data/
#
# ── Version control ───────────────────────────────────────────────────────────
# Override the tag with:  ESPEAK_TAG_OVERRIDE=1.51.1 bash scripts/build-espeak-static.sh
# No network calls are made before the actual git clone.
#
# ── Requirements ──────────────────────────────────────────────────────────────
#   cmake  git  libtool  nm   (all ship with Xcode Command Line Tools)
#
# ── Usage ─────────────────────────────────────────────────────────────────────
#   bash scripts/build-espeak-static.sh
# Called automatically by the cargo build script when libespeak-ng.a is absent.

set -euo pipefail

# ---------- helpers -----------------------------------------------------------
step() { printf '\n\033[1;34m▶ %s\033[0m\n' "$*"; }
die()  { printf '\n\033[1;31mERROR: %s\033[0m\n' "$*" >&2; exit 1; }

# ---------- paths -------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
STATIC_DIR="$REPO_ROOT/src-tauri/espeak-static"
STATIC_LIB="$STATIC_DIR/lib/libespeak-ng.a"

echo "REPO_ROOT  = $REPO_ROOT"
echo "STATIC_DIR = $STATIC_DIR"
echo "STATIC_LIB = $STATIC_LIB"

# ---------- cache check -------------------------------------------------------
if [[ -f "$STATIC_LIB" ]]; then
    # On macOS, verify the cached library is actually a Mach-O archive.
    # A library cross-compiled on Linux (ELF) passes the nm symbol check but
    # fails at link time with "not a mach-o file".  lipo -info prints the
    # architecture(s) for valid Mach-O archives and errors out on ELF ones.
    if [[ "$(uname)" == "Darwin" ]]; then
        if ! lipo -info "$STATIC_LIB" 2>/dev/null | grep -qE "arm64|x86_64|i386"; then
            echo "Found $STATIC_LIB but it is not a macOS Mach-O archive (wrong platform?) — rebuilding."
            rm -rf "$STATIC_DIR"
        fi
    fi
fi

if [[ -f "$STATIC_LIB" ]]; then
    if nm "$STATIC_LIB" 2>/dev/null | grep -q "ucd_isalpha" \
    || ar -t "$STATIC_LIB" 2>/dev/null | grep -qE "ctype.*__c|categories.*__c"; then
        echo "espeak-ng static library already built (self-contained):"
        echo "  $STATIC_LIB"
        echo "  (delete src-tauri/espeak-static/ to force a rebuild)"
        exit 0
    else
        echo "Found $STATIC_LIB but it is missing companion symbols — rebuilding."
        rm -rf "$STATIC_DIR"
    fi
fi

# ---------- prerequisites -----------------------------------------------------
step "Checking prerequisites"
for tool in cmake git nm; do
    if command -v "$tool" >/dev/null 2>&1; then
        echo "  $tool: $(command -v "$tool")"
    else
        die "'$tool' not found in PATH.  Install Xcode Command Line Tools:  xcode-select --install"
    fi
done

# ---------- version -----------------------------------------------------------
# No git ls-remote here — that call has no timeout and hangs silently on slow
# networks.  We hardcode a known-good tag; override via ESPEAK_TAG_OVERRIDE.
ESPEAK_TAG="${ESPEAK_TAG_OVERRIDE:-1.52.0}"
echo ""
echo "espeak-ng version : $ESPEAK_TAG"
echo "(set ESPEAK_TAG_OVERRIDE=<tag> to use a different release)"

# ---------- clone -------------------------------------------------------------
step "Cloning espeak-ng $ESPEAK_TAG"
BUILD_TMP="$(mktemp -d)"
trap 'echo "Cleaning up $BUILD_TMP …"; rm -rf "$BUILD_TMP"' EXIT

git clone --depth=1 --branch "$ESPEAK_TAG" \
    https://github.com/espeak-ng/espeak-ng.git \
    "$BUILD_TMP/espeak-ng" \
    || die "git clone failed — check your internet connection."

echo "Clone complete."

# ---------- cmake configure ---------------------------------------------------
HOST_ARCH="$(uname -m)"
OSX_ARCH="${ESPEAK_ARCHS:-arm64}"

step "CMake configure (arch: $OSX_ARCH)"
cmake -S "$BUILD_TMP/espeak-ng" \
      -B "$BUILD_TMP/build" \
      -DCMAKE_BUILD_TYPE=Release \
      -DBUILD_SHARED_LIBS=OFF \
      -DUSE_LIBPCAUDIO=OFF \
      -DUSE_ASYNC=OFF \
      -DUSE_MBROLA=OFF \
      -DCMAKE_OSX_ARCHITECTURES="$OSX_ARCH" \
      -DCMAKE_INSTALL_PREFIX="$STATIC_DIR" \
    || die "cmake configure failed."

# ---------- cmake build -------------------------------------------------------
NPROC="$(sysctl -n hw.logicalcpu 2>/dev/null || echo 4)"
step "CMake build (parallel: $NPROC)"
cmake --build "$BUILD_TMP/build" --config Release --parallel "$NPROC" \
    || die "cmake build failed."

step "CMake install → $STATIC_DIR"
cmake --install "$BUILD_TMP/build" \
    || die "cmake install failed."

# ---------- merge companion archives -----------------------------------------
#
# cmake builds libucd and libSpeechPlayer as separate archives that it links
# into the shared library but does NOT merge into the static archive.
# We use ar + ranlib to combine everything into one self-contained libespeak-ng.a.

step "Merging companion static libraries"
COMPANIONS=()
while IFS= read -r lib; do
    base="$(basename "$lib")"
    [[ "$base" == "libespeak-ng.a" ]] && continue
    COMPANIONS+=("$lib")
    echo "  found: $base  ($(du -sh "$lib" | cut -f1))"
done < <(find "$BUILD_TMP/build" -name "*.a" 2>/dev/null | sort)

if [[ ${#COMPANIONS[@]} -eq 0 ]]; then
    echo "  (no companion libraries found — libespeak-ng.a is already self-contained)"
else
    echo "Merging ${#COMPANIONS[@]} companion archive(s) into libespeak-ng.a …"

    EXTRACT_DIR="$BUILD_TMP/extract"

    extract_prefixed() {
        local lib="$1" prefix="$2"
        local out="$EXTRACT_DIR/$prefix"
        mkdir -p "$out"
        (cd "$out" && ar -x "$lib" 2>/dev/null)
        for obj in "$out"/*.o; do
            [[ -f "$obj" ]] || continue
            mv "$obj" "${obj%.o}__${prefix}.o"
        done
    }

    extract_prefixed "$STATIC_LIB" "main"
    idx=0
    for lib in "${COMPANIONS[@]}"; do
        extract_prefixed "$lib" "c$idx"
        idx=$((idx + 1))
    done

    ALL_OBJS=()
    while IFS= read -r -d '' obj; do
        ALL_OBJS+=("$obj")
    done < <(find "$EXTRACT_DIR" -name "*.o" -print0 2>/dev/null)
    echo "  packing ${#ALL_OBJS[@]} object files …"
    ar -rcs "$STATIC_LIB.new" "${ALL_OBJS[@]}" \
        || die "ar failed while creating merged archive."
    ranlib "$STATIC_LIB.new"
    mv "$STATIC_LIB.new" "$STATIC_LIB"
    echo "  Merged ✓"
fi

# ---------- copy espeak-ng-data if cmake didn't install it -------------------
if [[ ! -d "$STATIC_DIR/share/espeak-ng-data" ]]; then
    step "Copying espeak-ng-data"
    for candidate in \
        "$BUILD_TMP/espeak-ng/espeak-ng-data" \
        "$BUILD_TMP/build/espeak-ng-data"; do
        if [[ -d "$candidate" ]]; then
            mkdir -p "$STATIC_DIR/share"
            cp -R "$candidate" "$STATIC_DIR/share/espeak-ng-data"
            echo "  espeak-ng-data copied from $candidate"
            break
        fi
    done
fi

# ---------- remove stale dylibs from old builds ------------------------------
OLD_DYLIBS="$REPO_ROOT/src-tauri/dylibs"
if [[ -d "$OLD_DYLIBS" ]]; then
    echo "Removing stale $OLD_DYLIBS"
    rm -rf "$OLD_DYLIBS"
fi

# ---------- verify ------------------------------------------------------------
step "Verifying"

[[ -f "$STATIC_LIB" ]] || die "$STATIC_LIB not found after build."

_has_ucd_symbols() {
    nm "$STATIC_LIB" 2>/dev/null | grep -q "ucd_isalpha" && return 0
    ar -t "$STATIC_LIB" 2>/dev/null | grep -qE "ctype.*__c|categories.*__c" && return 0
    return 1
}

if ! _has_ucd_symbols; then
    echo "ERROR: $STATIC_LIB is missing ucd symbols after merge." >&2
    echo "Archive contents:" >&2
    ar -t "$STATIC_LIB" >&2
    exit 1
fi

echo ""
echo "espeak-ng static library ready:"
echo "  $STATIC_LIB  ($(du -sh "$STATIC_LIB" | cut -f1))"
echo "  ucd symbols : present ✓"
echo ""
