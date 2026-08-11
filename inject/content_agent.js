/* Content script — hubs with :8767 and relays RPC into page_world. */
(function () {
  const HUB = "http://127.0.0.1:8767";
  const SOURCE = "flash-mod-bridge-page";
  const REPLY = "flash-mod-bridge-page-reply";
  let alive = true;
  let seq = 0;
  const pending = new Map();

  function injectPageWorld() {
    if (document.documentElement.dataset.flashModBridge) return;
    document.documentElement.dataset.flashModBridge = "1";
    const s = document.createElement("script");
    s.src = chrome.runtime.getURL("page_world.js");
    s.onload = function () {
      s.remove();
    };
    (document.documentElement || document.head).appendChild(s);
  }

  function pageRpc(method, params, timeoutMs) {
    return new Promise(function (resolve, reject) {
      const id = "fmb-" + ++seq + "-" + Date.now();
      const t = setTimeout(function () {
        pending.delete(id);
        reject(new Error("page rpc timeout: " + method));
      }, timeoutMs || 15000);
      pending.set(id, {
        resolve: function (v) {
          clearTimeout(t);
          resolve(v);
        },
        reject: function (e) {
          clearTimeout(t);
          reject(e);
        },
      });
      window.postMessage(
        {
          source: "flash-mod-bridge-cs",
          target: SOURCE,
          id: id,
          method: method,
          params: params || {},
        },
        "*"
      );
    });
  }

  window.addEventListener("message", function (ev) {
    const d = ev.data;
    if (!d || d.source !== REPLY) return;
    if (d.type === "page_world_ready") return;
    const p = pending.get(d.id);
    if (!p) return;
    pending.delete(d.id);
    if (d.ok === false) p.reject(new Error(d.error || "page error"));
    else p.resolve(d.result);
  });

  async function hubFetch(path, method, body) {
    const opts = { method: method || "GET", headers: {} };
    if (body != null) {
      opts.method = method || "POST";
      opts.headers["Content-Type"] = "application/json";
      opts.body = JSON.stringify(body);
    }
    const r = await fetch(HUB + path, opts);
    let json = null;
    try {
      json = await r.json();
    } catch (_) {}
    return { ok: r.ok, json: json, status: r.status };
  }

  async function buildHello() {
    let has_ruffle = false;
    let has_mod_bridge = false;
    try {
      const probe = await pageRpc("ping", {}, 4000);
      has_ruffle = !!(probe && probe.has_ruffle);
      has_mod_bridge = !!(probe && probe.has_mod_bridge);
    } catch (_) {
      has_ruffle = !!document.querySelector("ruffle-player, ruffle-embed, ruffle-object, canvas");
    }
    return {
      page_url: location.href,
      title: document.title || "",
      has_ruffle: has_ruffle,
      has_mod_bridge: has_mod_bridge,
      capabilities: {
        mod_bridge: true,
        eval_js: false,
      },
    };
  }

  async function sendHello() {
    if (!alive) return;
    try {
      const payload = await buildHello();
      await hubFetch("/agent/hello", "POST", { type: "hello", payload: payload });
    } catch (_) {}
  }

  async function handleJob(job) {
    if (!job || !job.method) return;
    const targets = job.target_urls || [];
    if (targets.length) {
      const href = location.href || "";
      const hit = targets.some(function (u) {
        if (!u) return false;
        if (u === href) return true;
        try {
          const a = u.split("#")[0].split("?")[0].replace(/\/$/, "");
          const b = href.split("#")[0].split("?")[0].replace(/\/$/, "");
          return a === b;
        } catch (_) {
          return false;
        }
      });
      if (!hit) return;
    }
    const method = job.method;
    const params = job.params || {};
    const id = job.id;
    try {
      const result = await pageRpc(method, params, 20000);
      await hubFetch("/agent/result", "POST", {
        type: "result",
        id: id,
        ok: true,
        result: result,
      });
    } catch (e) {
      await hubFetch("/agent/result", "POST", {
        type: "result",
        id: id,
        ok: false,
        error: String((e && e.message) || e),
      });
    }
  }

  async function pollOnce() {
    if (!alive) return;
    try {
      const resp = await hubFetch("/agent/poll", "GET", null);
      if (resp && resp.ok && resp.json && resp.json.job) {
        await handleJob(resp.json.job);
      }
    } catch (_) {}
  }

  injectPageWorld();
  sendHello();
  setInterval(sendHello, 4000);
  setInterval(pollOnce, 400);
})();
