<#
.SYNOPSIS
  Run PSScriptAnalyzer on every tracked PowerShell script (`npm run lint:ps`, CI).

.DESCRIPTION
  Uses the rules in PSScriptAnalyzerSettings.psd1 at the repo root. Installs the
  pinned PSScriptAnalyzer for the current user when it is missing. Vendored
  crates under patches/ are skipped. Exits 1 when anything is reported.
#>

$ErrorActionPreference = "Stop"
$Version = "1.24.0"

$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

if (-not (Get-Module -ListAvailable PSScriptAnalyzer | Where-Object { $_.Version -eq $Version })) {
  Write-Host "Installing PSScriptAnalyzer $Version for the current user..."
  Set-PSRepository PSGallery -InstallationPolicy Trusted
  Install-Module PSScriptAnalyzer -RequiredVersion $Version -Scope CurrentUser -Force
}
Import-Module PSScriptAnalyzer -RequiredVersion $Version

$files = git ls-files "*.ps1" "*.psm1" | Where-Object { $_ -notlike "patches/*" -and (Test-Path $_) }
Write-Host "lint-powershell: checking $($files.Count) files"

$settings = Join-Path $Root "PSScriptAnalyzerSettings.psd1"
$results = $files | ForEach-Object { Invoke-ScriptAnalyzer -Path $_ -Settings $settings }

if ($results) {
  $results | ForEach-Object {
    "{0}:{1}: {2} [{3}] {4}" -f $_.ScriptPath.Substring($Root.Length + 1), $_.Line, $_.Severity, $_.RuleName, $_.Message
  }
  exit 1
}
Write-Host "lint-powershell: OK"
