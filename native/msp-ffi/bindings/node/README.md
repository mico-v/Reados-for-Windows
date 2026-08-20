# `@reados/msp-ffi-node`

This package is a small, bounded Node.js contract for the public
`msp_ffi.h` ABI. It owns only the opaque native session, workspace, and result
handles exposed by that header.

## Explicit DLL selection

The DLL path is required at the call site and must be absolute. The loader does
not inspect `PATH`, search the application directory, search `node_modules`, or
choose a repository/runtime copy. Before any native loader is called it:

1. checks the explicit path and `.dll` suffix;
2. reads only the bounded PE/DOS header;
3. compares the PE machine value with `process.arch`; and
4. validates ABI version `1` and runtime version `0.1.0`.

```js
const { loadMspFfi } = require("@reados/msp-ffi-node");

const msp = loadMspFfi("C:\\Program Files\\ReadOS\\native\\msp_ffi.dll");
const workspace = msp.createWorkspace();
const session = workspace.createSession();
const result = session.run("echo hello");
result.ensureSuccess("echo");
console.log(new TextDecoder().decode(result.stdout));
```

The default adapter is loaded lazily from the optional `koffi` peer dependency.
Offline consumers can pass an explicit `NativeBackend` adapter to
`loadMspFfi`; this is also useful for contract tests. Supplying a backend does
not bypass the absolute-path, PE, architecture, or ABI checks.

There are no package install or post-install scripts. Native DLL deployment is
an explicit application concern.

## Ownership and workers

`MspResult` is a snapshot, not a borrowed native view. The binding copies both
native result buffers and releases the native result handle before returning.
Every byte accessor returns another copy, preserving embedded NUL and non-UTF-8
bytes. A result can therefore be sent through a `worker_threads` message port
as ordinary structured-clone data; native pointers are never transferred.

`MspWorkspace` is an in-memory virtual workspace. Its methods accept only
absolute virtual POSIX paths and reject host drive, UNC, backslash, ADS,
hidden-component, reserved-name, control-character, and traversal forms before
crossing the ABI. File bytes remain arbitrary bytes.

`MspSession.run` is the registered in-process command surface from the public
ABI. This package intentionally exposes no `spawn`, `exec`, process-spec,
host-root, environment, or host-path API.

## Tests

The package tests are offline and use a small fake ABI backend plus generated PE
headers; they do not launch a process or require a native DLL. Run them with:

```sh
npm test
```

An Electron-specific consumer is not required for the package contract. If an
Electron integration job is added, it should be marked blocked when the
Electron runtime is not installed rather than falling back to ordinary Node
or to a host PATH lookup.
