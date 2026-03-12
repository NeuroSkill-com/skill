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

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Root = Split-Path -Parent $ScriptDir
$TauriDir = Join-Path $Root "src-tauri"
$Conf = Get-Content (Join-Path $TauriDir "tauri.conf.json") -Raw | ConvertFrom-Json

$ProductName = $Conf.productName
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

; ── Pages ───────────────────────────────────────────────────────────────
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; ── Version info ────────────────────────────────────────────────────────
VIProductVersion "$Version.0"
VIAddVersionKey "ProductName" "$ProductName"
VIAddVersionKey "ProductVersion" "$Version"
VIAddVersionKey "FileVersion" "$Version"
VIAddVersionKey "LegalCopyright" "GPL-3.0-only"
VIAddVersionKey "FileDescription" "$ProductName Installer"

; ── Install section ─────────────────────────────────────────────────────
Section "Install"
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
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\$ProductName" "DisplayName" "$ProductName"
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
SectionEnd

; ── Uninstall section ───────────────────────────────────────────────────
Section "Uninstall"
$($uninstallFiles -join "`n")
  Delete "`$INSTDIR\uninstall.exe"

  ; Shortcuts
  Delete "`$SMPROGRAMS\$ProductName\$ProductName.lnk"
  Delete "`$SMPROGRAMS\$ProductName\Uninstall.lnk"
  RMDir "`$SMPROGRAMS\$ProductName"
  Delete "`$DESKTOP\$ProductName.lnk"

  ; Registry
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\$ProductName"
  DeleteRegKey HKLM "Software\$ProductName"

  ; Remove install directory (only if empty after file deletions)
  RMDir "`$INSTDIR"
SectionEnd
"@

    Set-Content -Path $NsiScript -Value $nsiContent -Encoding UTF8
    Write-Host "  [ok] NSIS script written"

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
