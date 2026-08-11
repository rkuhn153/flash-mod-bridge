<# Restore stock Ruffle auto-update behavior for Flashpoint. #>
param([string]$FlashpointRoot = "C:\Flashpoint")
$ErrorActionPreference = "Stop"
$RuffleData = Join-Path $FlashpointRoot "Data\Ruffle"
$Pin = Join-Path $RuffleData ".mod-bridge-pin"
$ExtConfig = Join-Path $FlashpointRoot "extConfig.json"

if (Test-Path $Pin) { Remove-Item $Pin -Force; "Removed pin" }

foreach ($kind in @("standalone", "webhosted")) {
  $bak = Join-Path $RuffleData "$kind\latest.stock-backup"
  $dst = Join-Path $RuffleData "$kind\latest"
  if (Test-Path $bak) {
    if (Test-Path $dst) { Remove-Item $dst -Recurse -Force }
    Copy-Item -Recurse $bak $dst
    "Restored $kind from stock-backup"
  }
}

if (Test-Path $ExtConfig) {
  $cfg = Get-Content $ExtConfig -Raw | ConvertFrom-Json
  $cfg | Add-Member -NotePropertyName "com.ruffle.latest_standalone_version" -NotePropertyValue "2000-01-01T00:00:00Z" -Force
  $cfg | Add-Member -NotePropertyName "com.ruffle.latest_web_version" -NotePropertyValue "2000-01-01T00:00:00Z" -Force
  $cfg | Add-Member -NotePropertyName "com.ruffle.mod-bridge-default" -NotePropertyValue $false -Force
  $cfg | ConvertTo-Json -Depth 8 | Set-Content $ExtConfig -Encoding UTF8
  "Reset update timestamps in extConfig"
}

"Restart Flashpoint. Ruffle can auto-update from GitHub again."
