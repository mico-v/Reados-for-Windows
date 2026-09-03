# ReadOS MSP Web Host

`msp-web-host` is a small loopback HTTP adapter for the portable Rust MSP
runtime. It exists to let the shared `MSPChatUI` browser host use the same
virtual command semantics as native hosts.

The server binds to `127.0.0.1` by default and exposes only:

- `GET /api/v1/health`
- `GET /api/v1/capabilities`
- `POST /api/v1/runtime/execute` with `{ "command": "pwd", "cwd": "/workspace" }`

Requests contain a virtual cwd and command text only. The runtime uses the
portable `reados-portable-msp-v1` registry over an in-memory virtual workspace;
there is no shell, PATH lookup, host path, inherited environment, process, or
filesystem access. Policy is deliberately fail-closed at this adapter until a
product Host supplies an approval bridge; read-only commands are marked
`allow` by the local development host and still receive one terminal audit
record in the response.

Run from the repository root:

```text
cargo run -p msp-web-host -- --bind 127.0.0.1:8787 --root src/ReadOS.Web/MSPChatUI
```

The same process serves the product-facing `Hosts/Web/reados-workbench.html`,
the focused `Hosts/Web/reados-runtime.html` diagnostic, and the renderer
assets, so no second static-file server is required.

Optional OpenAI-compatible conversation mode is configured only in the Host
environment with `READOS_AI_API_KEY`, `READOS_AI_BASE_URL`, and
`READOS_AI_MODEL`. The API key is never exposed through the browser API.

This is a development/browser host, not a remotely exposed production API.
Production deployment must add an authenticated product Host and explicit
policy/approval/audit storage above this adapter.
