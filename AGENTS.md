# Flash mod-bridge — agent rules

MCP server id: **`flash-mod-bridge`**  
Hub: `http://127.0.0.1:8767`

## Mindset

1. **Ruffle fork only** — Coolmath AwayFL will not expose `modBridgeRpc` until rehosted on this player.
2. **Ping first** — `ping_flash_bridge` before mutations.
3. **Find before set** — `flash_find` / `flash_list_so` / `flash_list_props`.
4. **One write at a time**, then `flash_get` readback.
5. Prefer structured tools over raw `flash_mod_rpc`.

## Tool order

```text
ping_flash_bridge
  → flash_ping
  → flash_list_display / flash_find
  → flash_list_so / flash_list_props
  → flash_get / flash_set / flash_set_so_prop / flash_call
```

## Paths

- `root`, `stage`, `root/ChildName`
- `so:SharedObjectName|property`
