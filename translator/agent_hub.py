"""HTTP hub for Flash/Ruffle page agents (port 8767).

Also falls back to standalone desktop Ruffle mod-bridge on :8768
(Flashpoint / forked ruffle.exe).
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
import time
import urllib.error
import urllib.request
import uuid
from dataclasses import dataclass, field
from typing import Any

from fastapi import FastAPI, WebSocket, WebSocketDisconnect
from fastapi.middleware.cors import CORSMiddleware

logger = logging.getLogger("flash-mod-hub")
PROTOCOL = "1.0.0"
DESKTOP_BRIDGE = os.environ.get("FLASH_MOD_DESKTOP_URL", "http://127.0.0.1:8768")
# Serialize desktop RPCs — parallel floods previously crashed the player.
_desktop_rpc_lock = __import__("threading").Lock()


@dataclass
class AgentState:
    agent_id: str
    page_url: str = ""
    title: str = ""
    has_ruffle: bool = False
    has_mod_bridge: bool = False
    last_seen: float = 0.0
    transport: str = "http"
    caps: dict[str, Any] = field(default_factory=dict)
    last_error: str = ""


@dataclass
class LiveAgent:
    state: AgentState
    ws: WebSocket | None = None


class PendingCall:
    def __init__(self) -> None:
        self.event = asyncio.Event()
        self.result: Any = None
        self.error: str | None = None
        self.done = False


class AgentHub:
    def __init__(self) -> None:
        self._agents: dict[str, LiveAgent] = {}
        self._pending: dict[str, PendingCall] = {}
        self._http_jobs: asyncio.Queue[dict[str, Any]] = asyncio.Queue()

    def _prune(self, max_age: float = 30.0) -> None:
        now = time.time()
        for k in [k for k, a in self._agents.items() if now - a.state.last_seen > max_age]:
            self._agents.pop(k, None)

    def best_agent(self) -> LiveAgent | None:
        self._prune()
        if not self._agents:
            return None
        return max(
            self._agents.values(),
            key=lambda a: (
                1 if a.state.has_mod_bridge else 0,
                1 if a.state.has_ruffle else 0,
                a.state.last_seen,
            ),
        )

    def snapshot(self) -> dict[str, Any]:
        self._prune()
        best = self.best_agent()
        agents = [
            {
                "agent_id": a.state.agent_id,
                "page_url": a.state.page_url,
                "title": a.state.title,
                "has_ruffle": a.state.has_ruffle,
                "has_mod_bridge": a.state.has_mod_bridge,
                "last_seen": a.state.last_seen,
                "age_sec": time.time() - a.state.last_seen if a.state.last_seen else None,
                "transport": a.state.transport,
            }
            for a in self._agents.values()
        ]
        agents.sort(key=lambda x: (x["has_mod_bridge"], x["has_ruffle"]), reverse=True)
        desktop = self.desktop_health()
        s = best.state if best else AgentState(agent_id="")
        connected = bool(self._agents) or bool(desktop and desktop.get("ok"))
        return {
            "protocol": PROTOCOL,
            "connected": connected,
            "agent_count": len(self._agents),
            "agents": agents,
            "page_url": s.page_url,
            "title": s.title or (desktop.get("title") if desktop else ""),
            "has_ruffle": s.has_ruffle or bool(desktop),
            "has_mod_bridge": s.has_mod_bridge
            or bool(desktop and desktop.get("mod_bridge")),
            "desktop": desktop,
            "desktop_url": DESKTOP_BRIDGE,
            "last_seen": s.last_seen,
            "age_sec": (time.time() - s.last_seen) if s.last_seen else None,
            "preferred_agent_id": best.state.agent_id
            if best
            else ("desktop:8768" if desktop else None),
            "capabilities": s.caps,
            "last_error": s.last_error,
            "transport": (
                "multi"
                if len(self._agents) > 1
                else (
                    s.transport
                    if best
                    else ("desktop-http" if desktop else "none")
                )
            ),
        }

    async def register_hello(self, payload: dict[str, Any], ws: WebSocket | None = None) -> str:
        key = str(payload.get("page_url") or payload.get("url") or "unknown")
        st = AgentState(
            agent_id=key,
            page_url=key,
            title=str(payload.get("title") or ""),
            has_ruffle=bool(payload.get("has_ruffle")),
            has_mod_bridge=bool(payload.get("has_mod_bridge")),
            last_seen=time.time(),
            transport="websocket" if ws is not None else "http-poll",
            caps=dict(payload.get("capabilities") or {}),
            last_error=str(payload.get("error") or ""),
        )
        existing = self._agents.get(key)
        if existing and ws is None and existing.ws is not None:
            existing.state = st
            existing.state.transport = "websocket+http"
        else:
            self._agents[key] = LiveAgent(
                state=st, ws=ws if ws is not None else (existing.ws if existing else None)
            )
        return key

    async def handle_agent_message(self, data: dict[str, Any], ws: WebSocket | None = None) -> dict[str, Any] | None:
        typ = data.get("type") or data.get("op")
        if typ in ("hello", "heartbeat", "status"):
            aid = await self.register_hello(data.get("payload") or data, ws=ws)
            return {"type": "hello_ack", "protocol": PROTOCOL, "ok": True, "agent_id": aid}
        if typ in ("result", "rpc_result"):
            call_id = str(data.get("id") or data.get("call_id") or "")
            pending = self._pending.get(call_id)
            if pending and not pending.done:
                if data.get("ok", True) is False:
                    pending.error = str(data.get("error") or "agent error")
                else:
                    pending.result = data.get("result")
                pending.done = True
                pending.event.set()
            return {"type": "result_ack", "id": call_id}
        return {"type": "noop"}

    def desktop_health(self) -> dict[str, Any] | None:
        """Probe standalone Ruffle mod-bridge (:8768)."""
        try:
            req = urllib.request.Request(
                f"{DESKTOP_BRIDGE.rstrip('/')}/health",
                method="GET",
                headers={"Accept": "application/json"},
            )
            with urllib.request.urlopen(req, timeout=1.5) as resp:
                raw = resp.read().decode("utf-8", errors="replace")
                return json.loads(raw)
        except Exception:
            return None

    def call_desktop(self, method: str, params: dict[str, Any] | None = None) -> Any:
        """Map hub methods onto desktop POST /rpc (serialized)."""
        with _desktop_rpc_lock:
            return self._call_desktop_unlocked(method, params or {})

    def _call_desktop_unlocked(self, method: str, params: dict[str, Any]) -> Any:
        if method in ("ping",):
            body: Any = {"op": "ping"}
        elif method == "mod_bridge_rpc":
            body_raw = params.get("request_json") or "{}"
            if isinstance(body_raw, str):
                try:
                    body = json.loads(body_raw)
                except json.JSONDecodeError:
                    body = body_raw
            else:
                body = body_raw
            if isinstance(body, str):
                payload = body.encode("utf-8")
                req = urllib.request.Request(
                    f"{DESKTOP_BRIDGE.rstrip('/')}/rpc",
                    data=payload,
                    method="POST",
                    headers={"Content-Type": "application/json"},
                )
                with urllib.request.urlopen(req, timeout=25) as resp:
                    raw = resp.read().decode("utf-8", errors="replace")
                try:
                    return json.loads(raw)
                except json.JSONDecodeError:
                    return {"ok": False, "error": "non-json", "raw": raw[:500]}
        elif method == "mod_bridge":
            body = dict(params)
            if "op" not in body:
                raise RuntimeError("mod_bridge requires op")
        else:
            body = dict(params)
            body.setdefault("op", method)

        payload = json.dumps(body).encode("utf-8")
        req = urllib.request.Request(
            f"{DESKTOP_BRIDGE.rstrip('/')}/rpc",
            data=payload,
            method="POST",
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=25) as resp:
            raw = resp.read().decode("utf-8", errors="replace")
        try:
            return json.loads(raw)
        except json.JSONDecodeError:
            return {"ok": False, "error": "non-json", "raw": raw[:500]}

    async def call(self, method: str, params: dict[str, Any] | None = None, timeout: float = 20.0) -> Any:
        self._prune()
        params = params or {}

        # Prefer browser page agent when present
        if self._agents:
            call_id = str(uuid.uuid4())
            msg = {
                "type": "rpc",
                "id": call_id,
                "method": method,
                "params": params,
            }
            pending = PendingCall()
            self._pending[call_id] = pending
            try:
                best = self.best_agent()
                agents = [best] if best else list(self._agents.values())[:1]
                msg["target_urls"] = [a.state.page_url for a in agents if a]
                for a in agents:
                    if a and a.ws is not None:
                        try:
                            await a.ws.send_text(json.dumps(msg))
                        except Exception as exc:
                            logger.warning("ws send failed: %s", exc)
                for _ in range(max(3, len(self._agents) + 1)):
                    await self._http_jobs.put(msg)
                try:
                    await asyncio.wait_for(pending.event.wait(), timeout=timeout)
                except asyncio.TimeoutError:
                    # Fall through to desktop
                    logger.warning("page agent timeout for %s — trying desktop", method)
                else:
                    if pending.error:
                        raise RuntimeError(pending.error)
                    return pending.result
            finally:
                self._pending.pop(call_id, None)

        # Standalone / Flashpoint desktop Ruffle
        try:
            return await asyncio.to_thread(self.call_desktop, method, params)
        except urllib.error.URLError as exc:
            raise RuntimeError(
                "No Flash agents and desktop Ruffle not reachable at "
                f"{DESKTOP_BRIDGE}. Launch a game in forked ruffle.exe "
                "(Flashpoint with mod-bridge install) or load the Chrome inject on a web player."
            ) from exc
        except Exception as exc:
            raise RuntimeError(f"desktop mod-bridge failed: {exc}") from exc

    async def http_poll_job(self) -> dict[str, Any] | None:
        try:
            return self._http_jobs.get_nowait()
        except asyncio.QueueEmpty:
            return None


def create_app(hub: AgentHub) -> FastAPI:
    app = FastAPI(title="Flash Mod Bridge Hub")
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],
        allow_methods=["*"],
        allow_headers=["*"],
    )

    @app.get("/health")
    async def health() -> dict[str, Any]:
        return {"ok": True, **hub.snapshot()}

    @app.post("/agent/hello")
    async def agent_hello(body: dict[str, Any]) -> dict[str, Any]:
        aid = await hub.register_hello(body.get("payload") or body, ws=None)
        return {"ok": True, "protocol": PROTOCOL, "agent_id": aid}

    @app.get("/agent/poll")
    async def agent_poll() -> dict[str, Any]:
        job = await hub.http_poll_job()
        return {"job": job}

    @app.post("/agent/result")
    async def agent_result(body: dict[str, Any]) -> dict[str, Any]:
        await hub.handle_agent_message({**body, "type": body.get("type") or "result"})
        return {"ok": True}

    @app.post("/rpc")
    async def rpc(body: dict[str, Any]) -> dict[str, Any]:
        method = str(body.get("method") or "").strip()
        if not method:
            return {"ok": False, "error": "method required"}
        params = body.get("params") if isinstance(body.get("params"), dict) else {}
        timeout = float(body.get("timeout") or 25.0)
        try:
            result = await hub.call(method, params, timeout=timeout)
            return {"ok": True, "result": result}
        except Exception as exc:
            return {"ok": False, "error": str(exc), "hub": hub.snapshot()}

    @app.websocket("/ws")
    async def ws_agent(ws: WebSocket) -> None:
        await ws.accept()
        agent_key: str | None = None
        try:
            while True:
                raw = await ws.receive_text()
                try:
                    data = json.loads(raw)
                except json.JSONDecodeError:
                    continue
                if (data.get("type") or "") in ("hello", "heartbeat", "status"):
                    payload = data.get("payload") or data
                    agent_key = await hub.register_hello(payload, ws=ws)
                    await ws.send_text(
                        json.dumps(
                            {
                                "type": "hello_ack",
                                "protocol": PROTOCOL,
                                "ok": True,
                                "agent_id": agent_key,
                            }
                        )
                    )
                    continue
                reply = await hub.handle_agent_message(data, ws=ws)
                if reply:
                    await ws.send_text(json.dumps(reply))
        except WebSocketDisconnect:
            pass
        finally:
            if agent_key and agent_key in hub._agents:
                a = hub._agents[agent_key]
                if a.ws is ws:
                    a.ws = None
                    a.state.transport = "http-poll"

    return app
