async function refresh() {
  const out = document.getElementById("out");
  try {
    const r = await fetch("http://127.0.0.1:8767/health");
    const j = await r.json();
    out.textContent =
      "agents=" +
      j.agent_count +
      "\npreferred=" +
      (j.preferred_agent_id || "-") +
      "\nruffle=" +
      j.has_ruffle +
      " bridge=" +
      j.has_mod_bridge +
      "\n" +
      JSON.stringify(j.agents || [], null, 2);
  } catch (e) {
    out.textContent = "Hub offline: " + e;
  }
}
document.getElementById("refresh").onclick = refresh;
document.getElementById("open").onclick = () => {
  chrome.tabs.create({ url: "http://127.0.0.1:8767/health" });
};
refresh();
setInterval(refresh, 3000);
