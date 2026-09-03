# MSPChatUI browser host over the ReadOS Rust runtime

The repository now contains a small `native/msp-web-host` development host.
It connects the product-owned `src/ReadOS.Web/MSPChatUI` Web renderer to
the modular Rust MSP runtime without creating a second command implementation.

```text
Browser (MSPChatUI Default renderer)
        │ POST /api/v1/runtime/execute { command, cwd }
        ▼
msp-web-host (loopback HTTP + Host boundary)
        │ structured virtual request
        ▼
msp-command-runtime
        │ planner → registry → bounded backend
        ▼
msp-command-pack + msp-backend (virtual workspace only)
```

## What is implemented

- `GET /api/v1/health` reports the runtime profile.
- `GET /api/v1/capabilities` reports the portable v1 command allowlist.
- `POST /api/v1/runtime/execute` accepts only a command string, virtual cwd,
  and optional bounded base64 stdin.
- The host serves `/` as the responsive `Hosts/Web/reados-workbench.html`,
  retains `Hosts/Web/reados-runtime.html` as a focused diagnostic, and serves
  renderer assets from an explicitly supplied package root with
  traversal-safe canonicalization.
- The response contains binary-safe stdout/stderr, an exit code, a stable
  diagnostic code, one terminal audit record, and an
  `msp.chat-ui.timeline.v1` payload consumed by the existing renderer.

The development host binds to loopback (`127.0.0.1:8787`) by default. It uses
an in-memory `/workspace` containing only a small README fixture. It does not
open host files, inherit `PATH` or environment values, spawn arbitrary
processes, or expose shell text. The portable runtime rejects shell operators,
host-shaped paths, and unsupported commands.

## Ownership boundary

This is a browser adapter, not a replacement for the product Host:

- Rust owns portable parse/expansion, virtual path rules, command semantics,
  bounded results, and the neutral backend contract.
- The web host owns HTTP framing, request limits, capability projection, and
  terminal response shaping.
- Product policy, approval, durable audit, sessions, artifacts, credentials,
  document/chat providers, and user identity remain above this adapter.

The current development host allows read-only portable commands so that the
browser path can be exercised. A production ReadOS web deployment must inject
an authenticated policy/approval/audit Host and must not bind this listener to
an external interface by default.

## Run

From the repository root:

```text
cargo run -p msp-web-host -- --bind 127.0.0.1:8787 --root src/ReadOS.Web/MSPChatUI
```

Open `http://127.0.0.1:8787/`.

The workbench provides server-side local sessions, session switching and
deletion, an adaptive conversation shell, a multiline composer, runtime
capability inspection, theme switching, and accumulated MSPChatUI timelines.
Sessions are currently process-local and reset when the Host restarts.

### Optional AI conversation mode

The workbench can call an OpenAI-compatible chat-completions provider without
placing credentials in browser storage or HTML. Configure the Host process:

```powershell
$env:READOS_AI_API_KEY = "..."
$env:READOS_AI_BASE_URL = "https://api.openai.com/v1"
$env:READOS_AI_MODEL = "gpt-4.1-mini"
cargo run -p msp-web-host --offline
```

When configured, the composer exposes `对话` and `MSP` modes. Conversation
mode sends bounded message history to the configured provider. MSP mode remains
the only path that executes commands and continues to enforce the portable
registry, virtual cwd, read-only Host policy, and terminal audit. Provider
credentials are never returned by the capabilities endpoint.

The `?api=` query parameter can point the page at another explicitly
authorized loopback API during development; it does not grant a browser host
permission to discover or invoke arbitrary endpoints.
