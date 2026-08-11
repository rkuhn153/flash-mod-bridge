---
name: flash-mod-bridge
description: >-
  Live Flash/Ruffle mod bridge MCP (flash-mod-bridge). Forked Ruffle with
  modBridgeRpc — AVM get/set/call/find/SharedObject without reload. Use for
  Flashpoint SWFs, selfhosted Ruffle, or Coolmath SWFs rehosted on the fork.
  Not AwayFL (Papa's on Coolmath) until rehosted. Not Unity/HTML5 JS engines.
---

# Flash Mod Bridge MCP

MCP server: **`flash-mod-bridge`**  
Project: `Game Modding/FlashModding/mod-bridge`  
Hub: `http://127.0.0.1:8767`  
(Ruffle tree: `Game Modding/FlashModding`)

| Stack | Port |
|-------|------|
| Unity WebGL | 8765 |
| HTML5 JS | 8766 |
| **Flash / Ruffle** | **8767** |

## Always first

1. MCP server running (wired in `~/.grok/config.toml`)
2. **Flashpoint standalone (preferred):** forked `ruffle.exe` installed → plays SWF → desktop bridge `http://127.0.0.1:8768`
3. **Or** Chrome inject + forked web Ruffle tab
4. `ping_flash_bridge` → `desktop.has_player` and/or page agent

Hub `:8767` falls back to desktop `:8768` automatically.

## Tool order

```text
ping_flash_bridge
  → flash_ping
  → flash_list_display / flash_find
  → flash_list_so / flash_list_props
  → flash_get / flash_set / flash_set_so_prop / flash_call
  → flash_mod_rpc for raw JSON ops
```

## Paths

- `root`, `stage`, `root/ChildName`
- `so:SharedObjectName|property` (use `|` when name has slashes)

## Flashpoint

Default player pin: `FlashModding/flashpoint/`.  
SWFs launch via Ruffle; webhosted path works with inject + this MCP.

## Limits

- Stock Ruffle (no fork) has no `modBridgeRpc`
- Coolmath AwayFL ≠ this bridge until SWF is on forked Ruffle
- Primitive set/get first; nested object writes limited
