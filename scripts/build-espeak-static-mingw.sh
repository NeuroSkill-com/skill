#!/usr/bin/env bash
# build-espeak-static-mingw.sh
#
# Build a self-contained libespeak-ng.a for the Windows/MinGW ABI from any
# host that has the mingw-w64 cross-toolchain, or natively inside MSYS2/MinGW.
#
# ── Output ────────────────────────────────────────────────────────────────────
#   src-tauri/espeak-static-mingw/lib/libespeak-ng.a   (merged, self-contained)
#   src-tauri/espeak-static-mingw/include/espeak-ng/
#   src-tauri/espeak-static-mingw/share/espeak-ng-data/
#
# ── Version control ───────────────────────────────────────────────────────────
# Override the tag with:  ESPEAK_TAG_OVERRIDE=1.51.1 bash build-espeak-static-mingw.sh
#
# ── Requirements ──────────────────────────────────────────────────────────────
#   Linux:  sudo apt install gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 cmake git
#   macOS:  brew install mingw-w64 cmake git
#   MSYS2:  already inside a MinGW environment; cmake and git must be installed
#             pacman -S mingw-w64-x86_64-cmake git
#
# ── Usage ─────────────────────────────────────────────────────────────────────
#   bash scripts/build-espeak-static-mingw.sh
# Called automatically by scripts/tauri-build.js when
#   npm run tauri:build -- --target x86_64-pc-windows-gnu
# is invoked, or by cargo build.rs when the MinGW target is selected.

set -euo pipefail

# ---------- helpers -----------------------------------------------------------
step() { printf '\n\033[1;34m▶ %s\033[0m\n' "$*"; }
die()  { printf '\n\033[1;31mERROR: %s\033[0m\n' "$*" >&2; exit 1; }

# ---------- paths -------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# MinGW output lives in a separate directory from the native Unix build so
# the two archives never overwrite each other.
STATIC_DIR="$REPO_ROOT/src-tauri/espeak-static-mingw"
STATIC_LIB="$STATIC_DIR/lib/libespeak-ng.a"

echo "REPO_ROOT  = $REPO_ROOT"
echo "STATIC_DIR = $STATIC_DIR"
echo "STATIC_LIB = $STATIC_LIB"

# ---------- cache check -------------------------------------------------------
if [[ -f "$STATIC_LIB" ]]; then
    if nm "$STATIC_LIB" 2>/dev/null | grep -q "ucd_isalpha" \
    || ar -t "$STATIC_LIB" 2>/dev/null | grep -qE "ctype.*__c|categories.*__c"; then
        echo "espeak-ng MinGW static library already built (self-contained):"
        echo "  $STATIC_LIB"
        echo "  (delete src-tauri/espeak-static-mingw/ to force a rebuild)"
        exit 0
    else
        echo "Found $STATIC_LIB but it is missing companion symbols — rebuilding."
        rm -rf "$STATIC_DIR"
    fi
fi

# ---------- detect host & cross-compiler prefix -------------------------------
step "Detecting host and MinGW cross-compiler"
HOST_OS="$(uname -s 2>/dev/null || echo "Windows")"
CROSS_PREFIX=""
IS_CROSS=0

case "$HOST_OS" in
    Linux)
        IS_CROSS=1
        if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
            CROSS_PREFIX="x86_64-w64-mingw32-"
        else
            die "MinGW cross-compiler not found on Linux.
Install it with:
  sudo apt install gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64
  # or on Fedora/RHEL:
  sudo dnf install mingw64-gcc mingw64-gcc-c++"
        fi
        ;;
    Darwin)
        IS_CROSS=1
        # Augment PATH so Homebrew binaries are found even in minimal envs.
        export PATH="/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:$PATH"
        if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
            CROSS_PREFIX="x86_64-w64-mingw32-"
        else
            die "MinGW cross-compiler not found on macOS.
Install it with:
  brew install mingw-w64"
        fi
        ;;
    MSYS*|MINGW*|CYGWIN*)
        # Running natively inside an MSYS2/MinGW terminal on Windows.
        # The tools (gcc, g++, ar, …) are already in PATH without a prefix.
        IS_CROSS=0
        CROSS_PREFIX=""
        ;;
    *)
        IS_CROSS=1
        CROSS_PREFIX="x86_64-w64-mingw32-"
        echo "  Unrecognised host OS '$HOST_OS'; assuming cross-compilation with prefix '$CROSS_PREFIX'."
        ;;
esac

echo "  Host OS      : $HOST_OS"
echo "  Cross prefix : '${CROSS_PREFIX}' (IS_CROSS=$IS_CROSS)"
echo "  C compiler   : $(command -v "${CROSS_PREFIX}gcc" || echo 'NOT FOUND')"
echo "  C++ compiler : $(command -v "${CROSS_PREFIX}g++" || echo 'NOT FOUND')"

# ---------- prerequisites -----------------------------------------------------
step "Checking prerequisites"
for tool in cmake git "${CROSS_PREFIX}gcc" "${CROSS_PREFIX}g++" "${CROSS_PREFIX}ar"; do
    if command -v "$tool" >/dev/null 2>&1; then
        echo "  $tool: $(command -v "$tool")"
    else
        die "'$tool' not found in PATH."
    fi
done

# ---------- version -----------------------------------------------------------
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
# When cross-compiling (IS_CROSS=1) we set CMAKE_SYSTEM_NAME=Windows so cmake
# knows the build host and the target host differ.  Inside MSYS2/MinGW we omit
# it — cmake already detects the Windows environment correctly.

CROSS_DEFS=()
if [[ "$IS_CROSS" -eq 1 ]]; then
    CROSS_DEFS=(
        "-DCMAKE_SYSTEM_NAME=Windows"
        "-DCMAKE_C_COMPILER=${CROSS_PREFIX}gcc"
        "-DCMAKE_CXX_COMPILER=${CROSS_PREFIX}g++"
        "-DCMAKE_AR=$(command -v "${CROSS_PREFIX}ar")"
        "-DCMAKE_RANLIB=$(command -v "${CROSS_PREFIX}ranlib")"
        "-DCMAKE_RC_COMPILER=$(command -v "${CROSS_PREFIX}windres" 2>/dev/null || echo "${CROSS_PREFIX}windres")"
        # Prevent cmake from trying to run test executables on the host.
        "-DCMAKE_TRY_COMPILE_TARGET_TYPE=STATIC_LIBRARY"
    )
fi

step "CMake configure (cross=$IS_CROSS, prefix='$CROSS_PREFIX')"
cmake -S "$BUILD_TMP/espeak-ng" \
      -B "$BUILD_TMP/build" \
      -DCMAKE_BUILD_TYPE=Release \
      -DBUILD_SHARED_LIBS=OFF \
      -DUSE_LIBPCAUDIO=OFF \
      -DUSE_ASYNC=OFF \
      -DUSE_MBROLA=OFF \
      -DCMAKE_INSTALL_PREFIX="$STATIC_DIR" \
      "${CROSS_DEFS[@]}" \
    || die "cmake configure failed."

# ---------- cmake build -------------------------------------------------------
if command -v nproc >/dev/null 2>&1; then
    NPROC="$(nproc)"
elif command -v sysctl >/dev/null 2>&1; then
    NPROC="$(sysctl -n hw.logicalcpu 2>/dev/null || echo 4)"
else
    NPROC=4
fi

step "CMake build (parallel: $NPROC)"
cmake --build "$BUILD_TMP/build" --config Release --parallel "$NPROC" \
    || die "cmake build failed."

step "CMake install → $STATIC_DIR"
cmake --install "$BUILD_TMP/build" \
    || die "cmake install failed."

# ---------- merge companion archives -----------------------------------------
# Same logic as build-espeak-static.sh — libucd.a and others must be merged
# into one self-contained libespeak-ng.a.

step "Merging companion static libraries"
AR="${CROSS_PREFIX}ar"
RANLIB="${CROSS_PREFIX}ranlib"

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
        (cd "$out" && "$AR" -x "$lib" 2>/dev/null)
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
    "$AR" -rcs "$STATIC_LIB.new" "${ALL_OBJS[@]}" \
        || die "ar failed while creating merged archive."
    "$RANLIB" "$STATIC_LIB.new"
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

# ---------- verify ------------------------------------------------------------
step "Verifying"

[[ -f "$STATIC_LIB" ]] || die "$STATIC_LIB not found after build."

_has_ucd_symbols() {
    nm "$STATIC_LIB" 2>/dev/null | grep -q "ucd_isalpha" && return 0
    "$AR" -t "$STATIC_LIB" 2>/dev/null | grep -qE "ctype.*__c|categories.*__c" && return 0
    return 1
}

if ! _has_ucd_symbols; then
    echo "ERROR: $STATIC_LIB is missing ucd symbols after merge." >&2
    echo "Archive contents:" >&2
    "$AR" -t "$STATIC_LIB" >&2
    exit 1
fi

echo ""
echo "espeak-ng MinGW static library ready:"
echo "  $STATIC_LIB  ($(du -sh "$STATIC_LIB" | cut -f1))"
echo "  ucd symbols : present ✓"
echo ""
