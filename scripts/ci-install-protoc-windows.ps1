# SPDX-License-Identifier: GPL-3.0-only
# Install protoc on Windows (Chocolatey with fallback to direct download).

$ErrorActionPreference = 'Stop'

function Test-Protoc {
  try {
    $null = & protoc --version
    return ($LASTEXITCODE -eq 0)
  } catch {
    return $false
  }
}

if (Test-Protoc) {
  protoc --version
  exit 0
}

$installed = $false
for ($i = 1; $i -le 3; $i++) {
  try {
    choco install protoc --no-progress -y
  } catch {
    Write-Host "[warn] choco protoc install attempt $i failed: $($_.Exception.Message)"
  }

  if (Test-Protoc) {
    $installed = $true
    break
  }

  Start-Sleep -Seconds (5 * $i)
}

if (-not $installed) {
  Write-Host "[warn] Chocolatey unavailable; falling back to direct protoc download"
  $ver = "25.3"
  $url = "https://github.com/protocolbuffers/protobuf/releases/download/v$ver/protoc-$ver-win64.zip"
  $zip = Join-Path $env:RUNNER_TEMP "protoc-$ver-win64.zip"
  $dest = Join-Path $env:RUNNER_TEMP "protoc-$ver"
  Invoke-WebRequest -Uri $url -OutFile $zip
  Expand-Archive -Path $zip -DestinationPath $dest -Force
  $bin = Join-Path $dest "bin"
  if (-not (Test-Path (Join-Path $bin "protoc.exe"))) {
    throw "protoc fallback install failed: protoc.exe not found"
  }
  $bin | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
  $env:PATH = "$bin;$env:PATH"
  Write-Host "[ok] Installed protoc via direct download"
}

if (-not (Test-Protoc)) {
  throw "protoc installation failed after Chocolatey + fallback"
}
protoc --version
