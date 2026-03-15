<#
.SYNOPSIS
  Setup sccache for faster builds on Windows.

.DESCRIPTION
  Installs sccache (compilation cache) to speed up Rust/C++ builds by ~50%.
  mold is Linux-only and not applicable on Windows.

  The build wrapper (scripts/tauri-build.js) auto-detects sccache at build
  time and sets RUSTC_WRAPPER=sccache automatically. No configuration files
  are modified.

.PARAMETER Yes
  Non-interactive mode — installs without prompting.

.EXAMPLE
  .\scripts\setup-build-cache.ps1
  .\scripts\setup-build-cache.ps1 -Yes
#>

param(
  [switch]$Yes
)

$ErrorActionPreference = "Stop"

function Test-CommandExists($cmd) {
  $null -ne (Get-Command $cmd -ErrorAction SilentlyContinue)
}

function Confirm-Action($msg) {
  if ($Yes) { return $true }
  $answer = Read-Host "$msg [Y/n]"
  return ($answer -eq "" -or $answer -match "^[Yy]")
}

Write-Host ""
Write-Host "================================================================" -ForegroundColor Cyan
Write-Host "  Build Cache Setup - sccache (Windows)" -ForegroundColor Cyan
Write-Host "================================================================" -ForegroundColor Cyan
Write-Host ""

# ── sccache ────────────────────────────────────────────────────────────────────

if (Test-CommandExists "sccache") {
  $ver = & sccache --version 2>&1
  Write-Host "[OK] sccache already installed: $ver" -ForegroundColor Green
} else {
  Write-Host "[!] sccache not found" -ForegroundColor Yellow
  Write-Host ""
  Write-Host "  sccache caches Rust and C/C++ compilation outputs."
  Write-Host "  Clean rebuilds become ~50% faster after the first build."
  Write-Host ""

  if (Confirm-Action "Install sccache?") {
    $installed = $false

    # Try scoop first (fastest, prebuilt binary)
    if (Test-CommandExists "scoop") {
      Write-Host "-> Installing via scoop..." -ForegroundColor Cyan
      & scoop install sccache
      $installed = Test-CommandExists "sccache"
    }

    # Try winget
    if (-not $installed -and (Test-CommandExists "winget")) {
      Write-Host "-> Installing via winget..." -ForegroundColor Cyan
      & winget install --id Mozilla.sccache --accept-source-agreements --accept-package-agreements
      # winget installs may need PATH refresh
      $env:Path = [System.Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path", "User")
      $installed = Test-CommandExists "sccache"
    }

    # Fallback to cargo
    if (-not $installed -and (Test-CommandExists "cargo")) {
      Write-Host "-> Installing via cargo (this may take a minute)..." -ForegroundColor Cyan
      & cargo install sccache
      $installed = Test-CommandExists "sccache"
    }

    if ($installed) {
      $ver = & sccache --version 2>&1
      Write-Host "[OK] sccache installed: $ver" -ForegroundColor Green
    } else {
      Write-Host "[X] sccache installation failed. Install manually:" -ForegroundColor Red
      Write-Host "  scoop install sccache"
      Write-Host "  or: cargo install sccache"
      Write-Host "  or: winget install Mozilla.sccache"
    }
  } else {
    Write-Host "-> Skipping sccache" -ForegroundColor Cyan
  }
}

Write-Host ""
Write-Host "-> mold is Linux-only and not applicable on Windows." -ForegroundColor Cyan
Write-Host "   The MSVC linker (link.exe) is used automatically." -ForegroundColor Cyan

# ── Summary ────────────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "================================================================" -ForegroundColor Cyan
Write-Host "  Summary" -ForegroundColor Cyan
Write-Host "================================================================" -ForegroundColor Cyan
Write-Host ""

if (Test-CommandExists "sccache") {
  Write-Host "[OK] sccache: enabled (auto-detected by npm run tauri dev/build)" -ForegroundColor Green
  try {
    $stats = & sccache --show-stats 2>&1
    $loc = ($stats | Select-String "Cache location" | ForEach-Object { $_ -replace '.*:\s*', '' })
    if ($loc) { Write-Host "     Cache location: $loc" -ForegroundColor Cyan }
  } catch {}
} else {
  Write-Host "[!] sccache: not installed (builds will be slower on clean rebuilds)" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "No config files were modified. The build wrapper auto-detects" -ForegroundColor Cyan
Write-Host "sccache at build time. Just run: npm run tauri dev" -ForegroundColor Cyan
Write-Host ""
Write-Host "To disable at build time:" -ForegroundColor Cyan
Write-Host '  $env:SKILL_NO_SCCACHE="1"; npm run tauri build' -ForegroundColor White
Write-Host ""
