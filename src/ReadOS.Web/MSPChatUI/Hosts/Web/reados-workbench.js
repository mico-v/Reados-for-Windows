(function () {
  const api = window.ReadOSWorkbenchApi;
  const store = window.ReadOSWorkbenchState;
  const elements = Object.fromEntries([
    "session-list", "new-session", "delete-session", "conversation-title",
    "conversation-subtitle", "empty-state", "composer-input", "composer-cwd",
    "send-turn", "turn-status", "runtime-badge", "runtime-name", "workspace-kind",
    "policy-kind", "audit-kind", "chat-kind", "capability-list", "theme-toggle", "sidebar-toggle",
    "conversation-scroll"
  ].map((id) => [id, document.getElementById(id)]));

  function waitForRenderer(timeout = 10000) {
    if (window.MSPChatUIDefaultRenderer?.renderTimeline) return Promise.resolve();
    return new Promise((resolve, reject) => {
      const started = performance.now();
      const poll = () => {
        if (window.MSPChatUIDefaultRenderer?.renderTimeline) return resolve();
        if (performance.now() - started > timeout) return reject(new Error("消息渲染器未就绪"));
        window.setTimeout(poll, 25);
      };
      poll();
    });
  }

  function renderSessions(state) {
    elements["session-list"].replaceChildren(...state.sessions.map((session) => {
      const button = document.createElement("button");
      button.type = "button";
      button.className = `session-item${session.id === state.activeSessionId ? " active" : ""}`;
      const title = document.createElement("strong");
      title.textContent = session.title || "新对话";
      const detail = document.createElement("small");
      detail.textContent = `${session.message_count || 0} 条消息`;
      button.append(title, detail);
      button.addEventListener("click", () => openSession(session.id));
      return button;
    }));
  }

  async function renderTimeline(timeline) {
    const hasMessages = Boolean(timeline?.messages?.length);
    elements["empty-state"].classList.toggle("hidden", hasMessages);
    if (!hasMessages) {
      document.getElementById("messages").replaceChildren();
      return;
    }
    await waitForRenderer();
    const themedTimeline = structuredClone(timeline);
    themedTimeline.presentation = {
      ...(themedTimeline.presentation || {}),
      theme: document.documentElement.dataset.theme || "light"
    };
    await window.MSPChatUIDefaultRenderer.renderTimeline(themedTimeline);
    requestAnimationFrame(() => {
      elements["conversation-scroll"].scrollLeft = 0;
      elements["conversation-scroll"].scrollTop = elements["conversation-scroll"].scrollHeight;
    });
  }

  function renderState(state) {
    renderSessions(state);
    const active = state.sessions.find((session) => session.id === state.activeSessionId);
    elements["conversation-title"].textContent = active?.title || "新对话";
    elements["conversation-subtitle"].textContent = state.connected
      ? "Rust MSP · 虚拟工作区 · Host 审计"
      : "后端未连接";
    elements["send-turn"].disabled = state.busy || !state.activeSessionId;
    document.querySelectorAll("[data-mode]").forEach((button) => {
      button.classList.toggle("active", button.dataset.mode === state.turnMode);
    });
    elements["composer-cwd"].closest(".cwd-control").hidden = state.turnMode !== "command";
    elements["composer-input"].placeholder = state.turnMode === "chat"
      ? "向 ReadOS 提问，Enter 发送，Shift+Enter 换行"
      : "输入 MSP 命令，Enter 发送，Shift+Enter 换行";
    renderTimeline(state.timeline).catch(showError);
  }

  async function refreshSessions(preferredId) {
    const payload = await api.sessions();
    let sessions = payload.sessions || [];
    let activeId = preferredId || store.get().activeSessionId || sessions[0]?.id;
    if (!activeId) {
      const created = await api.createSession();
      sessions = [created.session];
      activeId = created.session.id;
    }
    store.update({ sessions, activeSessionId: activeId });
    await openSession(activeId, false);
  }

  async function openSession(id, closeSidebar = true) {
    if (store.get().busy) return;
    const payload = await api.session(id);
    store.upsertSession(payload.session);
    store.update({ activeSessionId: id, timeline: payload.timeline });
    if (closeSidebar) document.body.classList.remove("sidebar-open");
  }

  async function createSession() {
    const payload = await api.createSession();
    store.upsertSession(payload.session);
    store.update({ activeSessionId: payload.session.id, timeline: payload.timeline || null });
    elements["composer-input"].focus();
    document.body.classList.remove("sidebar-open");
  }

  async function deleteSession() {
    const id = store.get().activeSessionId;
    if (!id || !window.confirm("删除当前本地会话？")) return;
    await api.deleteSession(id);
    store.update({ activeSessionId: null, timeline: null });
    await refreshSessions();
  }

  async function sendTurn() {
    const state = store.get();
    const input = elements["composer-input"].value.trim();
    const cwd = elements["composer-cwd"].value.trim() || "/workspace";
    if (!input || !state.activeSessionId || state.busy) return;
    store.update({ busy: true });
    elements["turn-status"].textContent = "正在通过 Rust MSP 执行…";
    elements["composer-input"].value = "";
    resizeComposer();
    try {
      const payload = await api.sendTurn(state.activeSessionId, input, cwd, state.turnMode);
      store.upsertSession(payload.session);
      store.update({ timeline: payload.timeline, busy: false });
      elements["turn-status"].textContent = payload.mode === "chat"
        ? "AI 回复完成"
        : payload.audit?.count === 1
        ? `完成 · exit ${payload.exit_code} · 1 条终态审计`
        : `完成 · exit ${payload.exit_code}`;
    } catch (error) {
      store.update({ busy: false });
      elements["composer-input"].value = input;
      showError(error);
    }
  }

  function resizeComposer() {
    const input = elements["composer-input"];
    input.style.height = "auto";
    input.style.height = `${Math.min(input.scrollHeight, 180)}px`;
  }

  function showError(error) {
    elements["turn-status"].textContent = error instanceof Error ? error.message : String(error);
  }

  async function bootstrap() {
    store.subscribe(renderState);
    try {
      const [health, capabilities] = await Promise.all([api.health(), api.capabilities()]);
      store.update({ connected: true, capabilities });
      elements["runtime-badge"].textContent = "已连接";
      elements["runtime-badge"].className = "status-badge ready";
      elements["runtime-name"].textContent = health.runtime;
      elements["workspace-kind"].textContent = capabilities.workspace;
      elements["policy-kind"].textContent = capabilities.policy;
      elements["audit-kind"].textContent = capabilities.audit;
      elements["chat-kind"].textContent = capabilities.chat;
      elements["capability-list"].replaceChildren(...capabilities.commands.map((command) => {
        const chip = document.createElement("span");
        chip.textContent = command;
        return chip;
      }));
      await refreshSessions();
      if (capabilities.chat !== "not-configured") store.update({ turnMode: "chat" });
    } catch (error) {
      elements["runtime-badge"].textContent = "连接失败";
      elements["runtime-badge"].className = "status-badge error";
      showError(error);
    }
  }

  elements["new-session"].addEventListener("click", () => createSession().catch(showError));
  elements["delete-session"].addEventListener("click", () => deleteSession().catch(showError));
  elements["send-turn"].addEventListener("click", sendTurn);
  elements["composer-input"].addEventListener("input", resizeComposer);
  elements["composer-input"].addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      sendTurn();
    }
  });
  elements["sidebar-toggle"].addEventListener("click", () => document.body.classList.toggle("sidebar-open"));
  elements["theme-toggle"].addEventListener("click", () => {
    const dark = document.documentElement.dataset.theme !== "dark";
    document.documentElement.dataset.theme = dark ? "dark" : "light";
    localStorage.setItem("reados.web.theme", dark ? "dark" : "light");
    renderTimeline(store.get().timeline).catch(showError);
  });
  document.querySelectorAll("[data-command]").forEach((button) => button.addEventListener("click", () => {
    store.update({ turnMode: "command" });
    elements["composer-input"].value = button.dataset.command;
    resizeComposer();
    elements["composer-input"].focus();
  }));
  document.querySelectorAll("[data-mode]").forEach((button) => button.addEventListener("click", () => {
    if (button.dataset.mode === "chat" && store.get().capabilities?.chat === "not-configured") {
      showError("AI 未配置：请设置 READOS_AI_API_KEY 后重启 Host");
      return;
    }
    store.update({ turnMode: button.dataset.mode });
    elements["composer-input"].focus();
  }));
  document.documentElement.dataset.theme = localStorage.getItem("reados.web.theme") || "light";
  bootstrap();
})();
