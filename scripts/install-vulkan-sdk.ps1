# install-vulkan-sdk.ps1
#
# Ensures the LunarG Vulkan SDK is present on Windows before a build that
# uses the `llm-vulkan` feature flag.
#
# Detection order (first hit wins -- the script exits immediately if found):
#   1. VULKAN_SDK environment variable points to a directory that contains
#      Include\vulkan\vulkan.h
#   2. Registry key set by the LunarG installer
#      HKLM:\SOFTWARE\LunarG\Vulkan SDK  ->  InstallPath value
#   3. Default install root  C:\VulkanSDK\<version>\  (newest version picked)
#
# When none of those are found the script:
#   - Downloads the latest Windows installer from sdk.lunarg.com (~200 MB)
#   - Runs it silently  (/S)
#   - Discovers the freshly installed path and exports VULKAN_SDK into the
#     current process so that cargo / CMake pick it up in the same shell.
#
# Output on success (either path):
#   $env:VULKAN_SDK  -- set to the SDK root for the current process
#
# Requirements:
#   PowerShell 5.1+, internet access (first run only), admin rights
#   (the installer writes to C:\VulkanSDK and sets a machine-level env var).
#
# Usage:
#   Called automatically by scripts\tauri-build.js and release-windows.ps1.
#   Can also be run manually from any PowerShell prompt:
#     .\scripts\install-vulkan-sdk.ps1

$ErrorActionPreference = "Stop"

function Step ($msg) { Write-Host "`n>> $msg" -ForegroundColor Blue  }
function Ok   ($msg) { Write-Host "   $msg"   -ForegroundColor Green }
function Warn ($msg) { Write-Host "   $msg"   -ForegroundColor Yellow }
function Die  ($msg) { Write-Host "`nERROR: $msg" -ForegroundColor Red; exit 1 }

# -- Detection helpers ---------------------------------------------------------

# Returns the SDK root path if the directory looks like a valid Vulkan SDK
# (contains Include\vulkan\vulkan.h), otherwise returns $null.
function Test-VulkanRoot ([string]$path) {
    if (-not $path) { return $null }
    $header = Join-Path $path "Include\vulkan\vulkan.h"
    if (Test-Path $header) { return $path }
    return $null
}

# Walk the three detection sources and return the first valid SDK root.
function Find-InstalledVulkanSdk {
    # 1. Environment variable (already set in this session or by the installer
    #    in a previous build on this machine).
    $candidate = Test-VulkanRoot $env:VULKAN_SDK
    if ($candidate) { return $candidate }

    # 2. Registry -- LunarG installer writes InstallPath here.
    foreach ($reg in @(
        "HKLM:\SOFTWARE\LunarG\Vulkan SDK",
        "HKLM:\SOFTWARE\WOW6432Node\LunarG\Vulkan SDK"
    )) {
        if (Test-Path $reg) {
            $props = Get-ItemProperty $reg -ErrorAction SilentlyContinue
            $candidate = Test-VulkanRoot $props.InstallPath
            if ($candidate) { return $candidate }
        }
    }

    # 3. Default install root -- pick the highest version number.
    $vulkanBase = "C:\VulkanSDK"
    if (Test-Path $vulkanBase) {
        $latest = Get-ChildItem $vulkanBase -Directory |
            Sort-Object Name -Descending |
            Select-Object -First 1
        if ($latest) {
            $candidate = Test-VulkanRoot $latest.FullName
            if ($candidate) { return $candidate }
        }
    }

    return $null
}

# -- Main ----------------------------------------------------------------------

Step "Checking for Vulkan SDK"

$sdkRoot = Find-InstalledVulkanSdk
if ($sdkRoot) {
    Ok "Vulkan SDK already installed:"
    Ok "  $sdkRoot"
    Ok "  (delete the directory or unset VULKAN_SDK to force a re-install)"
    # Ensure the env var is set for the remainder of this process so CMake
    # can find it even when the installer put it in a previous session.
    $env:VULKAN_SDK = $sdkRoot
    exit 0
}

# -- Download ------------------------------------------------------------------

Step "Vulkan SDK not found -- downloading latest installer from sdk.lunarg.com"
Warn "(This is a ~200 MB download; it only happens once.)"

$downloadUrl  = "https://sdk.lunarg.com/sdk/download/latest/windows/vulkan-sdk.exe"
$installerPath = Join-Path $env:TEMP "VulkanSDK-installer-$(Get-Random).exe"

Write-Host "  URL:  $downloadUrl"
Write-Host "  Dest: $installerPath"

try {
    # Use Invoke-WebRequest with progress hidden so CI logs aren't flooded.
    $ProgressPreference = "SilentlyContinue"
    Invoke-WebRequest -Uri $downloadUrl -OutFile $installerPath -UseBasicParsing
} catch {
    Die "Download failed: $_`nCheck your internet connection and try again."
}

$sizeMB = [math]::Round((Get-Item $installerPath).Length / 1MB, 1)
Ok "Downloaded $sizeMB MB"

# -- Install -------------------------------------------------------------------

Step "Installing Vulkan SDK silently (/S)"
Warn "(Requires administrator privileges -- UAC may prompt.)"

try {
    $proc = Start-Process -FilePath $installerPath -ArgumentList "/S" -Wait -PassThru
    if ($proc.ExitCode -ne 0) {
        Die "Installer exited with code $($proc.ExitCode).`nTry running the installer manually: $installerPath"
    }
} finally {
    # Clean up the downloaded installer regardless of success.
    Remove-Item $installerPath -Force -ErrorAction SilentlyContinue
}

Ok "Installer finished"

# -- Post-install: refresh env + verify ---------------------------------------
#
# The installer sets VULKAN_SDK as a machine-level environment variable, but
# that only takes effect in NEW processes.  We probe the known install root
# directly so the current shell session (and any child processes like cargo)
# can use it without needing a restart.

Step "Verifying installation"

# Re-read the machine-level env var that the installer just wrote.
$machineVulkanSdk = [System.Environment]::GetEnvironmentVariable("VULKAN_SDK", "Machine")
$sdkRoot = Find-InstalledVulkanSdk

# Prefer the machine env var if it points to a valid SDK; otherwise fall back
# to filesystem discovery (in case the installer used a non-default path).
if ($machineVulkanSdk -and (Test-VulkanRoot $machineVulkanSdk)) {
    $sdkRoot = $machineVulkanSdk
}

if (-not $sdkRoot) {
    Die ("Vulkan SDK installation finished but the SDK root could not be located.`n" +
         "Expected Include\vulkan\vulkan.h under C:\VulkanSDK\<version>\.`n" +
         "Try opening a new terminal; the VULKAN_SDK env var should be set.")
}

# Export into the current process so cargo / CMake see it immediately.
$env:VULKAN_SDK = $sdkRoot

$header = Join-Path $sdkRoot "Include\vulkan\vulkan.h"
Ok "Vulkan SDK installed and verified:"
Ok "  VULKAN_SDK = $sdkRoot"
Ok "  Header:      $header"
