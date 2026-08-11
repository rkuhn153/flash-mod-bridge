# Ruffle engine patches (modBridgeRpc)

Upstream [Ruffle](https://github.com/ruffle-rs/ruffle) does **not** ship this RPC.  
The live bridge needs a **forked player** with `modBridgeRpc` / desktop HTTP bridge.

## Files in this folder

| File | Role |
|------|------|
| `mod_bridge.rs` | Core JSON ops: ping, get/set/call, display walk, SharedObject |
| `mod_bridge_server.rs` | Desktop HTTP bridge (default **:8768**) |
| `tracked-diff.patch` | Diff against upstream `master` for player/web/desktop wiring |

## Apply (developer)

1. Clone Ruffle and check out a known-good commit.
2. Copy `mod_bridge.rs` into `core/src/` and wire it in `lib.rs` / `player.rs` (see patch).
3. Copy `mod_bridge_server.rs` into `desktop/src/` and wire CLI/app (see patch).
4. Apply WASM/JS exports from the patch (`web/src/lib.rs`, player TS).
5. Build desktop and/or web selfhosted per upstream Ruffle docs.

A full prebuilt `ruffle.exe` release may be added later. Until then, **build the fork once**.

## License note

Ruffle is MIT / Apache-2.0. Your engine patches intended for that tree should stay compatible. MCP/inject code in the repo root is MIT (Ryan Kuhn).
