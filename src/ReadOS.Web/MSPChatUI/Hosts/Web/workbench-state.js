(function () {
  const listeners = new Set();
  const state = {
    sessions: [],
    activeSessionId: null,
    timeline: null,
    busy: false,
    connected: false,
    capabilities: null,
    turnMode: "command"
  };

  function emit() {
    for (const listener of listeners) listener(snapshot());
  }

  function snapshot() {
    return {
      ...state,
      sessions: [...state.sessions],
      timeline: state.timeline ? structuredClone(state.timeline) : null
    };
  }

  function update(values) {
    Object.assign(state, values);
    emit();
  }

  function upsertSession(session) {
    const sessions = state.sessions.filter((item) => item.id !== session.id);
    sessions.unshift(session);
    update({ sessions });
  }

  window.ReadOSWorkbenchState = Object.freeze({
    subscribe(listener) {
      listeners.add(listener);
      listener(snapshot());
      return () => listeners.delete(listener);
    },
    update,
    upsertSession,
    get: snapshot
  });
})();
