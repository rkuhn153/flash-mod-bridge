# Flashpoint + flash-mod-bridge

Make the **patched Ruffle** (with live mod bridge) the player [Flashpoint](https://flashpointarchive.org/) uses for SWFs.

## One-time install

1. Download **`ruffle_desktop.exe`** from  
   https://github.com/rkuhn153/flash-mod-bridge/releases/tag/continuous  
2. Run (adjust paths):

```powershell
cd path\to\flash-mod-bridge\flashpoint
.\install-to-flashpoint.ps1 -DesktopExe "C:\Downloads\ruffle_desktop.exe"
# if Flashpoint is not at C:\Flashpoint:
# .\install-to-flashpoint.ps1 -FlashpointRoot "D:\Flashpoint" -DesktopExe "..."
```

That:

- Backs up stock `Data\Ruffle\standalone\latest` once  
- Installs fork as `Data\Ruffle\standalone\latest\ruffle.exe`  
- Writes pin `Data\Ruffle\.mod-bridge-pin` (blocks auto-update overwrite)  
- Turns on Ruffle for supported + all `.swf` games in `extConfig.json`

3. **Restart Flashpoint Launcher**  
4. Play any Flash game → forked Ruffle window → bridge on **`http://127.0.0.1:8768`**  
5. Start MCP hub (`python run_mcp.py`, port **8767**) → `ping_flash_bridge`

## Undo

```powershell
.\uninstall-pin.ps1
# or: .\uninstall-pin.ps1 -FlashpointRoot "D:\Flashpoint"
```

Restores stock from `*.stock-backup` and allows GitHub updates again.

## How it fits together

| Step | What |
|------|------|
| Flashpoint launches SWF | `Data\Ruffle\standalone\latest\ruffle.exe` |
| Player | HTTP mod bridge **:8768** |
| MCP hub | **:8767** (falls back to desktop if no Chrome agent) |
| Agent | `flash_find` / `flash_get` / `flash_set` / SharedObject tools |

Webhosted Flashpoint games (Chromium embed) need a web build of the fork + optional `inject/` extension; **standalone is enough for most archive SWFs**.
