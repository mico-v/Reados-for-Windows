# `msp-backend-windows-core-bridge`

ReadOS-owned, read-only Windows bridge over the public
`msp_core::WindowsLocalReadOnlyWorkspace` API.

## Security boundary

The bridge exposes a small owned DTO and trait surface for the virtual mount `/workspace`:

- bounded `stat`, directory `list`, and binary range reads;
- no mutable operations and no process bridge;
- path-free typed errors and no host path in metadata DTOs or diagnostics;
- rejection of traversal, UNC/device syntax, alternate data streams, DOS device names,
  trailing-dot/space aliases, controls, and case-insensitive `.msp` components; and
- reparse points are fail-closed rather than exposed as ordinary files.

The underlying core workspace retains its trusted root handle and checks final target handles for
containment and hidden-component policy. This crate does not duplicate that security boundary with
`canonicalize`, string containment, or check-then-reopen logic. It translates core metadata and
errors into bridge-owned types and never returns a core trait, core path, `File`, raw handle, or
other host resource.

The crate deliberately does **not** implement the neutral mutable `msp_backend::WorkspaceBackend`:
Windows root and metadata semantics are narrower and security-sensitive, so coercing them into the
neutral mutable contract would hide important differences.

## Features and portability

The root workspace excludes `native/msp-core`. The only dependency edge to it is optional and is
activated by the explicit `windows-bridge` feature. The default feature set is empty, so normal
workspace builds do not resolve or compile the excluded package. On non-Windows targets, and on
Windows without that feature, the same public constructor and trait are fail-closed stubs that
return `Unsupported` without evaluating the supplied host path.

A Windows handle-backed build can be checked with:

```powershell
cargo test -p msp-backend-windows-core-bridge --all-features --locked
cargo clippy -p msp-backend-windows-core-bridge --all-features --all-targets --locked -- -D warnings
cargo check -p msp-backend-windows-core-bridge --features windows-bridge --target x86_64-pc-windows-msvc --locked
```

## Provenance and package contents

This is authored ReadOS code under Apache-2.0. It does not copy or modify `native/msp-core`,
`native/msp-ffi`, managed product code, or raw `MSP/` source. The optional dependency is a private
implementation detail and is never part of the public API.
