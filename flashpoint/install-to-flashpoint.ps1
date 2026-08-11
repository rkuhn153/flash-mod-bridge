<#
.SYNOPSIS
  Install flash-mod-bridge Ruffle as Flashpoint's default Flash player.

.DESCRIPTION
  Copies a patched desktop build to:
    <Flashpoint>\Data\Ruffle\standalone\latest\ruffle.exe

  Enables Ruffle for SWF games and pins auto-update off so stock GitHub
  builds do not overwrite the fork.

.PARAMETER FlashpointRoot
  Default: C:\Flashpoint

.PARAMETER DesktopExe
  Path to ruffle_desktop.exe (or ruffle.exe) with mod bridge.
  Download from: https://github.com/rkuhn153/flash-mod-bridge/releases

.PARAMETER WebDist
  Optional folder containing ruffle.js for webhosted Flashpoint path.
#>
param(
  [string]$FlashpointRoot = "C:\Flashpoint",
  [string]$DesktopExe = "",
  [string]$WebDist = ""
)

$ErrorActionPreference = "Stop"

Write-Host "Flashpoint root: $FlashpointRoot"

if (-not (Test-Path $FlashpointRoot)) {
  throw "Flashpoint not found at $FlashpointRoot. Pass -FlashpointRoot 'D:\path\to\Flashpoint'"
}

$RuffleData = Join-Path $FlashpointRoot "Data\Ruffle"
$StandDir = Join-Path $RuffleData "standalone\latest"
$WebDir = Join-Path $RuffleData "webhosted\latest"
$ExtConfig = Join-Path $FlashpointRoot "extConfig.json"
$Pin = Join-Path $RuffleData ".mod-bridge-pin"

if (-not $DesktopExe) {
  $here = $PSScriptRoot
  $candidates = @(
    (Join-Path $here "..\ruffle_desktop.exe"),
    (Join-Path $here "..\dist\ruffle_desktop.exe"),
    (Join-Path (Get-Location) "ruffle_desktop.exe"),
    (Join-Path (Get-Location) "ruffle.exe")
  )
  foreach ($c in $candidates) {
    if (Test-Path $c) { $DesktopExe = (Resolve-Path $c).Path; break }
  }
}

if (-not $DesktopExe -or -not (Test-Path $DesktopExe)) {
  throw @"
No DesktopExe found.

1) Download ruffle_desktop.exe from
   https://github.com/rkuhn153/flash-mod-bridge/releases/tag/continuous
2) Re-run:
   .\install-to-flashpoint.ps1 -DesktopExe 'C:\path\to\ruffle_desktop.exe'
"@
}

Write-Host "Desktop: $DesktopExe"

# Backup stock once
$bak = Join-Path $RuffleData "standalone\latest.stock-backup"
if ((Test-Path $StandDir) -and -not (Test-Path $bak)) {
  Copy-Item -Recurse $StandDir $bak
  Write-Host "Backed up stock standalone -> $bak"
}

New-Item -ItemType Directory -Force -Path $StandDir | Out-Null
Copy-Item $DesktopExe (Join-Path $StandDir "ruffle.exe") -Force
Write-Host "Installed standalone -> $StandDir\ruffle.exe" -ForegroundColor Green

if ($WebDist -and (Test-Path (Join-Path $WebDist "ruffle.js"))) {
  $wbak = Join-Path $RuffleData "webhosted\latest.stock-backup"
  if ((Test-Path $WebDir) -and -not (Test-Path $wbak)) {
    Copy-Item -Recurse $WebDir $wbak
  }
  New-Item -ItemType Directory -Force -Path $WebDir | Out-Null
  Copy-Item -Path (Join-Path $WebDist "*") -Destination $WebDir -Recurse -Force
  Write-Host "Installed webhosted -> $WebDir" -ForegroundColor Green
} else {
  Write-Host "Skipping webhosted (optional). Standalone is enough for most SWFs." -ForegroundColor DarkYellow
}

@"
Flashpoint Ruffle is pinned to flash-mod-bridge.
Installed: $(Get-Date -Format o)
Source: $DesktopExe
Remove this file (or run uninstall-pin.ps1) to allow GitHub auto-updates again.
"@ | Set-Content -Path $Pin -Encoding UTF8

if (Test-Path $ExtConfig) {
  $cfg = Get-Content $ExtConfig -Raw | ConvertFrom-Json
  $cfg | Add-Member -NotePropertyName "com.ruffle.enabled" -NotePropertyValue $true -Force
  $cfg | Add-Member -NotePropertyName "com.ruffle.enabled-all" -NotePropertyValue $true -Force
  # Far-future timestamps discourage the extension from replacing our pin
  $cfg | Add-Member -NotePropertyName "com.ruffle.latest_standalone_version" -NotePropertyValue "2099-01-01T00:00:00Z" -Force
  $cfg | Add-Member -NotePropertyName "com.ruffle.latest_web_version" -NotePropertyValue "2099-01-01T00:00:00Z" -Force
  $cfg | Add-Member -NotePropertyName "com.ruffle.mod-bridge-default" -NotePropertyValue $true -Force
  $cfg | ConvertTo-Json -Depth 8 | Set-Content $ExtConfig -Encoding UTF8
  Write-Host "Updated extConfig.json (Ruffle ON for SWFs)" -ForegroundColor Green
} else {
  Write-Warning "extConfig.json not found — enable Ruffle in Flashpoint settings if SWFs still use another player."
}

Write-Host @"

Done.
1. Restart Flashpoint Launcher.
2. Play any .swf — should open forked ruffle.exe (bridge :8768).
3. Start MCP: python run_mcp.py  (hub :8767 → falls back to :8768)
4. Agent: ping_flash_bridge

Pin: $Pin
Undo: .\uninstall-pin.ps1
"@
