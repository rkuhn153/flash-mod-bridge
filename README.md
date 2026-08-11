# flash-mod-bridge

Live **Flash / Ruffle** bridge for AI agents and tools via **MCP (Model Context Protocol)**.

Inspect a running SWF, walk the display list, get/set properties, call methods, and edit **SharedObject** data — **without reloading** for in-memory state — through a forked [Ruffle](https://github.com/ruffle-rs/ruffle) player plus a Python MCP hub.

This is **not** stock Ruffle (no agent API). It is **not** AwayFL, Unity, or HTML5/Phaser (see suite links below).

| Piece | Path | Role |
|--------|------|------|
| Patched Ruffle (desktop) | [Releases](https://github.com/rkuhn153/flash-mod-bridge/releases) → `ruffle_desktop.exe` | Play SWF + HTTP mod bridge **:8768** |
| Engine patches | `engine/` | `modBridgeRpc` sources to drop into a Ruffle tree |
| MCP hub | `run_mcp.py`, `translator/` | FastMCP tools (hub **:8767**) |
| Chrome inject | `inject/` | Optional page agent for web Ruffle |
| Self-host page | `host/` | Simple local player host |
| Flashpoint pin | `flashpoint/` | Install fork as Flashpoint’s default Ruffle |
| Agent skill | `skills/flash-mod-bridge/` | How agents should use the tools |

**Status:** Early public release. Core path is desktop + Flashpoint; web inject is optional.

## What you can do

| Area | Capabilities |
|------|----------------|
| **Discover** | List display tree, find by keywords, list props on a path |
| **Read / write** | Get/set AVM properties by path (`root`, `stage`, `root/Child`) |
| **Call** | Invoke methods on display objects with JSON args |
| **SharedObject** | List SOs, set SO properties (saves / persistent data) |
| **Raw RPC** | Pass through any player JSON op via `flash_mod_rpc` |
| **Transport** | Desktop HTTP bridge, or Chrome inject when using web Ruffle |

**Not supported:** stock ruffle.rs builds (no `modBridgeRpc`), AwayFL runtimes, or rewriting the SWF on disk. Prefer live paths over re-decompiling ActionScript unless you have a separate pipeline.

**Always start with** `ping_flash_bridge` so you know a player/agent is connected before bulk get/set.

## How it fits together

```text
  Agent (Cursor / Claude / Grok)
       │  MCP stdio
  run_mcp.py + translator/          hub :8767
       │  HTTP
  ┌────┴──────────────────────────┐
  │ Desktop forked ruffle         │  :8768  ← Flashpoint / local SWF
  │ Web forked Ruffle + inject/   │  optional browser path
  └───────────────────────────────┘
       │
  AVM display list / SharedObject
```

| Port | Who |
|------|-----|
| **8767** | Python MCP hub (agents connect here) |
| **8768** | Desktop Ruffle mod-bridge HTTP (hub falls back here) |
| 8765 / 8766 | Other suite tools (Unity WebGL / HTML5) — not this repo |

## MCP tools

Names as exposed by `translator/server.py`:

### Connection
- `ping_flash_bridge` — hub health, desktop player, connected agents
- `list_connected_agents` — which inject/desktop endpoints are live
- `flash_ping` — ping the player RPC
- `flash_mod_rpc` — raw JSON request string to the player

### Display list & values
- `flash_list_display` — walk tree (`max_depth`, `limit`)
- `flash_find` — keyword search (money, score, day, …)
- `flash_list_props` — properties on a path (default `root`)
- `flash_get` / `flash_set` — read/write by path (`value_json` for set)
- `flash_call` — `path` + `method` + `args_json`

### SharedObject
- `flash_list_so` — known SharedObjects
- `flash_set_so_prop` — set one SO field (`name`, `prop`, `value_json`)

### Paths

| Path | Meaning |
|------|---------|
| `stage` | Stage AS3 object |
| `root` | Root movie clip |
| `root/Child/Grand` | Display-list walk |
| `so:name` | Live SharedObject **data** |
| `so:name\|prop` | SO property (`\|` when name has `/` or `.`) |

## Requirements

- **Windows x64** for the prebuilt desktop player (typical)
- **Python 3.10+** for the MCP hub
- A **patched Ruffle** build with mod bridge (prebuilt release **or** apply `engine/` yourself)
- Optional: [Flashpoint](https://flashpointarchive.org/) for archive SWFs
- Optional: Chrome for `inject/` + web selfhosted Ruffle

Upstream Ruffle quality limits AS1/AS2/AS3 compatibility — this bridge cannot fix content Ruffle cannot run.

## Quick start

### 1. Get the patched player (recommended)

From **[Releases](https://github.com/rkuhn153/flash-mod-bridge/releases)** (tag **`continuous`**):

| Asset | Role |
|-------|------|
| `ruffle_desktop.exe` | Patched Ruffle + mod bridge (**:8768**) |
| `flash-mod-bridge-mcp.zip` | Python hub + inject/host (optional zip) |

```powershell
.\ruffle_desktop.exe "D:\path\to\game.swf"
```

Building from source is **optional** — see [`engine/README.md`](engine/README.md).

### 2. Run the MCP hub

```powershell
git clone https://github.com/rkuhn153/flash-mod-bridge.git
cd flash-mod-bridge
python -m venv .venv
.\.venv\Scripts\activate
pip install -r requirements.txt
python run_mcp.py
```

Hub listens on **8767** by default (`FLASH_MOD_BRIDGE_PORT`).

### 3. Wire an MCP client

**Cursor** (`~/.cursor/mcp.json`):

```json
{
  "mcpServers": {
    "flash-mod-bridge": {
      "command": "C:/Python313/python.exe",
      "args": ["C:/path/to/flash-mod-bridge/run_mcp.py"],
      "env": {
        "FLASH_MOD_BRIDGE_PORT": "8767"
      }
    }
  }
}
```

**Grok Build** (`~/.grok/config.toml`):

```toml
[mcp_servers.flash-mod-bridge]
command = 'C:\Python313\python.exe'
args = ['C:\path\to\flash-mod-bridge\run_mcp.py']
enabled = true

[mcp_servers.flash-mod-bridge.env]
FLASH_MOD_BRIDGE_PORT = "8767"
```

Restart the client (or reload MCP) after config changes.

### 4. Flashpoint (best for archive SWFs)

Pin the prebuilt player so every Flashpoint `.swf` launches with the bridge:

```powershell
cd flash-mod-bridge\flashpoint
.\install-to-flashpoint.ps1 -DesktopExe "C:\Downloads\ruffle_desktop.exe"
# .\install-to-flashpoint.ps1 -FlashpointRoot "D:\Flashpoint" -DesktopExe "..."
```

Then:

1. **Restart Flashpoint Launcher**  
2. Play any Flash game → forked Ruffle → **:8768**  
3. MCP hub running (`python run_mcp.py`)  
4. Agent: `ping_flash_bridge`  

| Detail | |
|--------|--|
| Installed as | `%Flashpoint%\Data\Ruffle\standalone\latest\ruffle.exe` |
| Pin | `Data\Ruffle\.mod-bridge-pin` (blocks stock auto-update) |
| Undo | `flashpoint\uninstall-pin.ps1` |

Full notes: [`flashpoint/README.md`](flashpoint/README.md).

### 5. Web / inject (optional)

- Build web selfhosted from a patched Ruffle tree; serve `host/`  
- Load `inject/` as an **unpacked** Chrome extension; hard-refresh the player tab  

For **Flashpoint standalone** you usually **do not** need inject.

### 6. First agent session

```text
ping_flash_bridge
  → flash_list_display / flash_find
  → flash_list_props / flash_get
  → flash_set / flash_call
  → flash_list_so / flash_set_so_prop
```

## Player JSON RPC (reference)

The desktop/web player speaks the same ops the MCP wraps:

```json
{"op":"ping"}
{"op":"list_display","max_depth":3,"limit":100}
{"op":"list_props","path":"root"}
{"op":"get","path":"root.someProp"}
{"op":"set","path":"root.someProp","value":999999}
{"op":"call","path":"root","method":"play","args":[]}
{"op":"find","keywords":"money,tip,score","max_depth":5,"limit":60}
{"op":"list_so"}
{"op":"set_so_prop","name":"//example_so","prop":"coins","value":999}
```

Use `flash_mod_rpc` with a JSON string when you need an op not wrapped by a dedicated tool.

## AI skills

| Skill | Role |
|--------|------|
| [`skills/flash-mod-bridge`](skills/flash-mod-bridge/SKILL.md) | Tool order, paths, Flashpoint, limits |

```powershell
Copy-Item ".\skills\flash-mod-bridge" "$env:USERPROFILE\.cursor\skills\flash-mod-bridge" -Recurse -Force
```

## Related projects (same suite)

| Need | Repo |
|------|------|
| Live **Flash / Ruffle** (this) | [flash-mod-bridge](https://github.com/rkuhn153/flash-mod-bridge) |
| Live **Unity** get/set/patch | [bepinex-mcp](https://github.com/rkuhn153/bepinex-mcp) |
| Live **Unreal runtime** (UE4SS) | [unreal-engine-mcp](https://github.com/rkuhn153/unreal-engine-mcp) |
| Mono C# search | [gamecode-rag](https://github.com/rkuhn153/gamecode-rag) |
| IL2CPP static decompile | [il2cpp-decompiler](https://github.com/rkuhn153/il2cpp-decompiler) |

## Limits

- **Stock Ruffle** has no `modBridgeRpc` — must use prebuilt or patched build.  
- Content must **run in Ruffle**; bridge quality follows emulator support.  
- **AwayFL** / some Coolmath hosts need rehost on the fork.  
- Bad paths or spammy set/call can still upset a SWF.  
- Nested object writes are more limited than simple primitives.

## Layout

```text
flash-mod-bridge/
  run_mcp.py              # MCP entry
  translator/             # FastMCP + HTTP hub
  inject/                 # Chrome extension agent
  host/                   # Simple self-host page
  engine/                 # Ruffle patch sources
  flashpoint/             # Install/uninstall pin scripts
  skills/flash-mod-bridge/
  samples/
  requirements.txt
  README.md
  LICENSE
```

## License

- MCP, inject, host, samples, scripts: **MIT** — see [LICENSE](LICENSE).  
- Ruffle engine: **MIT / Apache-2.0** (upstream). Patches in `engine/` are intended for that tree.  
- Prebuilt `ruffle_desktop.exe` includes our mod-bridge patches on top of Ruffle.
