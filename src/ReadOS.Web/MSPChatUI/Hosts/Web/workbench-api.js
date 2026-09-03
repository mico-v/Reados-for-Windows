(function () {
  const apiBase = new URLSearchParams(window.location.search).get("api") || window.location.origin;

  async function request(path, options = {}) {
    const response = await fetch(`${apiBase}${path}`, {
      ...options,
      headers: { "content-type": "application/json", ...(options.headers || {}) }
    });
    const payload = await response.json().catch(() => ({ error: `HTTP ${response.status}` }));
    if (!response.ok) throw new Error(payload.error || `HTTP ${response.status}`);
    return payload;
  }

  window.ReadOSWorkbenchApi = Object.freeze({
    apiBase,
    health: () => request("/api/v1/health"),
    capabilities: () => request("/api/v1/capabilities"),
    sessions: () => request("/api/v1/sessions"),
    createSession: (title) => request("/api/v1/sessions", {
      method: "POST",
      body: JSON.stringify({ title })
    }),
    session: (id) => request(`/api/v1/sessions/${encodeURIComponent(id)}`),
    deleteSession: (id) => request(`/api/v1/sessions/${encodeURIComponent(id)}`, { method: "DELETE" }),
    sendTurn: (id, input, cwd, mode) => request(`/api/v1/sessions/${encodeURIComponent(id)}/turns`, {
      method: "POST",
      body: JSON.stringify({ input, cwd, mode })
    })
  });
})();
