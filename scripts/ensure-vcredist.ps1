<#
.SYNOPSIS
    Ensure VC++ Redistributable CRT DLLs are available for app-local bundling.

.DESCRIPTION
    Locates the MSVC CRT redistributable DLLs (vcruntime140.dll, msvcp140.dll,
    etc.) needed for app-local deployment.

    Strategy order:
      1. VCToolsRedistDir env var (set by ilammy/msvc-dev-cmd on CI)
      2. vswhere to find VS install path
      3. Download + install VC++ Redistributable from Microsoft, copy from System32

    On success, sets CRT_REDIST_DIR via GITHUB_ENV (CI) or process env.
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$required = @(
    "vcruntime140.dll",
    "vcruntime140_1.dll",
    "msvcp140.dll",
    "msvcp140_1.dll",
    "vcomp140.dll"
)

function Test-AllDlls($dir) {
    foreach ($dll in $required) {
        if (-not (Test-Path (Join-Path $dir $dll))) { return $false }
    }
    return $true
}

function Find-CrtInRedistRoot($redistRoot) {
    if (-not $redistRoot -or -not (Test-Path $redistRoot)) { return $null }
    foreach ($vcVer in @("VC143", "VC142")) {
        $dirs = Get-ChildItem $redistRoot -Directory -ErrorAction SilentlyContinue |
                  Sort-Object Name -Descending
        foreach ($d in $dirs) {
            $candidate = Join-Path $d.FullName "x64\Microsoft.$vcVer.CRT"
            if (Test-Path $candidate) { return $candidate }
        }
    }
    return $null
}

$crtDir = $null

# ── Strategy 1: VCToolsRedistDir (set by ilammy/msvc-dev-cmd) ───────────────
if ($env:VCToolsRedistDir) {
    Write-Host "[info] VCToolsRedistDir = $env:VCToolsRedistDir"
    $crtDir = Find-CrtInRedistRoot $env:VCToolsRedistDir
    if (-not $crtDir) {
        # VCToolsRedistDir itself may be the parent — try going up
        $parent = Split-Path $env:VCToolsRedistDir -Parent
        $crtDir = Find-CrtInRedistRoot $parent
    }
    if ($crtDir -and (Test-AllDlls $crtDir)) {
        Write-Host "[ok] CRT DLLs found via VCToolsRedistDir: $crtDir"
    }
    else {
        Write-Host "[warn] VCToolsRedistDir set but DLLs not found there"
        $crtDir = $null
    }
}

# ── Strategy 2: vswhere ──────────────────────────────────────────────────────
if (-not $crtDir) {
    $pf86 = [Environment]::GetFolderPath("ProgramFilesX86")
    if (-not $pf86) { $pf86 = "C:\Program Files (x86)" }
    $vswhere = Join-Path $pf86 "Microsoft Visual Studio\Installer\vswhere.exe"

    if (Test-Path $vswhere) {
        $vsPath = & $vswhere -latest -property installationPath 2>$null
        if ($vsPath) {
            $redistRoot = Join-Path $vsPath "VC\Redist\MSVC"
            $crtDir = Find-CrtInRedistRoot $redistRoot
            if ($crtDir -and (Test-AllDlls $crtDir)) {
                Write-Host "[ok] CRT DLLs found via vswhere: $crtDir"
            }
            else {
                $crtDir = $null
            }
        }
    }
}

# ── Strategy 3: Download + install VC++ Redistributable ──────────────────────
if (-not $crtDir) {
    Write-Host "[info] Downloading VC++ Redistributable from Microsoft..."

    $tempDir = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
    $vcRedistExe = Join-Path $tempDir "vc_redist.x64.exe"

    Invoke-WebRequest -Uri "https://aka.ms/vs/17/release/vc_redist.x64.exe" `
        -OutFile $vcRedistExe -UseBasicParsing

    if (-not (Test-Path $vcRedistExe)) {
        Write-Error "Failed to download VC++ Redistributable"
        exit 1
    }

    $sizeMB = [math]::Round((Get-Item $vcRedistExe).Length / 1MB, 1)
    Write-Host "[ok] Downloaded ($sizeMB MB)"

    Write-Host "[info] Installing VC++ Redistributable..."
    $proc = Start-Process -FilePath $vcRedistExe `
                -ArgumentList "/install", "/quiet", "/norestart" `
                -Wait -PassThru

    $exitCode = $proc.ExitCode
    if ($exitCode -ne 0 -and $exitCode -ne 1638) {
        Write-Error "VC++ Redistributable install failed (exit $exitCode)"
        exit 1
    }
    Write-Host "[ok] VC++ Redistributable installed (exit $exitCode)"

    Remove-Item $vcRedistExe -Force -ErrorAction SilentlyContinue

    $crtDir = Join-Path $env:SystemRoot "System32"
}

# ── Final verification ───────────────────────────────────────────────────────
$missing = @()
foreach ($dll in $required) {
    if (-not (Test-Path (Join-Path $crtDir $dll))) {
        $missing += $dll
    }
}

if ($missing.Count -gt 0) {
    $missingList = $missing -join ", "
    Write-Error "CRT DLLs still missing after all strategies: $missingList"
    exit 1
}

Write-Host "[ok] All $($required.Count) CRT DLLs verified:"
foreach ($dll in $required) {
    Write-Host "  $(Join-Path $crtDir $dll)"
}

# ── Export CRT_REDIST_DIR ────────────────────────────────────────────────────
$env:CRT_REDIST_DIR = $crtDir

if ($env:GITHUB_ENV) {
    "CRT_REDIST_DIR=$crtDir" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
}

Write-Host "[ok] CRT_REDIST_DIR=$crtDir"
