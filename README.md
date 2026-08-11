# flash-mod-bridge

**Live Flash / Ruffle mod bridge for AI agents (MCP).**

Get, set, call, find display objects, and edit **SharedObject** data in a running SWF — **no reload** for in-memory state — via a forked [Ruffle](https://github.com/ruffle-rs/ruffle) player + Python MCP hub.

> **Status:** Early public release. Needs a **patched Ruffle** (this repo is not upstream Ruffle).  
> Not AwayFL. Not Unity/HTML5 JS engines (see suite links below).

## Why this exists

| Tool | Covers |
|------|--------|
| Upstream Ruffle | Play SWFs |
| Generic browser MCP | Click DOM, not AVM objects |
| **This** | **AVM-aware** paths: `root`, `stage`, `root/Child`, `so:Name\|prop` |

No public **Ruffle / Flash AVM MCP** turned up in research when this shipped — first-mover claim for *this* product shape.

## Architecture

```text
  Agent (Cursor / Claude / Grok)
       │  MCP stdio
  run_mcp.py + translator/     Python FastMCP  (:8767 hub)
       │  HTTP
  ┌────┴─────────────────────┐
  │ Desktop forked ruffle    │  :8768  (Flashpoint / local SWF)
  │ Web forked Ruffle +      │  inject/ Chrome agent or host/
  │   modBridgeRpc           │
  └──────────────────────────┘
       │
  AVM2 / display list / SharedObject
```

| Piece | Path |
|-------|------|
| MCP server | `run_mcp.py`, `translator/` |
| Chrome inject | `inject/` (load unpacked) |
| Self-host page | `host/` |
| Engine patches | `engine/` (drop into a Ruffle tree) |
| Agent skill | `skills/flash-mod-bridge/` |

Default hub port: **8767** (Unity WebGL 8765, HTML5 8766 in the same suite).

## Related projects

| Repo | Role |
|------|------|
| [bepinex-mcp](https://github.com/rkuhn153/bepinex-mcp) | Live Unity + BepInEx |
| [gamecode-rag](https://github.com/rkuhn153/gamecode-rag) | Mono C# search |
| [il2cpp-decompiler](https://github.com/rkuhn153/il2cpp-decompiler) | IL2CPP static decompile |
| *This* | Flash / Ruffle live bridge |

## Quick start (MCP)

### 1. Python hub

```powershell
git clone https://github.com/rkuhn153/flash-mod-bridge.git
cd flash-mod-bridge
python -m venv .venv
.\.venv\Scripts\activate   # Windows
pip install -r requirements.txt
python run_mcp.py
```

### 2. Wire the client

```json
{
  "mcpServers": {
    "flash-mod-bridge": {
      "command": "C:/path/to/python.exe",
      "args": ["C:/path/to/flash-mod-bridge/run_mcp.py"],
      "env": {
        "FLASH_MOD_BRIDGE_PORT": "8767"
      }
    }
  }
}
```

Restart the AI client after editing config.

### 3. Run a patched Ruffle player

You need a build with **`modBridgeRpc`** (see [`engine/README.md`](engine/README.md)):

- **Desktop:** forked `ruffle.exe` opens SWF → HTTP bridge on **:8768** (hub falls back here).  
- **Web:** selfhosted fork + `host/` static page, and/or load `inject/` as an unpacked Chrome extension, then hard-refresh the tab.

### 4. Agent tool order

```text
ping_flash_bridge
  → flash_list_display / flash_find
  → flash_get / flash_set / flash_call
  → flash_list_so / flash_set_so_prop
```

## JSON RPC (player)

```json
{"op":"ping"}
{"op":"list_display","max_depth":3,"limit":100}
{"op":"get","path":"root.someProp"}
{"op":"set","path":"root.someProp","value":999999}
{"op":"call","path":"root","method":"play","args":[]}
{"op":"find","keywords":"money,tip,score","max_depth":5,"limit":60}
{"op":"list_so"}
{"op":"set_so_prop","name":"//example_so","prop":"coins","value":999}
```

| Path | Meaning |
|------|---------|
| `stage` / `root` | Stage or root clip |
| `root/Child/Grand` | Display list walk |
| `so:name` | SharedObject data |
| `so:name\|prop` | SO property (`\|` if name has `/` or `.`) |

## Limits

- Requires **this fork’s** Ruffle — stock ruffle.rs builds will not answer `modBridgeRpc`.  
- AS1/AS2 vs AS3 coverage follows **upstream Ruffle** quality.  
- Coolmath / AwayFL titles need rehost on the fork (or another path).  
- Experimental: bad paths or heavy call spam can still upset a SWF.

## License

- MCP, inject, host, samples: **MIT** — see [LICENSE](LICENSE).  
- Ruffle engine: **MIT / Apache-2.0** (upstream). Patches in `engine/` are meant to apply to that tree.
