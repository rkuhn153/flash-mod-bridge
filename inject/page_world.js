/* MAIN world — finds forked Ruffle instances and exposes mod bridge helpers. */
(function () {
  const SOURCE = "flash-mod-bridge-page";
  const REPLY = "flash-mod-bridge-page-reply";

  function findPlayers() {
    const found = [];
    // Custom elements / embeds from Ruffle
    document.querySelectorAll("ruffle-player, ruffle-embed, ruffle-object").forEach((el, i) => {
      found.push({ el, tag: el.tagName, index: i });
    });
    // Global registries some builds expose
    if (window.RufflePlayer && window.RufflePlayer.newest) {
      try {
        // no direct instance list — DOM is primary
      } catch (_) {}
    }
    return found;
  }

  function getCore(el) {
    // Ruffle web components stash wasm handle differently across versions.
    // Prefer public methods we added: modBridgeRpc / modBridge
    if (el && typeof el.modBridgeRpc === "function") return el;
    if (el && el.ruffle && typeof el.ruffle.modBridgeRpc === "function") return el.ruffle;
    if (el && el._native_player && typeof el._native_player.modBridgeRpc === "function") {
      return el._native_player;
    }
    // shadow / internal
    try {
      if (el && el.shadowRoot) {
        const canvas = el.shadowRoot.querySelector("canvas");
        if (canvas && canvas.__ruffle_handle && typeof canvas.__ruffle_handle.modBridgeRpc === "function") {
          return canvas.__ruffle_handle;
        }
      }
    } catch (_) {}
    return null;
  }

  function pickPlayer() {
    const list = findPlayers();
    for (const item of list) {
      const core = getCore(item.el);
      if (core) return { core, el: item.el, tag: item.tag };
    }
    // last resort: walk window for handles with modBridgeRpc
    try {
      for (const k of Object.keys(window)) {
        const v = window[k];
        if (v && typeof v.modBridgeRpc === "function") {
          return { core: v, el: null, tag: "window." + k };
        }
      }
    } catch (_) {}
    return null;
  }

  function parseRpcResult(raw) {
    if (raw == null) return { ok: false, error: "null result" };
    if (typeof raw === "object") return raw;
    try {
      return JSON.parse(String(raw));
    } catch (e) {
      return { ok: false, error: "non-json: " + String(raw).slice(0, 200) };
    }
  }

  const handlers = {
    ping: function () {
      const p = pickPlayer();
      const has = !!(p && p.core);
      let live = null;
      if (has) {
        try {
          live = parseRpcResult(p.core.modBridgeRpc(JSON.stringify({ op: "ping" })));
        } catch (e) {
          live = { ok: false, error: String(e) };
        }
      }
      return {
        ok: true,
        href: location.href,
        title: document.title,
        has_ruffle: findPlayers().length > 0 || has,
        has_mod_bridge: has,
        player_tag: p && p.tag,
        live: live,
      };
    },
    mod_bridge_rpc: function (p) {
      const player = pickPlayer();
      if (!player || !player.core) {
        return { ok: false, error: "no Ruffle modBridgeRpc on page — use forked selfhosted build" };
      }
      const raw = player.core.modBridgeRpc(String(p.request_json || "{}"));
      return parseRpcResult(raw);
    },
    mod_bridge: function (p) {
      const player = pickPlayer();
      if (!player || !player.core) {
        return { ok: false, error: "no Ruffle modBridgeRpc on page" };
      }
      const op = String(p.op || "");
      const fields = Object.assign({}, p);
      delete fields.op;
      if (typeof player.core.modBridge === "function") {
        return parseRpcResult(player.core.modBridge(op, fields));
      }
      const req = Object.assign({ op: op }, fields);
      return parseRpcResult(player.core.modBridgeRpc(JSON.stringify(req)));
    },
  };

  function reply(id, ok, result, error) {
    window.postMessage(
      {
        source: REPLY,
        id: id,
        ok: ok,
        result: result,
        error: error,
      },
      "*"
    );
  }

  window.addEventListener("message", function (ev) {
    const d = ev.data;
    if (!d || d.source !== "flash-mod-bridge-cs" || d.target !== SOURCE) return;
    const id = d.id;
    const method = d.method;
    const params = d.params || {};
    try {
      if (!handlers[method]) throw new Error("unknown method: " + method);
      const result = handlers[method](params);
      reply(id, true, result, null);
    } catch (e) {
      reply(id, false, null, String((e && e.message) || e));
    }
  });

  window.__FlashModBridgePageWorld = true;
  window.postMessage({ source: REPLY, type: "page_world_ready" }, "*");
  console.info("[FlashModBridge] page world ready", location.href);
})();
