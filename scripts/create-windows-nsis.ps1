<#
.SYNOPSIS
    Create a Windows NSIS installer from a pre-built release binary.

.DESCRIPTION
    Assembles the application directory structure, generates an NSIS installer
    with background/banner images (app icon + version), and optionally signs
    the installer with a code-signing certificate.

    This script bypasses the Tauri CLI bundler for cases where it fails or
    when a standalone packaging step is preferred.

    Prerequisites:
      - NSIS installed (makensis on PATH, or set $env:NSIS_DIR)
        Install: choco install nsis  -or-  winget install NSIS.NSIS
      - Python 3 with Pillow (for installer images)
        Install: pip install Pillow
      - The release binary must be pre-built

.PARAMETER Target
    Rust target triple (default: x86_64-pc-windows-msvc)

.PARAMETER Sign
    When set, sign the installer with signtool.exe using $env:CERTIFICATE_THUMBPRINT

.EXAMPLE
    .\scripts\create-windows-nsis.ps1
    .\scripts\create-windows-nsis.ps1 -Sign
    $env:CERTIFICATE_THUMBPRINT = "ABC123..." ; .\scripts\create-windows-nsis.ps1 -Sign
#>

param(
    [string]$Target = "x86_64-pc-windows-msvc",
    [switch]$Sign
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Ensure consistent UTF-8 handling for trademark and other non-ASCII UI text.
try {
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [Console]::InputEncoding = $utf8NoBom
    [Console]::OutputEncoding = $utf8NoBom
    $OutputEncoding = $utf8NoBom
    chcp 65001 | Out-Null
} catch {
    # best-effort only; keep script functional in restricted hosts
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Root = Split-Path -Parent $ScriptDir
$TauriDir = Join-Path $Root "src-tauri"
$Conf = Get-Content (Join-Path $TauriDir "tauri.conf.json") -Raw | ConvertFrom-Json

$ProductName = $Conf.productName
$ProductDisplayName = if ($ProductName.EndsWith("™")) { $ProductName } else { "$ProductName™" }
$Version = $Conf.version
$Identifier = $Conf.identifier
$BinaryName = "skill.exe"
$TargetReleaseDir = Join-Path $TauriDir "target/$Target/release"
$HostReleaseDir = Join-Path $TauriDir "target/release"
$TargetBinary = Join-Path -Path $TargetReleaseDir -ChildPath $BinaryName
$HostBinary = Join-Path -Path $HostReleaseDir -ChildPath $BinaryName
$BinaryCandidates = @(
    $TargetBinary,
    $HostBinary
)

$Binary = $null
$ReleaseDir = $null
foreach ($candidate in $BinaryCandidates) {
    if (Test-Path $candidate) {
        $Binary = $candidate
        $ReleaseDir = Split-Path -Parent $candidate
        break
    }
}

if (-not $Binary) {
    Write-Error @"
Release binary not found.
Checked:
  - $($BinaryCandidates[0])
  - $($BinaryCandidates[1])

Run first:  npx tauri build --target $Target --no-bundle
  -or-  npx tauri build --no-bundle
  -or-  cargo build --release --target $Target --features custom-protocol
"@
    exit 1
}

if ($Binary -eq $HostBinary) {
    Write-Warning "Using host release binary layout at target/release/$BinaryName (no explicit Rust target output path found)."
}

Write-Host "-> Creating NSIS installer for $ProductName v$Version ($Target)"

# ── Locate NSIS ─────────────────────────────────────────────────────────────
$NsisDir = $env:NSIS_DIR
if ($NsisDir -and (Test-Path $NsisDir) -and -not (Test-Path (Join-Path $NsisDir "makensis.exe"))) {
    $item = Get-Item $NsisDir -ErrorAction SilentlyContinue
    if ($item -and -not $item.PSIsContainer -and $item.Name -ieq "makensis.exe") {
        $NsisDir = Split-Path -Parent $item.FullName
    }
}

if (-not $NsisDir -or -not (Test-Path (Join-Path $NsisDir "makensis.exe"))) {
    $makensis = Get-Command makensis -ErrorAction SilentlyContinue
    if ($makensis) {
        $NsisDir = Split-Path -Parent $makensis.Source
    } else {
        # Common install locations
        foreach ($path in @(
            "C:\Program Files (x86)\NSIS",
            "C:\Program Files\NSIS",
            "$env:LOCALAPPDATA\NSIS"
        )) {
            if (Test-Path (Join-Path $path "makensis.exe")) {
                $NsisDir = $path
                break
            }
        }
    }
}
if (-not $NsisDir -or -not (Test-Path (Join-Path $NsisDir "makensis.exe"))) {
    Write-Error @"
NSIS not found. Install it:
  choco install nsis
  -or-  winget install NSIS.NSIS
  -or-  set `$env:NSIS_DIR to the NSIS install directory
"@
    exit 1
}
$MakeNsis = Join-Path $NsisDir "makensis.exe"
Write-Host "  NSIS: $MakeNsis"

# ── Prepare staging directory ───────────────────────────────────────────────
$BundleDir = Join-Path $ReleaseDir "bundle"
$NsisOutDir = Join-Path $BundleDir "nsis"
$Staging = Join-Path $env:TEMP "neuroskill-nsis-staging-$(Get-Random)"

New-Item -ItemType Directory -Force -Path $Staging | Out-Null
New-Item -ItemType Directory -Force -Path $NsisOutDir | Out-Null

try {
    # Binary
    Copy-Item $Binary (Join-Path $Staging $BinaryName)
    Write-Host "  [ok] $BinaryName"

    # Icon
    $IconIco = Join-Path $TauriDir "icons/icon.ico"
    if (Test-Path $IconIco) {
        Copy-Item $IconIco (Join-Path $Staging "icon.ico")
        Write-Host "  [ok] icon.ico"
    }

    # Resources (espeak-ng-data, neutts-samples)
    $resources = $Conf.bundle.resources
    if ($resources) {
        foreach ($prop in $resources.PSObject.Properties) {
            $srcRel = $prop.Name
            $dstRel = $prop.Value
            $src = Join-Path $TauriDir $srcRel
            $dst = Join-Path $Staging $dstRel
            if (Test-Path $src) {
                if ((Get-Item $src).PSIsContainer) {
                    Copy-Item $src $dst -Recurse -Force
                } else {
                    New-Item -ItemType Directory -Force -Path (Split-Path $dst) | Out-Null
                    Copy-Item $src $dst -Force
                }
                Write-Host "  [ok] $dstRel"
            } else {
                Write-Warning "  Missing resource: $srcRel"
            }
        }
    }

    # Docs
    foreach ($doc in @("README.md", "CHANGELOG.md", "LICENSE")) {
        $docPath = Join-Path $Root $doc
        if (Test-Path $docPath) {
            Copy-Item $docPath (Join-Path $Staging $doc)
            Write-Host "  [ok] $doc"
        }
    }

    # ── Generate installer images ───────────────────────────────────────────
    # NSIS uses two bitmap images:
    #   - Header image (150×57) — top-right of installer pages
    #   - Welcome/Finish image (164×314) — left panel of welcome/finish pages
    $HeaderBmp = Join-Path $Staging "header.bmp"
    $WelcomeBmp = Join-Path $Staging "welcome.bmp"
    $IconPng = Join-Path $TauriDir "icons/icon.png"

    $imagesGenerated = $false
    if (Test-Path $IconPng) {
        try {
            python3 -c @"
import sys
from PIL import Image, ImageDraw, ImageFont

icon_path = sys.argv[1]
version = sys.argv[2]
header_out = sys.argv[3]
welcome_out = sys.argv[4]

icon = Image.open(icon_path).convert('RGBA')

# ── Header image (150x57) ──────────────────────────────────────────
header = Image.new('RGB', (150, 57), (30, 30, 30))
hi = icon.resize((45, 45), Image.LANCZOS)
header.paste(hi, (8, 6), hi)
draw_h = ImageDraw.Draw(header)

font_h = None
for fp in [
    'C:/Windows/Fonts/segoeui.ttf',
    'C:/Windows/Fonts/arial.ttf',
]:
    try:
        font_h = ImageFont.truetype(fp, 14)
        break
    except (OSError, IOError):
        continue
if font_h is None:
    font_h = ImageFont.load_default()

draw_h.text((60, 12), f'v{version}', fill=(200, 200, 200), font=font_h)
header.save(header_out, 'BMP')

# ── Welcome/Finish image (164x314) ─────────────────────────────────
welcome = Image.new('RGB', (164, 314), (30, 30, 30))
wi = icon.resize((100, 100), Image.LANCZOS)
welcome.paste(wi, (32, 60), wi)
draw_w = ImageDraw.Draw(welcome)

font_w = None
font_ws = None
for fp in [
    'C:/Windows/Fonts/segoeui.ttf',
    'C:/Windows/Fonts/arial.ttf',
]:
    try:
        font_w = ImageFont.truetype(fp, 16)
        font_ws = ImageFont.truetype(fp, 12)
        break
    except (OSError, IOError):
        continue
if font_w is None:
    font_w = ImageFont.load_default()
    font_ws = font_w

vtxt = f'v{version}'
vbox = draw_w.textbbox((0, 0), vtxt, font=font_w)
vw = vbox[2] - vbox[0]
draw_w.text(((164 - vw) // 2, 170), vtxt, fill=(200, 200, 200), font=font_w)

welcome.save(welcome_out, 'BMP')
print('  [ok] installer images generated')
"@ -- $IconPng $Version $HeaderBmp $WelcomeBmp 2>&1
            if ($LASTEXITCODE -eq 0) {
                $imagesGenerated = $true
            }
        } catch {
            Write-Host "  [skip] Could not generate installer images (Pillow not available?)"
        }
    }

    # ── Write NSIS script ───────────────────────────────────────────────────
    $InstallerExe = "NeuroSkill_${Version}_x64-setup.exe"
    $InstallerPath = Join-Path $NsisOutDir $InstallerExe
    $NsiScript = Join-Path $Staging "installer.nsi"

    # Build the file/directory install commands and uninstall commands
    $installFiles = @('  SetOutPath "$INSTDIR"')
    $installFiles += '  File "skill.exe"'
    if (Test-Path (Join-Path $Staging "icon.ico")) {
        $installFiles += '  File "icon.ico"'
    }
    foreach ($doc in @("README.md", "CHANGELOG.md", "LICENSE")) {
        if (Test-Path (Join-Path $Staging $doc)) {
            $installFiles += "  File `"$doc`""
        }
    }

    $uninstallFiles = @()
    $uninstallFiles += '  Delete "$INSTDIR\skill.exe"'
    $uninstallFiles += '  Delete "$INSTDIR\icon.ico"'
    foreach ($doc in @("README.md", "CHANGELOG.md", "LICENSE")) {
        $uninstallFiles += "  Delete `"`$INSTDIR\$doc`""
    }

    # Resource directories
    $resourceDirs = @()
    if ($resources) {
        foreach ($prop in $resources.PSObject.Properties) {
            $dstRel = $prop.Value
            $srcStaging = Join-Path $Staging $dstRel
            if (Test-Path $srcStaging) {
                $installFiles += "  SetOutPath `"`$INSTDIR\$dstRel`""
                $installFiles += "  File /r `"$dstRel\*.*`""
                $resourceDirs += $dstRel
                $uninstallFiles += "  RMDir /r `"`$INSTDIR\$dstRel`""
            }
        }
    }

    # Header/welcome image directives
    $imageDirectives = ""
    if ($imagesGenerated) {
        $imageDirectives = @"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_BITMAP "header.bmp"
!define MUI_HEADERIMAGE_RIGHT
!define MUI_WELCOMEFINISHPAGE_BITMAP "welcome.bmp"
"@
    }

    $nsiContent = @"
; NeuroSkill NSIS Installer Script
; Generated by create-windows-nsis.ps1

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "Sections.nsh"
!include "nsDialogs.nsh"

; ── General ─────────────────────────────────────────────────────────────
Name "$ProductName"
OutFile "$InstallerPath"
InstallDir "`$PROGRAMFILES64\$ProductName"
InstallDirRegKey HKLM "Software\$ProductName" "InstallDir"
RequestExecutionLevel admin
Unicode True

; ── Branding ────────────────────────────────────────────────────────────
!define MUI_ICON "icon.ico"
!define MUI_UNICON "icon.ico"
$imageDirectives
!define MUI_ABORTWARNING

; ── Finish page: launch app after install ───────────────────────────────
; MUI_FINISHPAGE_RUN is intentionally left empty — we use a custom callback
; (un-elevated launch) instead.  The installer runs as admin
; (RequestExecutionLevel admin), so a naive Exec/ExecShell would launch the
; app as Administrator.  The MUI_FINISHPAGE_RUN_FUNCTION callback invokes
; explorer.exe to start the app in the real user's context (not elevated).
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_TEXT "Launch $ProductDisplayName"
!define MUI_FINISHPAGE_RUN_FUNCTION LaunchAsCurrentUser

; ── Pages ───────────────────────────────────────────────────────────────
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "LICENSE"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; ── Version info ────────────────────────────────────────────────────────
VIProductVersion "$Version.0"
VIAddVersionKey "ProductName" "$ProductDisplayName"
VIAddVersionKey "ProductVersion" "$Version"
VIAddVersionKey "FileVersion" "$Version"
VIAddVersionKey "LegalCopyright" "GPL-3.0-only"
VIAddVersionKey "FileDescription" "$ProductDisplayName Installer"

; ── Launch helper (drop admin elevation) ────────────────────────────────
; The installer runs elevated (admin).  Launching the app directly with
; Exec/ExecShell would run it as Administrator, which breaks per-user
; paths, tray registration, and autostart.
;
; The reliable no-plugin approach: use the Windows "runas" trick in
; reverse — ask Explorer (running as the real user) to open the exe.
; We do this by invoking explorer.exe with the full exe path, which
; causes Explorer to ShellExecute it in the user's own session context.
Function LaunchAsCurrentUser
  Exec '"`$WINDIR\explorer.exe" "`$INSTDIR\skill.exe"'
FunctionEnd

; ── Kill running instance before install ─────────────────────────────────
; If the app is already running, the installer cannot replace skill.exe.
; Gracefully ask the user, then force-kill if needed.
Function KillRunningInstance
  FindWindow `$0 "" "$ProductDisplayName"
  IntCmp `$0 0 not_running
    ; App window found — ask user
    MessageBox MB_OKCANCEL|MB_ICONINFORMATION "$ProductDisplayName is currently running and must be closed before installing.$\n$\nClick OK to close it automatically, or Cancel to abort." IDOK kill_it
      Abort
    kill_it:
    ; Try graceful WM_CLOSE first
    SendMessage `$0 `${WM_CLOSE} 0 0
    Sleep 2000
    ; Force-kill if still running
    nsExec::ExecToLog 'taskkill /F /IM skill.exe'
    Sleep 500
  not_running:
FunctionEnd

; ── Install section ─────────────────────────────────────────────────────
Section "$ProductName (required)" SEC_MAIN
  SectionIn RO  ; required — cannot be unchecked

  ; Kill any running instance before overwriting files
  Call KillRunningInstance
$($installFiles -join "`n")

  ; Uninstaller
  SetOutPath "`$INSTDIR"
  WriteUninstaller "`$INSTDIR\uninstall.exe"

  ; Start Menu shortcut
  CreateDirectory "`$SMPROGRAMS\$ProductName"
  CreateShortCut "`$SMPROGRAMS\$ProductName\$ProductName.lnk" "`$INSTDIR\skill.exe" "" "`$INSTDIR\icon.ico"
  CreateShortCut "`$SMPROGRAMS\$ProductName\Uninstall.lnk" "`$INSTDIR\uninstall.exe"

  ; Desktop shortcut
  CreateShortCut "`$DESKTOP\$ProductName.lnk" "`$INSTDIR\skill.exe" "" "`$INSTDIR\icon.ico"

  ; Registry (Add/Remove Programs)
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\$ProductName" "DisplayName" "$ProductDisplayName"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\$ProductName" "DisplayVersion" "$Version"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\$ProductName" "UninstallString" "`$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\$ProductName" "DisplayIcon" "`$INSTDIR\icon.ico"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\$ProductName" "Publisher" "NeuroSkill"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\$ProductName" "InstallLocation" "`$INSTDIR"
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\$ProductName" "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\$ProductName" "NoRepair" 1

  ; Estimated size
  `${GetSize} "`$INSTDIR" "/S=0K" `$0 `$1 `$2
  IntFmt `$0 "0x%08X" `$0
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\$ProductName" "EstimatedSize" `$0

  WriteRegStr HKLM "Software\$ProductName" "InstallDir" "`$INSTDIR"

  ; ── Windows Firewall rule for the local LLM/WebSocket server ───────────
  ; The app listens on localhost for LLM inference and WebSocket commands.
  ; Pre-adding a firewall rule prevents the "allow access?" popup on first
  ; launch.  Failure is non-fatal (user can allow manually).
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="$ProductName"'
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="$ProductName" dir=in action=allow program="`$INSTDIR\skill.exe" enable=yes profile=private,public'
SectionEnd

; ── Vulkan Runtime section (optional, auto-selected when missing) ───────
; GPU-accelerated LLM inference requires the Vulkan loader (vulkan-1.dll).
; Most Windows 10/11 PCs with discrete GPUs already have it.  If it is
; missing, this section downloads and silently installs the LunarG Vulkan
; Runtime, which is ~3 MB and redistributable.
Section /o "Install Vulkan Runtime (GPU acceleration)" SEC_VULKAN
  ; Temporary download path
  StrCpy `$0 "`$TEMP\VulkanRT-Installer.exe"

  ; Download the latest Vulkan Runtime installer from LunarG
  DetailPrint "Downloading Vulkan Runtime..."
  NSISdl::download "https://sdk.lunarg.com/sdk/download/latest/windows/vulkan-runtime.exe" `$0
  Pop `$1
  StrCmp `$1 "success" +3
    DetailPrint "Vulkan Runtime download failed (`$1). GPU acceleration may not work."
    Goto vulkan_done

  ; Silent install (/S = silent mode for the NSIS-based LunarG installer)
  DetailPrint "Installing Vulkan Runtime..."
  nsExec::ExecToLog '"`$0" /S'
  Pop `$1
  ; Non-zero exit is non-fatal — the app still works (falls back to CPU)
  IntCmp `$1 0 +2
    DetailPrint "Vulkan Runtime installer exited with code `$1 (non-fatal)."

  Delete `$0

  vulkan_done:
SectionEnd

; ── VC++ Redistributable section (optional, auto-selected when missing) ─
; Some native dependencies (ONNX Runtime, etc.) require the Visual C++
; 2015-2022 Redistributable.  The binary itself is statically linked, but
; bundled DLLs or plugins may need vcruntime140.dll / msvcp140.dll.
; The official Microsoft installer is ~25 MB and is a no-op if already
; present — it silently exits with code 0 or 1638 (already installed).
Section /o "Install VC++ Redistributable" SEC_VCREDIST
  StrCpy `$0 "`$TEMP\vc_redist.x64.exe"

  DetailPrint "Downloading Visual C++ Redistributable..."
  NSISdl::download "https://aka.ms/vs/17/release/vc_redist.x64.exe" `$0
  Pop `$1
  StrCmp `$1 "success" +3
    DetailPrint "VC++ Redistributable download failed (`$1). Some features may not work."
    Goto vcredist_done

  ; /install /quiet /norestart — standard silent switches for the VC++ installer.
  ; Exit code 0 = success, 1638 = already installed (both are fine).
  DetailPrint "Installing Visual C++ Redistributable..."
  nsExec::ExecToLog '"`$0" /install /quiet /norestart'
  Pop `$1
  IntCmp `$1 0 vcredist_ok
  IntCmp `$1 1638 vcredist_ok
    DetailPrint "VC++ Redistributable installer exited with code `$1 (non-fatal)."
    Goto vcredist_cleanup
  vcredist_ok:
    DetailPrint "Visual C++ Redistributable installed successfully."
  vcredist_cleanup:
  Delete `$0
  vcredist_done:
SectionEnd

; ── WebView2 Runtime section (optional, auto-selected when missing) ─────
; Tauri 2 requires the Microsoft Edge WebView2 Runtime to render the app
; UI.  It is built into Windows 11 but may be absent on older Windows 10
; machines.  The Evergreen Bootstrapper is ~1.8 MB and installs silently.
Section /o "Install WebView2 Runtime (required for UI)" SEC_WEBVIEW2
  StrCpy `$0 "`$TEMP\MicrosoftEdgeWebview2Setup.exe"

  DetailPrint "Downloading WebView2 Runtime..."
  NSISdl::download "https://go.microsoft.com/fwlink/p/?LinkId=2124703" `$0
  Pop `$1
  StrCmp `$1 "success" +3
    DetailPrint "WebView2 download failed (`$1). The app may not display correctly."
    Goto webview2_done

  ; /silent /install — standard quiet switches for the Evergreen Bootstrapper.
  DetailPrint "Installing WebView2 Runtime..."
  nsExec::ExecToLog '"`$0" /silent /install'
  Pop `$1
  IntCmp `$1 0 webview2_ok
    DetailPrint "WebView2 installer exited with code `$1 (may already be installed)."
    Goto webview2_cleanup
  webview2_ok:
    DetailPrint "WebView2 Runtime installed successfully."
  webview2_cleanup:
  Delete `$0
  webview2_done:
SectionEnd

; ── Component descriptions ──────────────────────────────────────────────
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT `${SEC_MAIN}      "Install $ProductDisplayName application files."
  !insertmacro MUI_DESCRIPTION_TEXT `${SEC_VULKAN}    "Download and install the Vulkan Runtime for GPU-accelerated LLM inference. Not needed if your GPU driver already provides Vulkan support."
  !insertmacro MUI_DESCRIPTION_TEXT `${SEC_VCREDIST}  "Download and install the Microsoft Visual C++ 2015-2022 Redistributable (x64). Required by some GPU and AI components. Safe to install even if already present."
  !insertmacro MUI_DESCRIPTION_TEXT `${SEC_WEBVIEW2}  "Download and install the Microsoft Edge WebView2 Runtime. Required to display the application interface. Already included in Windows 11."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

; ── Auto-select optional sections when prerequisites are missing ────────
Function .onInit
  ; Vulkan Runtime
  IfFileExists "`$SYSDIR\vulkan-1.dll" vulkan_found vulkan_missing
  vulkan_missing:
    !insertmacro SelectSection `${SEC_VULKAN}
    Goto vulkan_check_done
  vulkan_found:
  vulkan_check_done:

  ; VC++ Redistributable — check for vcruntime140.dll
  IfFileExists "`$SYSDIR\vcruntime140.dll" vcredist_found vcredist_missing
  vcredist_missing:
    !insertmacro SelectSection `${SEC_VCREDIST}
    Goto vcredist_check_done
  vcredist_found:
  vcredist_check_done:

  ; WebView2 — check registry for installed WebView2 Runtime
  ; The Evergreen Runtime writes to this key on both per-user and per-machine installs.
  ReadRegStr `$0 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  StrCmp `$0 "" 0 webview2_reg_found
  ReadRegStr `$0 HKCU "SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  StrCmp `$0 "" 0 webview2_reg_found
    ; Not found — select the WebView2 section
    !insertmacro SelectSection `${SEC_WEBVIEW2}
    Goto webview2_check_done
  webview2_reg_found:
  webview2_check_done:
FunctionEnd

; ── Uninstall section ───────────────────────────────────────────────────
Section "Uninstall"
$($uninstallFiles -join "`n")
  Delete "`$INSTDIR\uninstall.exe"

  ; Shortcuts
  Delete "`$SMPROGRAMS\$ProductName\$ProductName.lnk"
  Delete "`$SMPROGRAMS\$ProductName\Uninstall.lnk"
  RMDir "`$SMPROGRAMS\$ProductName"
  Delete "`$DESKTOP\$ProductName.lnk"

  ; Firewall rule
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="$ProductName"'

  ; Registry
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\$ProductName"
  DeleteRegKey HKLM "Software\$ProductName"

  ; Remove install directory (only if empty after file deletions)
  RMDir "`$INSTDIR"
SectionEnd
"@

    # NSIS with `Unicode True` requires a BOM to detect UTF-8 encoding.
    # Without BOM it falls back to the system ANSI codepage and mangles
    # non-ASCII characters like ™ in the product display name.
    $utf8WithBom = New-Object System.Text.UTF8Encoding($true)
    [System.IO.File]::WriteAllText($NsiScript, $nsiContent, $utf8WithBom)
    Write-Host "  [ok] NSIS script written (UTF-8 with BOM)"

    # ── Run NSIS ────────────────────────────────────────────────────────────
    Write-Host "  Compiling installer ..."
    & $MakeNsis /V2 $NsiScript
    if ($LASTEXITCODE -ne 0) {
        Write-Error "makensis failed with exit code $LASTEXITCODE"
        exit $LASTEXITCODE
    }
    Write-Host "  [ok] $InstallerExe"

    # ── Sign installer (optional) ───────────────────────────────────────────
    if ($Sign) {
        $thumbprint = $env:CERTIFICATE_THUMBPRINT
        if (-not $thumbprint) {
            Write-Warning "  -Sign requested but `$env:CERTIFICATE_THUMBPRINT is not set. Skipping."
        } else {
            Write-Host "  Signing installer ..."
            & signtool sign `
                /sha1 $thumbprint `
                /fd sha256 `
                /tr http://timestamp.digicert.com `
                /td sha256 `
                /v `
                $InstallerPath
            if ($LASTEXITCODE -ne 0) {
                Write-Error "signtool failed with exit code $LASTEXITCODE"
                exit $LASTEXITCODE
            }
            Write-Host "  [ok] Installer signed"
        }
    }

    # ── Summary ─────────────────────────────────────────────────────────────
    $size = "{0:N1} MB" -f ((Get-Item $InstallerPath).Length / 1MB)
    Write-Host ""
    Write-Host "[ok] $InstallerPath ($size)"
    Write-Host ""
    Write-Host "Contents:"
    Write-Host "  - $BinaryName"
    Write-Host "  - icon.ico"
    foreach ($doc in @("README.md", "CHANGELOG.md", "LICENSE")) {
        if (Test-Path (Join-Path $Staging $doc)) {
            Write-Host "  - $doc"
        }
    }
    if ($resources) {
        foreach ($prop in $resources.PSObject.Properties) {
            Write-Host "  - $($prop.Value)/"
        }
    }
    Write-Host ""
    Write-Host "To install: run $InstallerExe"
} finally {
    # Clean up staging directory
    if (Test-Path $Staging) {
        Remove-Item $Staging -Recurse -Force -ErrorAction SilentlyContinue
    }
}
