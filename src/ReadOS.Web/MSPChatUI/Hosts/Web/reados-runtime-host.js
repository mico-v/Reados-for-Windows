(function () {
  const apiBase = new URLSearchParams(window.location.search).get("api") || "http://127.0.0.1:8787";
  const output = document.getElementById("runtime-output");
  const commandInput = document.getElementById("runtime-command");
  const cwdInput = document.getElementById("runtime-cwd");
  const status = document.getElementById("runtime-status");

  async function request(path, options) {
    const response = await fetch(`${apiBase}${path}`, {
      ...options,
      headers: { "content-type": "application/json", ...(options?.headers || {}) }
    });
    const payload = await response.json();
    if (!response.ok) throw new Error(payload.error || `HTTP ${response.status}`);
    return payload;
  }

  async function execute() {
    const command = commandInput.value.trim();
    const cwd = cwdInput.value.trim() || "/workspace";
    if (!command) return;
    status.textContent = "Running through ReadOS MSP runtime…";
    try {
      const payload = await request("/api/v1/runtime/execute", {
        method: "POST",
        body: JSON.stringify({ command, cwd })
      });
      output.textContent = payload.stderr_base64 ? `exit ${payload.exit_code} · ${payload.diagnostic_code || "failed"}` : `exit ${payload.exit_code}`;
      await waitForRenderer();
      await window.MSPChatUIDefaultRenderer.renderTimeline(payload.timeline);
      status.textContent = payload.audit?.count === 1 ? "Completed · one Host terminal audit" : "Completed";
    } catch (error) {
      output.textContent = error instanceof Error ? error.message : String(error);
      status.textContent = "Backend unavailable";
    }
  }

  function waitForRenderer(timeoutMilliseconds = 10000) {
    if (window.MSPChatUIDefaultRenderer && typeof window.MSPChatUIDefaultRenderer.renderTimeline === "function") {
      return Promise.resolve();
    }
    return new Promise((resolve, reject) => {
      const started = performance.now();
      const poll = () => {
        if (window.MSPChatUIDefaultRenderer && typeof window.MSPChatUIDefaultRenderer.renderTimeline === "function") return resolve();
        if (performance.now() - started > timeoutMilliseconds) return reject(new Error("MSPChatUI renderer is not ready"));
        window.setTimeout(poll, 25);
      };
      poll();
    });
  }

  document.getElementById("runtime-execute").addEventListener("click", execute);
  commandInput.addEventListener("keydown", (event) => {
    if ((event.ctrlKey || event.metaKey) && event.key === "Enter") execute();
  });
  window.MSPReadOSRuntimeWebHost = Object.freeze({ apiBase, execute });
})();
