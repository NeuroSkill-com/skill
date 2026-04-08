param(
    [ValidateSet("windows")]
    [string]$OS = "windows",
    [switch]$Build,
    [string]$Target = "x86_64-pc-windows-msvc"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Root = Split-Path -Parent $ScriptDir
$TauriDir = Join-Path $Root "src-tauri"

function Fail($msg) {
    Write-Error $msg
    exit 1
}

function Pass($msg) {
    Write-Host "✅ $msg"
}

if ($Build) {
    Push-Location $Root
    try {
        npm run tauri:build:win:nsis
    } finally {
        Pop-Location
    }
}

$releaseDir = Join-Path $TauriDir "target/$Target/release"
$appExe = Join-Path $releaseDir "skill.exe"
$daemonExe = Join-Path $releaseDir "skill-daemon.exe"

if (-not (Test-Path $appExe)) {
    Fail "Missing release app binary: $appExe"
}
if (-not (Test-Path $daemonExe)) {
    Fail "Missing release daemon sidecar: $daemonExe"
}

$nsisInstaller = Get-ChildItem -Path (Join-Path $releaseDir "bundle/nsis") -Filter "*.exe" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

if (-not $nsisInstaller) {
    Fail "No NSIS installer found under $releaseDir/bundle/nsis"
}

# Static guard: ensure installer script still includes daemon copy/install rules.
$nsisScript = Join-Path $Root "scripts/create-windows-nsis.ps1"
$nsisContent = Get-Content $nsisScript -Raw
if ($nsisContent -notmatch 'File "skill-daemon\.exe"') {
    Fail "NSIS script does not include skill-daemon.exe in install payload"
}
if ($nsisContent -notmatch 'Delete "\$INSTDIR\\skill-daemon\.exe"') {
    Fail "NSIS script does not remove skill-daemon.exe on uninstall"
}

# Optional deep check: if 7z is available, inspect installer payload for daemon filename.
$sevenZip = Get-Command 7z -ErrorAction SilentlyContinue
if ($sevenZip) {
    $listOut = & $sevenZip.Source l $nsisInstaller.FullName | Out-String
    if ($listOut -notmatch 'skill-daemon\.exe') {
        Fail "Installer does not appear to contain skill-daemon.exe: $($nsisInstaller.FullName)"
    }
    Pass "NSIS installer bundles skill-daemon.exe ($($nsisInstaller.FullName))"
} else {
    Write-Warning "7z not found; skipped binary payload inspection. Static + release-dir checks passed."
    Pass "Windows packaging checks passed (release sidecar + NSIS script guards)"
}
