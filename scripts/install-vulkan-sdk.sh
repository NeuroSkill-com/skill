#!/usr/bin/env bash
# install-vulkan-sdk.sh
#
# Ensures the LunarG Vulkan SDK development packages are present on Linux
# before a build that uses the `llm-vulkan` feature flag.
#
# Detection order (first hit wins -- the script exits immediately if found):
#   1. VULKAN_SDK environment variable points to a directory that contains
#      include/vulkan/vulkan.h
#   2. Vulkan headers found at the standard system path /usr/include/vulkan/vulkan.h
#
# When neither is found the script installs via:
#   Ubuntu noble / jammy / focal  →  LunarG apt repository  (vulkan-sdk meta-package)
#   Other Debian/Ubuntu            →  system apt packages     (libvulkan-dev + glslang-tools)
#   Fedora / RHEL / CentOS         →  dnf
#   Arch / Manjaro                 →  pacman
#   Alpine                         →  apk  (vulkan-loader-dev + shaderc + spirv-tools)
#
# On success:
#   Vulkan headers + loader + glslc shader compiler are available to CMake.
#   On GitHub Actions the PATH addition and any shell-env changes are propagated
#   to subsequent steps automatically because packages install to system paths;
#   no VULKAN_SDK variable needs to be set (cmake's FindVulkan uses pkg-config).
#
# Requirements:
#   bash 4+, sudo, one of: apt-get / dnf / pacman, internet access (first run)
#
# Usage:
#   Called automatically by scripts/tauri-build.js on Linux dev + build.
#   Can also be run manually:
#     bash scripts/install-vulkan-sdk.sh

set -euo pipefail

# Coloured output helpers (no-op when not a tty so CI logs stay clean)
if [ -t 1 ]; then
  _BLUE='\033[0;34m'; _GREEN='\033[0;32m'; _YELLOW='\033[1;33m'; _RED='\033[0;31m'; _NC='\033[0m'
else
  _BLUE=''; _GREEN=''; _YELLOW=''; _RED=''; _NC=''
fi

step() { echo -e "\n${_BLUE}>> $*${_NC}"; }
ok()   { echo -e "${_GREEN}   $*${_NC}"; }
warn() { echo -e "${_YELLOW}   $*${_NC}"; }
die()  { echo -e "\n${_RED}ERROR: $*${_NC}" >&2; exit 1; }

# -- Detection helpers ---------------------------------------------------------

# Returns 0 (true) if the given directory looks like a valid Vulkan SDK root
# (contains include/vulkan/vulkan.h).
test_vulkan_root() {
    local path="${1:-}"
    [[ -n "$path" && -f "$path/include/vulkan/vulkan.h" ]]
}

# Walk detection sources and print the first valid SDK root (or nothing).
find_installed_vulkan_sdk() {
    # 1. Env var already set in this session.
    if test_vulkan_root "${VULKAN_SDK:-}"; then
        echo "$VULKAN_SDK"
        return
    fi

    # 2. Standard system install (libvulkan-dev / vulkan-sdk via apt).
    #    Headers land in /usr/include/vulkan/ which is under the /usr prefix.
    if [[ -f /usr/include/vulkan/vulkan.h ]]; then
        echo "/usr"
        return
    fi

    # 3. LunarG tarball SDK conventional location.
    if [[ -f /opt/VulkanSDK/include/vulkan/vulkan.h ]]; then
        echo "/opt/VulkanSDK"
        return
    fi
}

# -- Main ----------------------------------------------------------------------

step "Checking for Vulkan SDK"

sdk_root=$(find_installed_vulkan_sdk || true)
if [[ -n "$sdk_root" ]]; then
    ok "Vulkan SDK already installed:"
    ok "  $sdk_root"
    ok "  (remove /usr/include/vulkan or unset VULKAN_SDK to force a re-install)"
    exit 0
fi

# -- Detect distro -------------------------------------------------------------

step "Vulkan SDK not found -- installing"

DISTRO_ID="unknown"
CODENAME=""
if [[ -f /etc/os-release ]]; then
    # shellcheck source=/dev/null
    source /etc/os-release
    DISTRO_ID="${ID:-unknown}"
    CODENAME="${VERSION_CODENAME:-}"
fi

warn "Detected distro: ${DISTRO_ID} ${CODENAME}"

# -- Install -------------------------------------------------------------------

case "$DISTRO_ID" in

  ubuntu|debian)
    lunarg_ok=false

    # Prefer the LunarG apt repo for supported Ubuntu codenames -- it ships
    # a newer SDK than the Ubuntu universe archive and includes glslc (shaderc),
    # spirv-tools, and validation layers in one meta-package.
    if [[ "$DISTRO_ID" == "ubuntu" ]]; then
        case "$CODENAME" in
          noble|jammy|focal)
            step "Adding LunarG apt repository for Ubuntu ${CODENAME}"

            # Signing key
            curl -fsSL https://packages.lunarg.com/lunarg-signing-key-pub.asc \
                | sudo tee /etc/apt/trusted.gpg.d/lunarg.asc > /dev/null

            # Repo list
            sudo curl -fsSLo \
                "/etc/apt/sources.list.d/lunarg-vulkan-${CODENAME}.list" \
                "https://packages.lunarg.com/vulkan/lunarg-vulkan-${CODENAME}.list"

            sudo apt-get update -y -q

            # vulkan-sdk is the LunarG meta-package: headers, loader, glslc,
            # spirv-tools, validation layers, and shader toolchain.
            if sudo apt-get install -y vulkan-sdk 2>/dev/null; then
                lunarg_ok=true
                ok "LunarG vulkan-sdk installed"
            else
                warn "vulkan-sdk not found after adding LunarG repo; falling back to system packages"
            fi
            ;;
          *)
            warn "Ubuntu codename '${CODENAME}' not in LunarG repo -- using system packages"
            ;;
        esac
    fi

    if [[ "$lunarg_ok" == false ]]; then
        step "Installing system Vulkan packages via apt"
        sudo apt-get update -y -q
        # libvulkan-dev   : Vulkan headers + loader .so (required by cmake FindVulkan)
        # glslang-tools   : glslangValidator -- shader compiler used by llama.cpp
        # shaderc         : glslc            -- preferred shader compiler (llama.cpp)
        # spirv-tools     : spirv-val / spirv-opt (shader validation at build time)
        # vulkan-validationlayers-dev : runtime validation (optional but useful locally)
        sudo apt-get install -y \
            libvulkan-dev                \
            libvulkan1                   \
            glslang-tools                \
            shaderc                      \
            spirv-tools                  \
            vulkan-validationlayers-dev
        ok "System Vulkan packages installed"
    fi
    ;;

  fedora|rhel|centos|almalinux|rocky)
    step "Installing Vulkan development packages via dnf"
    sudo dnf install -y \
        vulkan-loader-devel     \
        vulkan-headers          \
        glslang                 \
        shaderc                 \
        spirv-tools             \
        vulkan-validation-layers-devel
    ok "dnf Vulkan packages installed"
    ;;

  arch|manjaro)
    step "Installing Vulkan development packages via pacman"
    sudo pacman -S --noconfirm \
        vulkan-devel    \
        glslang         \
        shaderc         \
        spirv-tools
    ok "pacman Vulkan packages installed"
    ;;

  alpine)
    step "Installing Vulkan development packages via apk"
    # vulkan-loader-dev : Vulkan headers + loader (cmake FindVulkan)
    # vulkan-headers    : standalone Khronos headers (provides vulkan/vulkan.h)
    # glslang           : glslangValidator shader compiler (llama.cpp fallback)
    # shaderc           : glslc shader compiler (preferred by llama.cpp)
    # spirv-tools       : spirv-val / spirv-opt
    # vulkan-validation-layers : runtime validation (optional, useful for dev)
    apk add --no-cache \
        vulkan-loader-dev        \
        vulkan-headers           \
        glslang                  \
        shaderc                  \
        spirv-tools              \
        vulkan-validation-layers
    ok "apk Vulkan packages installed"
    ;;

  *)
    die "Unsupported distro '${DISTRO_ID}'. Please install libvulkan-dev and glslc manually, then re-run."
    ;;
esac

# -- Verify installation -------------------------------------------------------

step "Verifying installation"

sdk_root=$(find_installed_vulkan_sdk || true)
if [[ -z "$sdk_root" ]]; then
    die "Vulkan headers not found after installation.
Expected: /usr/include/vulkan/vulkan.h
Try opening a new shell; package metadata may need a moment to settle."
fi

ok "Vulkan SDK installed and verified:"
ok "  Root:   $sdk_root"
ok "  Header: $sdk_root/include/vulkan/vulkan.h"

# Verify a GLSL shader compiler is in PATH -- llama.cpp's cmake requires one.
if command -v glslc &>/dev/null; then
    ok "  Shader compiler: glslc  ($(command -v glslc))"
elif command -v glslangValidator &>/dev/null; then
    ok "  Shader compiler: glslangValidator  ($(command -v glslangValidator))"
else
    warn "No GLSL shader compiler found (glslc / glslangValidator)."
    warn "llama.cpp Vulkan shaders will fail to compile."
    warn "Install 'shaderc' or 'glslang-tools' and re-run."
    exit 1
fi

# cmake's FindVulkan module on Linux uses pkg-config to locate the loader and
# standard system include paths -- it does NOT require VULKAN_SDK to be set
# when headers are in /usr/include.  No env-var export is needed here.
ok "cmake FindVulkan will discover headers and loader via pkg-config automatically."
