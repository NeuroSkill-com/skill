# build-espeak-static.ps1
#
# Build a self-contained espeak-ng.lib (MSVC static library) from source and
# cache the result in src-tauri\espeak-static\.
#
# ── Output ────────────────────────────────────────────────────────────────────
#   src-tauri\espeak-static\lib\espeak-ng.lib   (merged, self-contained)
#   src-tauri\espeak-static\include\espeak-ng\
#   src-tauri\espeak-static\share\espeak-ng-data\
#
# ── Version control ───────────────────────────────────────────────────────────
# Override the tag with:  $env:ESPEAK_TAG_OVERRIDE="1.51.1"; .\build-espeak-static.ps1
#
# ── Requirements ──────────────────────────────────────────────────────────────
#   cmake  git  lib.exe (MSVC)   — all available when Visual Studio Build Tools
#                                  or Visual Studio is installed and a Developer
#                                  Command Prompt / "vcvarsall.bat" env is active.
#
# ── Usage ─────────────────────────────────────────────────────────────────────
#   From the repository root (Developer PowerShell for VS):
#     .\scripts\build-espeak-static.ps1
#   Called automatically by scripts\tauri-build.js on Windows.

$ErrorActionPreference = "Stop"

function Step($msg) { Write-Host "`n▶ $msg" -ForegroundColor Blue }
function Die($msg)  { Write-Host "`nERROR: $msg" -ForegroundColor Red; exit 1 }

# ── Paths ─────────────────────────────────────────────────────────────────────
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot  = Split-Path -Parent $ScriptDir
$StaticDir = Join-Path $RepoRoot "src-tauri\espeak-static"
$StaticLib = Join-Path $StaticDir "lib\espeak-ng.lib"

Write-Host "RepoRoot  = $RepoRoot"
Write-Host "StaticDir = $StaticDir"
Write-Host "StaticLib = $StaticLib"

# ── Cache check ───────────────────────────────────────────────────────────────
if (Test-Path $StaticLib) {
    Write-Host "`nespeak-ng static library already built:"
    Write-Host "  $StaticLib"
    Write-Host "  (delete src-tauri\espeak-static\ to force a rebuild)"
    exit 0
}

# ── Prerequisites ─────────────────────────────────────────────────────────────
Step "Checking prerequisites"
foreach ($tool in @("cmake", "git")) {
    $found = Get-Command $tool -ErrorAction SilentlyContinue
    if (-not $found) { Die "'$tool' not found in PATH. Install it (cmake: winget install Kitware.CMake  git: winget install Git.Git) and restart your terminal." }
    Write-Host "  $tool: $($found.Source)"
}

# lib.exe (MSVC static-library tool) — try PATH first, then vswhere
$LibExe = (Get-Command "lib.exe" -ErrorAction SilentlyContinue)?.Source
if (-not $LibExe) {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $vsInstallPath = & $vswhere -latest -property installationPath 2>$null
        if ($vsInstallPath) {
            $candidate = Get-ChildItem -Path $vsInstallPath -Recurse -Filter "lib.exe" -ErrorAction SilentlyContinue |
                Where-Object { $_.FullName -match "MSVC" -and $_.FullName -match "x64" } |
                Select-Object -First 1
            if ($candidate) {
                $LibExe = $candidate.FullName
                $env:PATH = "$($candidate.DirectoryName);$env:PATH"
            }
        }
    }
}
if ($LibExe) {
    Write-Host "  lib.exe: $LibExe"
} else {
    Write-Host "  lib.exe: not found — companion libs will NOT be merged (run from a Developer Command Prompt for best results)"
}

# ── Version ───────────────────────────────────────────────────────────────────
$EspeakTag = if ($env:ESPEAK_TAG_OVERRIDE) { $env:ESPEAK_TAG_OVERRIDE } else { "1.52.0" }
Write-Host "`nespeak-ng version : $EspeakTag"
Write-Host "(set `$env:ESPEAK_TAG_OVERRIDE to use a different release)"

# ── Clone ─────────────────────────────────────────────────────────────────────
Step "Cloning espeak-ng $EspeakTag"
$BuildTmp = Join-Path ([System.IO.Path]::GetTempPath()) "espeak-build-$(Get-Random)"
New-Item -ItemType Directory -Path $BuildTmp | Out-Null

try {
    git clone --depth=1 --branch $EspeakTag `
        https://github.com/espeak-ng/espeak-ng.git `
        (Join-Path $BuildTmp "espeak-ng")
    if ($LASTEXITCODE -ne 0) { Die "git clone failed — check your internet connection." }
    Write-Host "Clone complete."

    # ── CMake configure ───────────────────────────────────────────────────────
    Step "CMake configure"
    $SrcDir   = Join-Path $BuildTmp "espeak-ng"
    $BuildDir = Join-Path $BuildTmp "build"
    New-Item -ItemType Directory -Path $BuildDir | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $StaticDir "lib") | Out-Null

    cmake -S $SrcDir -B $BuildDir `
        -DCMAKE_BUILD_TYPE=Release `
        -DBUILD_SHARED_LIBS=OFF `
        -DUSE_LIBPCAUDIO=OFF `
        -DUSE_ASYNC=OFF `
        -DUSE_MBROLA=OFF `
        -DCMAKE_INSTALL_PREFIX="$StaticDir"
    if ($LASTEXITCODE -ne 0) { Die "cmake configure failed." }

    # ── CMake build ───────────────────────────────────────────────────────────
    $nproc = (Get-CimInstance Win32_ComputerSystem).NumberOfLogicalProcessors
    Step "CMake build (parallel: $nproc)"
    cmake --build $BuildDir --config Release --parallel $nproc
    if ($LASTEXITCODE -ne 0) { Die "cmake build failed." }

    Step "CMake install → $StaticDir"
    cmake --install $BuildDir --config Release
    if ($LASTEXITCODE -ne 0) { Die "cmake install failed." }

    # ── Merge companion static libraries ──────────────────────────────────────
    #
    # cmake builds libucd and libSpeechPlayer as separate archives.  We merge
    # everything into one self-contained espeak-ng.lib using MSVC lib.exe,
    # mirroring what the Unix script does with ar.

    Step "Merging companion static libraries"
    $companions = Get-ChildItem -Path $BuildDir -Recurse -Filter "*.lib" -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -ne "espeak-ng.lib" }

    if ($companions.Count -gt 0 -and $LibExe) {
        Write-Host "  Found $($companions.Count) companion lib(s):"
        $companions | ForEach-Object { Write-Host "    $($_.Name)  ($([math]::Round($_.Length/1KB)) KB)" }

        $allLibs  = @($StaticLib) + ($companions | ForEach-Object { $_.FullName })
        $mergedTmp = Join-Path $StaticDir "lib\espeak-ng-merged.lib"

        & $LibExe /OUT:$mergedTmp /LTCG:OFF $allLibs
        if ($LASTEXITCODE -eq 0) {
            Move-Item -Force $mergedTmp $StaticLib
            Write-Host "  Merged ✓"
        } else {
            Write-Host "  lib.exe merge failed — using install output as-is (link errors may occur)."
            if (Test-Path $mergedTmp) { Remove-Item $mergedTmp -Force }
        }
    } elseif ($companions.Count -eq 0) {
        Write-Host "  (no companion libraries found — espeak-ng.lib is already self-contained)"
    } else {
        Write-Host "  (lib.exe unavailable — skipping merge; run from a Developer Command Prompt)"
    }

    # ── Copy espeak-ng-data if cmake didn't install it ────────────────────────
    $DataDst = Join-Path $StaticDir "share\espeak-ng-data"
    if (-not (Test-Path $DataDst)) {
        Step "Copying espeak-ng-data"
        $candidates = @(
            (Join-Path $BuildTmp "espeak-ng\espeak-ng-data"),
            (Join-Path $BuildDir "espeak-ng-data"),
            (Join-Path $BuildDir "Release\espeak-ng-data")
        )
        $dataSrc = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
        if ($dataSrc) {
            New-Item -ItemType Directory -Force -Path (Join-Path $StaticDir "share") | Out-Null
            Copy-Item -Recurse -Force $dataSrc $DataDst
            Write-Host "  espeak-ng-data copied from $dataSrc"
        } else {
            Write-Host "  WARNING: espeak-ng-data not found in build tree — TTS will be silent."
        }
    }

    # ── Verify ────────────────────────────────────────────────────────────────
    Step "Verifying"
    if (-not (Test-Path $StaticLib)) { Die "$StaticLib not found after build." }
    $sizeKB = [math]::Round((Get-Item $StaticLib).Length / 1KB)
    Write-Host "`nespeak-ng static library ready:"
    Write-Host "  $StaticLib  (${sizeKB} KB)"

} finally {
    Write-Host "`nCleaning up $BuildTmp ..."
    Remove-Item -Recurse -Force $BuildTmp -ErrorAction SilentlyContinue
}
