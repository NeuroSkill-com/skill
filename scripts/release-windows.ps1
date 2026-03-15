#Requires -Version 5.1
<#
.SYNOPSIS
    Build, sign, package, and upload a NeuroSkill Windows release.

.DESCRIPTION
    Handles the full Windows release pipeline:
      1. Build the Tauri app in release mode (frontend + Rust)
      2. Code-sign the NSIS installer with the Windows certificate
      3. Recreate the updater ZIP from the signed installer
      4. Sign the updater artifact with the Tauri Ed25519 key
      5. Generate the updater JSON manifest
      6. Upload everything to S3

.PARAMETER DryRun
    Print every command that would be executed without running anything
    destructive.  Useful for verifying env vars and paths are correct.

.PARAMETER EnvFile
    Path to a KEY=VALUE env file to load before running.
    Defaults to env.txt next to this script.

.EXAMPLE
    # Full release (all env vars set):
    .\release-windows.ps1

.EXAMPLE
    # Dry run — see what would happen:
    .\release-windows.ps1 -DryRun

.EXAMPLE
    # Use a custom env file:
    .\release-windows.ps1 -EnvFile C:\secrets\prod.env

.EXAMPLE
    # Sign + package only, skip upload:
    $env:SKIP_UPLOAD = "1"; .\release-windows.ps1

# ══════════════════════════════════════════════════════════════════════
# REQUIRED ENVIRONMENT VARIABLES
# ══════════════════════════════════════════════════════════════════════
#
#   WINDOWS_CERTIFICATE
#       Base64-encoded .pfx code-signing certificate (Developer ID or
#       EV/OV certificate from DigiCert, Sectigo, etc.).
#
#       How to get it:
#         1. Purchase a code-signing certificate from a trusted CA.
#            For SmartScreen reputation, an EV certificate is strongly
#            recommended (OV certs require many signed binaries before
#            SmartScreen stops showing warnings).
#         2. Export as .pfx: right-click the cert in certmgr → Export
#            → include private key → set a strong password.
#         3. Base64-encode it:
#              [Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx"))
#              | Set-Clipboard
#            Then paste into the WINDOWS_CERTIFICATE env var / secret.
#
#   WINDOWS_CERTIFICATE_PASSWORD
#       Password protecting the .pfx file.
#
#   TAURI_SIGNING_PRIVATE_KEY
#       Base64-encoded Ed25519 private key for signing Tauri updater
#       artifacts.  The app verifies updates against the corresponding
#       public key embedded in tauri.conf.json → plugins.updater.pubkey.
#
#       How to generate:
#         node -e "require('child_process').execSync('npx tauri signer generate', {stdio:'inherit'})"
#         — or —
#         python3 src-tauri/keys/generate-keys.py
#
#   AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY
#       IAM credentials for uploading release artifacts to S3.
#       Only required when SKIP_UPLOAD is not "1".
#
# ══════════════════════════════════════════════════════════════════════
# OPTIONAL ENVIRONMENT VARIABLES
# ══════════════════════════════════════════════════════════════════════
#
#   TAURI_SIGNING_PRIVATE_KEY_PASSWORD      (default: "")
#       Password protecting the Ed25519 signing key.
#
#   SKIP_SIGN                               (default: "0")
#       Set to "1" to skip Authenticode signing.
#       The installer will trigger a SmartScreen warning on other machines.
#
#   SKIP_UPLOAD                             (default: "0")
#       Set to "1" to skip the S3 upload step.
#
#   TAURI_TARGET                            (default: x86_64-pc-windows-msvc)
#       Override the Rust compilation target triple.
#       Common values:
#         x86_64-pc-windows-msvc   — 64-bit Intel/AMD (default)
#         aarch64-pc-windows-msvc  — Windows on ARM
#
#   TIMESTAMP_URL                           (default: http://timestamp.digicert.com)
#       RFC 3161 timestamp server URL for Authenticode signing.
#       Alternatives: http://timestamp.sectigo.com
#                     http://timestamp.globalsign.com/tsa/r6advanced1
#
#   S3_BUCKET                               (default: releases.example.com)
#   S3_REGION                               (default: us-east-1)
#   S3_PREFIX                               (default: skill)
#   AWS_PROFILE
#   CLOUDFRONT_DISTRIBUTION_ID
#
# ══════════════════════════════════════════════════════════════════════
# REQUIRED TOOLS (must be in PATH or auto-discovered)
# ══════════════════════════════════════════════════════════════════════
#
#   signtool.exe  — Windows SDK (installed with Visual Studio or
#                   "Windows 10/11 SDK" standalone installer).
#                   Auto-discovered under Program Files (x86)\Windows Kits\10.
#   npx           — Node.js package runner  (nodejs.org)
#   npm           — Node.js package manager (bundled with Node.js)
#   cargo         — Rust toolchain          (rustup.rs)
#   aws           — AWS CLI v2              (aws.amazon.com/cli)
#                   Only required when SKIP_UPLOAD != 1
#
#>

[CmdletBinding(SupportsShouldProcess)]
param(
    [switch]$DryRun,
    [string]$EnvFile = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Ensure consistent UTF-8 rendering for UI/user-facing strings (e.g. ™).
try {
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [Console]::InputEncoding = $utf8NoBom
    [Console]::OutputEncoding = $utf8NoBom
    $OutputEncoding = $utf8NoBom
    chcp 65001 | Out-Null
} catch {
    # best-effort only
}

# ── Constants ──────────────────────────────────────────────────────────────────

$APP_NAME   = "skill"
$REPO_ROOT  = Split-Path -Parent $MyInvocation.MyCommand.Path
$TAURI_DIR  = Join-Path $REPO_ROOT "src-tauri"

$S3_BUCKET  = if ($env:S3_BUCKET)  { $env:S3_BUCKET }  else { "releases.example.com" }
$S3_REGION  = if ($env:S3_REGION)  { $env:S3_REGION }  else { "us-east-1" }
$S3_PREFIX  = if ($env:S3_PREFIX)  { $env:S3_PREFIX }  else { "skill" }
$TIMESTAMP_URL = if ($env:TIMESTAMP_URL) { $env:TIMESTAMP_URL } else { "http://timestamp.digicert.com" }

# ── Helpers ────────────────────────────────────────────────────────────────────

function Log  ($msg) { Write-Host "-> $msg" -ForegroundColor Blue }
function Ok   ($msg) { Write-Host "v  $msg" -ForegroundColor Green }
function Warn ($msg) { Write-Host "!  $msg" -ForegroundColor Yellow }
function Err  ($msg) { Write-Host "x  $msg" -ForegroundColor Red }
function Dry  ($msg) { Write-Host "  [dry-run] $msg" -ForegroundColor Yellow }

function Fail ($msg) {
    Err $msg
    exit 1
}

# Run a command, or just print it in dry-run mode.
function Run {
    param([string]$Cmd, [string[]]$Args)
    if ($DryRun) {
        Dry "$Cmd $($Args -join ' ')"
    } else {
        & $Cmd @Args
        if ($LASTEXITCODE -ne 0) { Fail "'$Cmd $($Args -join ' ')' exited with code $LASTEXITCODE" }
    }
}

function CheckVar ($name) {
    $val = [System.Environment]::GetEnvironmentVariable($name)
    if ([string]::IsNullOrEmpty($val)) {
        if ($DryRun) {
            Dry "WARNING: $name is not set (would fail in real run)"
        } else {
            Fail "Missing required env var: $name"
        }
    }
}

# ── Load env file ──────────────────────────────────────────────────────────────

$resolvedEnvFile = if ($EnvFile -ne "") {
    $EnvFile
} else {
    Join-Path $REPO_ROOT "env.txt"
}

if ($EnvFile -ne "" -and -not (Test-Path $resolvedEnvFile)) {
    Fail "Env file not found: $resolvedEnvFile"
}

if (Test-Path $resolvedEnvFile) {
    Log "Loading environment from $resolvedEnvFile"
    Get-Content $resolvedEnvFile | ForEach-Object {
        $line = $_.Trim()
        if ($line -eq "" -or $line.StartsWith("#")) { return }
        if ($line -match "^([A-Za-z_][A-Za-z0-9_]*)=(.*)$") {
            $key = $Matches[1]
            $val = $Matches[2]
            # Strip surrounding quotes
            if ($val -match '^"(.*)"$' -or $val -match "^'(.*)'$") {
                $val = $Matches[1]
            }
            [System.Environment]::SetEnvironmentVariable($key, $val, "Process")
        }
    }
} else {
    Log "No env.txt found at $resolvedEnvFile — falling back to environment variables"
}

# ── Preflight checks ───────────────────────────────────────────────────────────

$SKIP_SIGN   = $env:SKIP_SIGN   -eq "1"
$SKIP_UPLOAD = $env:SKIP_UPLOAD -eq "1"

if (-not $SKIP_SIGN) {
    CheckVar "WINDOWS_CERTIFICATE"
    CheckVar "WINDOWS_CERTIFICATE_PASSWORD"
}
CheckVar "TAURI_SIGNING_PRIVATE_KEY"
if (-not $SKIP_UPLOAD) {
    CheckVar "AWS_ACCESS_KEY_ID"
    CheckVar "AWS_SECRET_ACCESS_KEY"
}

# Verify Node / npm / cargo are available
foreach ($cmd in @("npx", "npm", "cargo")) {
    if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) {
        Fail "Required tool not found: $cmd"
    }
}
if (-not $SKIP_UPLOAD) {
    if (-not (Get-Command "aws" -ErrorAction SilentlyContinue)) {
        Fail "Required tool not found: aws  (install from https://aws.amazon.com/cli/)"
    }
}

# Auto-discover signtool.exe from the Windows SDK
$signtool = $null
if (-not $SKIP_SIGN) {
    $wkBase = "C:\Program Files (x86)\Windows Kits\10\bin"
    if (Test-Path $wkBase) {
        $signtool = Get-ChildItem $wkBase -Recurse -Filter "signtool.exe" -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -like "*x64*" } |
            Sort-Object FullName -Descending |
            Select-Object -First 1 -ExpandProperty FullName
    }
    if (-not $signtool) {
        # Fall back to PATH
        $cmd = Get-Command "signtool.exe" -ErrorAction SilentlyContinue
        if ($cmd) { $signtool = $cmd.Source }
    }
    if (-not $signtool) {
        Fail ("signtool.exe not found.  Install the Windows 10/11 SDK:" +
              "`n  winget install Microsoft.WindowsSDK.10.0.22621" +
              "`n  — or install Visual Studio with the 'Desktop development with C++' workload.")
    }
    Log "signtool: $signtool"
}

# Read version from tauri.conf.json
$tauriConf = Get-Content (Join-Path $TAURI_DIR "tauri.conf.json") -Raw | ConvertFrom-Json
$VERSION = $tauriConf.version

$TAURI_TARGET = if ($env:TAURI_TARGET) { $env:TAURI_TARGET } else { "x86_64-pc-windows-msvc" }

Log "Release build for $APP_NAME v$VERSION"
Log "Target:      $TAURI_TARGET"
Log "Timestamp:   $TIMESTAMP_URL"
Log "S3 dest:     s3://$S3_BUCKET/$S3_PREFIX/$VERSION/"
if ($DryRun) { Log "MODE: DRY RUN — nothing destructive will execute" }

# ── Step 1: Build ──────────────────────────────────────────────────────────────

Log "Installing JS dependencies…"
Set-Location $REPO_ROOT
Run "npm" @("ci", "--prefer-offline")

Log "Building frontend…"
Run "npm" @("run", "build")

# ── Step 0: Ensure Vulkan SDK is installed ────────────────────────────────────
#
# The llm-vulkan feature requires the LunarG Vulkan SDK at build time.
# install-vulkan-sdk.ps1 is a no-op when the SDK is already present; when it
# is missing it downloads and silently installs the latest version (~200 MB).

Log "Ensuring Vulkan SDK is installed…"
if ($DryRun) {
    Dry "powershell -NoProfile -ExecutionPolicy Bypass -File $REPO_ROOT\scripts\install-vulkan-sdk.ps1"
} else {
    & powershell -NoProfile -ExecutionPolicy Bypass -File "$REPO_ROOT\scripts\install-vulkan-sdk.ps1"
    if ($LASTEXITCODE -ne 0) { Fail "install-vulkan-sdk.ps1 failed (exit $LASTEXITCODE)" }
}

Log "Building Rust binary (target: $TAURI_TARGET, GPU: Vulkan)…"
# llm-vulkan enables Vulkan GPU offloading for LLM inference (covers NVIDIA,
# AMD, and Intel Arc without requiring the CUDA toolkit).  Requires the Vulkan
# SDK (https://vulkan.lunarg.com) at build time; falls back to CPU at runtime
# when no Vulkan-capable device is present.
$cargoBuildArgs = @("build", "--release", "--target", $TAURI_TARGET, "--features", "llm-vulkan,custom-protocol")
if ($DryRun) {
    Dry "cargo $($cargoBuildArgs -join ' ')"
} else {
    Set-Location $TAURI_DIR
    & cargo @cargoBuildArgs
    if ($LASTEXITCODE -ne 0) { Fail "cargo build failed" }
    Set-Location $REPO_ROOT
}

Ok "Build complete"

# ── Assemble NSIS bundle ───────────────────────────────────────────────────────
#
# We run `npm run build` + `cargo build` directly (same pattern as the GitHub
# Actions macOS workflow) and then invoke the Tauri bundler only for NSIS
# packaging — not the full `tauri build` pipeline — to keep signing separate.
# The bundler is invoked via `npx tauri bundle --target nsis` so the binary it
# packs is already compiled.

Log "Bundling NSIS installer…"

$bundleArgs = @("tauri", "bundle", "--target", $TAURI_TARGET, "--bundle", "nsis", "--no-sign", "--features", "llm-vulkan")
if ($DryRun) {
    Dry "npx $($bundleArgs -join ' ')"
} else {
    & npx @bundleArgs
    if ($LASTEXITCODE -ne 0) { Fail "tauri bundle failed" }
}

Ok "NSIS bundle created"

# ── Locate the installer ───────────────────────────────────────────────────────

$BUNDLE_BASE = Join-Path $TAURI_DIR "target\$TAURI_TARGET\release\bundle"
$NSIS_DIR    = Join-Path $BUNDLE_BASE "nsis"

$installer = Get-ChildItem $NSIS_DIR -Filter "*-setup.exe" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1

if (-not $installer) {
    if ($DryRun) {
        $installer = [PSCustomObject]@{ FullName = "$NSIS_DIR\${APP_NAME}_${VERSION}_x64-setup.exe" }
        Dry "Would expect installer at: $($installer.FullName)"
    } else {
        Fail "Could not find *-setup.exe in $NSIS_DIR"
    }
}

$INSTALLER_PATH = $installer.FullName
Log "Installer: $INSTALLER_PATH"

# ── Step 2: Authenticode-sign the installer ────────────────────────────────────

if ($SKIP_SIGN) {
    Warn "Skipping Authenticode signing (SKIP_SIGN=1)"
} else {
    Log "Decoding certificate to temp file…"

    $certPath = Join-Path $env:TEMP "neuroskill-sign-$([System.Guid]::NewGuid().ToString('N')).pfx"

    if (-not $DryRun) {
        $certBytes = [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE)
        [IO.File]::WriteAllBytes($certPath, $certBytes)
    }

    try {
        Log "Signing installer with Authenticode…"
        $signArgs = @(
            "sign",
            "/fd",  "SHA256",
            "/td",  "SHA256",
            "/tr",  $TIMESTAMP_URL,
            "/f",   $certPath,
            "/p",   $env:WINDOWS_CERTIFICATE_PASSWORD,
            $INSTALLER_PATH
        )

        if ($DryRun) {
            # Redact the password in dry-run output
            $redacted = $signArgs -replace [regex]::Escape($env:WINDOWS_CERTIFICATE_PASSWORD), "***"
            Dry "$signtool $($redacted -join ' ')"
        } else {
            & $signtool @signArgs
            if ($LASTEXITCODE -ne 0) { Fail "signtool failed (exit $LASTEXITCODE)" }
        }

        Ok "Installer signed"

        Log "Verifying Authenticode signature…"
        if ($DryRun) {
            Dry "$signtool verify /pa /v $INSTALLER_PATH"
        } else {
            & $signtool "verify" "/pa" "/v" $INSTALLER_PATH
            if ($LASTEXITCODE -ne 0) { Fail "Signature verification failed" }
            Ok "Signature valid"
        }
    } finally {
        if (Test-Path $certPath) { Remove-Item $certPath -Force }
    }
}

# ── Step 3: Recreate updater ZIP from the signed installer ─────────────────────
#
# The Tauri bundler packs the installer into a .nsis.zip *before* we
# Authenticode-sign it, so the bundler-produced ZIP contains a stale,
# unsigned copy.  We delete it and rebuild from the freshly-signed installer.
#
# Signing note: in Tauri v2, `tauri signer sign` takes the file as a plain
# positional argument.  The short flag `-f` is now an alias for
# --private-key-path (the path to the *key* file) and therefore conflicts with
# the TAURI_SIGNING_PRIVATE_KEY env var (which maps to --private-key).
# Using `-f <zipfile>` would produce:
#   error: the argument '--private-key-path' cannot be used with '--private-key'

Log "Recreating updater ZIP from signed installer…"

# Remove any stale ZIP(s) the bundler produced before signing.
Get-ChildItem $NSIS_DIR -Include "*.nsis.zip", "*.nsis.zip.sig" -ErrorAction SilentlyContinue |
    Remove-Item -Force

$INSTALLER_NAME = [IO.Path]::GetFileName($INSTALLER_PATH)
$ZIP_NAME       = "$INSTALLER_NAME.zip"
$ZIP_PATH       = Join-Path $NSIS_DIR $ZIP_NAME
$SIG_PATH       = "$ZIP_PATH.sig"

if ($DryRun) {
    Dry "Compress-Archive -Path $INSTALLER_PATH -DestinationPath $ZIP_PATH"
    Dry "npx tauri signer sign $ZIP_PATH"
} else {
    Compress-Archive -Path $INSTALLER_PATH -DestinationPath $ZIP_PATH -Force
    Ok "Updater ZIP created: $ZIP_PATH"

    # Sign with the Tauri Ed25519 key.
    # FILE is a positional argument — do NOT use -f (that flag is --private-key-path).
    & npx tauri signer sign $ZIP_PATH
    if ($LASTEXITCODE -ne 0) { Fail "tauri signer sign failed" }

    if (-not (Test-Path $SIG_PATH)) { Fail "Signer did not produce $SIG_PATH" }
    Ok "Updater ZIP signed: $SIG_PATH"
}

Log "Updater artifacts:"
Log "  installer: $INSTALLER_PATH"
Log "  zip:       $ZIP_PATH"
Log "  sig:       $SIG_PATH"

# ── Step 4: Generate updater JSON manifest ─────────────────────────────────────

Log "Generating latest.json manifest…"

$MANIFEST_FILE  = Join-Path $BUNDLE_BASE "latest.json"
$PUB_DATE       = (Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ")
$ZIP_NAME_ONLY  = [IO.Path]::GetFileName($ZIP_PATH)
$UPDATER_URL    = "https://$S3_BUCKET/$S3_PREFIX/$VERSION/$ZIP_NAME_ONLY"

$sigContent = if (-not $DryRun -and (Test-Path $SIG_PATH)) {
    (Get-Content $SIG_PATH -Raw).Trim()
} else {
    "<signature-placeholder>"
}

# Get git tag annotation as release notes; fall back gracefully.
$releaseNotes = ""
try {
    $releaseNotes = (& git tag -l --format="%(contents)" "v$VERSION" 2>$null).Trim()
} catch { }
if ([string]::IsNullOrEmpty($releaseNotes)) {
    $releaseNotes = "NeuroSkill™ v$VERSION"
}

$manifest = [ordered]@{
    version  = $VERSION
    notes    = $releaseNotes
    pub_date = $PUB_DATE
    platforms = [ordered]@{
        "windows-x86_64" = [ordered]@{
            url       = $UPDATER_URL
            signature = $sigContent
        }
    }
}

$manifestJson = $manifest | ConvertTo-Json -Depth 10
Set-Content -Path $MANIFEST_FILE -Value $manifestJson -Encoding UTF8
Log "Manifest written to $MANIFEST_FILE"
if ($DryRun) { Dry "Manifest contents:`n$manifestJson" }

# ── Step 5: Upload to S3 ───────────────────────────────────────────────────────

if ($SKIP_UPLOAD) {
    Log "Skipping S3 upload (SKIP_UPLOAD=1)"
} else {
    Log "Uploading to s3://$S3_BUCKET/$S3_PREFIX/$VERSION/ …"

    $awsArgs = @("--region", $S3_REGION)
    if ($env:AWS_PROFILE) { $awsArgs += @("--profile", $env:AWS_PROFILE) }

    $S3_DEST = "s3://$S3_BUCKET/$S3_PREFIX/$VERSION"

    # Installer .exe
    if (Test-Path $INSTALLER_PATH) {
        Run "aws" ($awsArgs + @(
            "s3", "cp",
            $INSTALLER_PATH, "$S3_DEST/$INSTALLER_NAME",
            "--content-type", "application/octet-stream"
        ))
        Ok "Uploaded installer"
    }

    # Updater ZIP
    if (Test-Path $ZIP_PATH) {
        Run "aws" ($awsArgs + @(
            "s3", "cp",
            $ZIP_PATH, "$S3_DEST/$ZIP_NAME_ONLY",
            "--content-type", "application/zip"
        ))
        Ok "Uploaded updater ZIP"
    }

    # Updater signature
    if (Test-Path $SIG_PATH) {
        Run "aws" ($awsArgs + @(
            "s3", "cp",
            $SIG_PATH, "$S3_DEST/$ZIP_NAME_ONLY.sig",
            "--content-type", "text/plain"
        ))
        Ok "Uploaded updater signature"
    }

    # Manifest — versioned path
    if (Test-Path $MANIFEST_FILE) {
        Run "aws" ($awsArgs + @(
            "s3", "cp",
            $MANIFEST_FILE, "$S3_DEST/latest.json",
            "--content-type", "application/json"
        ))

        # latest path (stable URL for the updater endpoint)
        Run "aws" ($awsArgs + @(
            "s3", "cp",
            $MANIFEST_FILE, "s3://$S3_BUCKET/$S3_PREFIX/latest/latest.json",
            "--content-type", "application/json"
        ))
        Ok "Uploaded manifest (versioned + latest)"
    }

    Ok "S3 upload complete"

    # Optionally invalidate CloudFront cache
    if ($env:CLOUDFRONT_DISTRIBUTION_ID) {
        Log "Invalidating CloudFront cache…"
        Run "aws" ($awsArgs + @(
            "cloudfront", "create-invalidation",
            "--distribution-id", $env:CLOUDFRONT_DISTRIBUTION_ID,
            "--paths", "/$S3_PREFIX/*"
        ))
        Ok "CloudFront invalidation submitted"
    }
}

# ── Summary ────────────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "================================================================" -ForegroundColor Cyan
Write-Host "                     Release Complete                          " -ForegroundColor Cyan
Write-Host "================================================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "  App:          $APP_NAME v$VERSION"
Write-Host "  Target:       $TAURI_TARGET"
Write-Host "  Installer:    $INSTALLER_PATH"
Write-Host "  Signed:       $(if ($SKIP_SIGN) { 'SKIPPED' } else { 'YES (Authenticode)' })"
Write-Host "  S3 upload:    $(if ($SKIP_UPLOAD) { 'SKIPPED' } else { "s3://$S3_BUCKET/$S3_PREFIX/$VERSION/" })"
Write-Host "  Dry run:      $(if ($DryRun) { 'YES' } else { 'no' })"
Write-Host ""
if (-not $SKIP_UPLOAD -and -not $DryRun) {
    Write-Host "  Update endpoint URL:"
    Write-Host "    https://$S3_BUCKET/$S3_PREFIX/latest/latest.json"
    Write-Host ""
}
Write-Host "  Artifacts uploaded to S3:"
Write-Host "    s3://$S3_BUCKET/$S3_PREFIX/$VERSION/$INSTALLER_NAME"
Write-Host "    s3://$S3_BUCKET/$S3_PREFIX/$VERSION/$ZIP_NAME_ONLY"
Write-Host "    s3://$S3_BUCKET/$S3_PREFIX/$VERSION/$ZIP_NAME_ONLY.sig"
Write-Host "    s3://$S3_BUCKET/$S3_PREFIX/$VERSION/latest.json"
Write-Host "    s3://$S3_BUCKET/$S3_PREFIX/latest/latest.json"
Write-Host ""
