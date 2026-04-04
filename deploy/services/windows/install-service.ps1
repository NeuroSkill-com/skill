param(
  [string]$BinaryPath = "$PSScriptRoot\\..\\..\\..\\bin\\skill-daemon.exe",
  [string]$ServiceName = "SkillDaemon"
)

if (-not (Test-Path $BinaryPath)) {
  Write-Error "Daemon binary not found: $BinaryPath"
  exit 1
}

$existing = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($existing) {
  Write-Host "Service exists, stopping/removing $ServiceName"
  sc.exe stop $ServiceName | Out-Null
  sc.exe delete $ServiceName | Out-Null
}

$cmd = '"' + $BinaryPath + '"'
sc.exe create $ServiceName binPath= $cmd start= auto | Out-Null
sc.exe failure $ServiceName reset= 86400 actions= restart/5000/restart/5000/restart/5000 | Out-Null
sc.exe start $ServiceName | Out-Null

Write-Host "Installed and started service: $ServiceName"
