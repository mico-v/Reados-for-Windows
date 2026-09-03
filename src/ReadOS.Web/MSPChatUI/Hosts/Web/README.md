# Web Host

This host loads the same `Renderers/Default/renderer.manifest.json` that native
hosts should use. It is primarily a cross-platform fixture runner for the shared
Default Web renderer.

Run it from the `MSPChatUI` directory with a static HTTP server, then open:

```sh
python3 -m http.server 8765 --bind 127.0.0.1
```

```text
http://127.0.0.1:8765/Hosts/Web/default.html
```

The page loads `Conformance/fixtures/default-basic.conversation.json` by
default. Pass `?fixture=<url>` to render another payload.

## ReadOS Rust runtime host

`reados-workbench.html` is the default product-facing local Web workbench. It
provides a responsive Codex-style shell, real server-side sessions,
conversation history, command composition, capability inspection, theme
switching, and timeline rendering. The root URL served by `msp-web-host` opens
this page. The narrower `reados-runtime.html` page remains available as a
single-request development diagnostic.

`reados-runtime.html` is the browser integration page. It uses the same
Default renderer, but obtains its timeline from the loopback Rust
`msp-web-host` API:

```text
cargo run -p msp-web-host -- --bind 127.0.0.1:8787
python3 -m http.server 8765 --bind 127.0.0.1
```

Open the workbench:

```text
http://127.0.0.1:8787/
```

The browser sends a structured `{ command, cwd }` request. The Rust host runs
the portable `reados-portable-msp-v1` command pack over a virtual workspace and
returns an MSPChatUI timeline. Shell operators, host paths, PATH lookup, and
remote binding are not part of this endpoint. This page is intentionally a
development host; production must place authentication and product policy,
approval, and durable audit above the same runtime adapter.
