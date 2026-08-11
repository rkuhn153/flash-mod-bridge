"""FastMCP tools for Flash Ruffle mod bridge (hub :8767)."""

from __future__ import annotations

import logging
import threading
from typing import Any

import uvicorn
from fastmcp import FastMCP

from translator.agent_hub import AgentHub, create_app

import os

logger = logging.getLogger("flash-mod-mcp")
HOST = os.environ.get("FLASH_MOD_BRIDGE_HOST", "127.0.0.1")
PORT = int(os.environ.get("FLASH_MOD_BRIDGE_PORT", "8767"))

_hub = AgentHub()
mcp = FastMCP("flash-mod-bridge")


def _hub_thread() -> None:
    app = create_app(_hub)
    uvicorn.run(app, host=HOST, port=PORT, log_level="warning")


async def _rpc(method: str, params: dict[str, Any] | None = None, timeout: float = 20.0) -> Any:
    return await _hub.call(method, params or {}, timeout=timeout)


@mcp.tool
async def ping_flash_bridge() -> dict[str, Any]:
    """Check hub + browser agent and/or standalone desktop Ruffle (:8768)."""
    snap = _hub.snapshot()
    live = None
    probe_error = None
    try:
        live = await _rpc("ping", {}, timeout=8.0)
    except Exception as exc:
        probe_error = str(exc)
    desktop = snap.get("desktop")
    hint = None
    if not live and not (desktop and desktop.get("has_player")):
        hint = (
            "No player. For Flashpoint: install forked ruffle.exe "
            "(flashpoint/install-to-flashpoint.ps1), launch a game, then retry. "
            "Desktop bridge: http://127.0.0.1:8768/health"
        )
    return {
        "ok": True,
        "protocol": snap.get("protocol"),
        "hub": f"http://{HOST}:{PORT}",
        "agent": snap,
        "live_probe": live,
        "probe_error": probe_error,
        "hint": hint,
    }


@mcp.tool
async def list_connected_agents() -> dict[str, Any]:
    """List connected Flash/Ruffle page frames."""
    return {"ok": True, **_hub.snapshot()}


@mcp.tool
async def flash_mod_rpc(request_json: str) -> dict[str, Any]:
    """Raw mod-bridge JSON RPC string (same ops as Player.modBridgeRpc)."""
    if not (request_json or "").strip():
        return {"ok": False, "error": "request_json required"}
    try:
        result = await _rpc("mod_bridge_rpc", {"request_json": request_json}, timeout=25.0)
        return {"ok": True, "result": result}
    except Exception as exc:
        return {"ok": False, "error": str(exc)}


@mcp.tool
async def flash_ping() -> dict[str, Any]:
    """Movie / stage / SharedObject summary via mod bridge."""
    try:
        return {"ok": True, "result": await _rpc("mod_bridge", {"op": "ping"}, timeout=15.0)}
    except Exception as exc:
        return {"ok": False, "error": str(exc)}


@mcp.tool
async def flash_list_display(max_depth: int = 3, limit: int = 100) -> dict[str, Any]:
    """Walk the display list (names, classes, paths)."""
    try:
        return {
            "ok": True,
            "result": await _rpc(
                "mod_bridge",
                {"op": "list_display", "max_depth": max_depth, "limit": limit},
                timeout=20.0,
            ),
        }
    except Exception as exc:
        return {"ok": False, "error": str(exc)}


@mcp.tool
async def flash_find(
    keywords: str = "money,tip,score,coin,gold,hp,day,rank,allmoney",
    max_depth: int = 5,
    limit: int = 60,
) -> dict[str, Any]:
    """Search display tree + SharedObjects for property names matching keywords."""
    try:
        return {
            "ok": True,
            "result": await _rpc(
                "mod_bridge",
                {
                    "op": "find",
                    "keywords": keywords,
                    "max_depth": max_depth,
                    "limit": limit,
                },
                timeout=30.0,
            ),
        }
    except Exception as exc:
        return {"ok": False, "error": str(exc)}


@mcp.tool
async def flash_get(path: str) -> dict[str, Any]:
    """Get an AS3 path (e.g. root.x or so:name|allmoney)."""
    try:
        return {
            "ok": True,
            "result": await _rpc("mod_bridge", {"op": "get", "path": path}, timeout=15.0),
        }
    except Exception as exc:
        return {"ok": False, "error": str(exc)}


@mcp.tool
async def flash_set(path: str, value_json: str) -> dict[str, Any]:
    """Set a primitive AS3 property live. value_json: '9999', 'true', '\"name\"'."""
    import json as _json

    try:
        value = _json.loads(value_json)
    except Exception as exc:
        return {"ok": False, "error": f"value_json parse: {exc}"}
    try:
        return {
            "ok": True,
            "result": await _rpc(
                "mod_bridge",
                {"op": "set", "path": path, "value": value},
                timeout=15.0,
            ),
        }
    except Exception as exc:
        return {"ok": False, "error": str(exc)}


@mcp.tool
async def flash_call(path: str, method: str, args_json: str = "[]") -> dict[str, Any]:
    """Call a public AS3 method on path. args_json is a JSON array."""
    import json as _json

    try:
        args = _json.loads(args_json)
        if not isinstance(args, list):
            return {"ok": False, "error": "args_json must be a JSON array"}
    except Exception as exc:
        return {"ok": False, "error": f"args_json parse: {exc}"}
    try:
        return {
            "ok": True,
            "result": await _rpc(
                "mod_bridge",
                {"op": "call", "path": path, "method": method, "args": args},
                timeout=20.0,
            ),
        }
    except Exception as exc:
        return {"ok": False, "error": str(exc)}


@mcp.tool
async def flash_list_so() -> dict[str, Any]:
    """List live SharedObject names in the player."""
    try:
        return {
            "ok": True,
            "result": await _rpc("mod_bridge", {"op": "list_so"}, timeout=10.0),
        }
    except Exception as exc:
        return {"ok": False, "error": str(exc)}


@mcp.tool
async def flash_set_so_prop(name: str, prop: str, value_json: str) -> dict[str, Any]:
    """Set a SharedObject data property live (no reload)."""
    import json as _json

    try:
        value = _json.loads(value_json)
    except Exception as exc:
        return {"ok": False, "error": f"value_json parse: {exc}"}
    try:
        return {
            "ok": True,
            "result": await _rpc(
                "mod_bridge",
                {"op": "set_so_prop", "name": name, "prop": prop, "value": value},
                timeout=15.0,
            ),
        }
    except Exception as exc:
        return {"ok": False, "error": str(exc)}


@mcp.tool
async def flash_list_props(path: str = "root", limit: int = 80) -> dict[str, Any]:
    """List public/dynamic properties on an object path."""
    try:
        return {
            "ok": True,
            "result": await _rpc(
                "mod_bridge",
                {"op": "list_props", "path": path, "limit": limit},
                timeout=20.0,
            ),
        }
    except Exception as exc:
        return {"ok": False, "error": str(exc)}


def main() -> None:
    t = threading.Thread(target=_hub_thread, name="flash-mod-hub", daemon=True)
    t.start()
    logger.info("Flash mod-bridge hub on http://%s:%s", HOST, PORT)
    mcp.run()


if __name__ == "__main__":
    main()
