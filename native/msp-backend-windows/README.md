# `msp-backend-windows`

ReadOS-owned Windows adapter boundary for the neutral `msp-backend` contract.

## Retained-handle security boundary

On Windows, `WindowsWorkspaceBackend` binds an absolute fixed local NTFS root to an owned handle
and retains the source only as a private implementation detail. Each stat/list/range-read/write
operation opens its target first, obtains the final handle path, compares it to the retained root
with ordinal case-insensitive component boundaries, revalidates every resolved component, rejects
reparse points and hidden `.msp` components, and then uses that same handle. It never uses
canonicalize/check/reopen pathname containment. The public API exposes only the neutral virtual
`WorkspaceBackend` contract and DTOs; no legacy type, host path, or handle crosses the boundary.

The first write contract is deliberately small: bounded whole-file replace-or-create beneath an
already-existing verified parent. It does not create directories, rename, delete, or implement
trash. Process and PTY remain typed `Unsupported` until a separate verified launch binding exists.
Non-Windows builds remain fail-closed stubs and do not inspect supplied roots.

The neutral contract remains in `native/msp-backend`; it has no host filesystem, process, legacy
`msp-core`, FFI, managed-code, or public ABI dependency. Legacy compatibility is kept in the
separate `msp-backend-windows-core-bridge` migration oracle, not in this crate.

## Features and tests

- `default = ["read"]`: retains the compatibility feature name and enables the Windows workspace
  backend on Windows.
- `read`: compatibility feature; the backend is available only on Windows and is unsupported on
  other targets.
- `process`: compatibility feature; always unsupported.

The crate tests capability reporting, retained-handle reads/writes, binary ranges, hidden and
host-syntax rejection, root-form rejection, path-free errors, and the unsupported process
boundary. Run:

```powershell
cargo test -p msp-backend-windows --all-features --locked
cargo clippy -p msp-backend-windows --all-features --all-targets --locked -- -D warnings
cargo check -p msp-backend-windows --all-features --target x86_64-pc-windows-msvc --locked
```

## Provenance and packaging

The manifest uses the workspace path dependency on `msp-backend`, so this crate is a workspace
package until the neutral contract is published or the dependency is changed to a reviewed
registry release. `cargo publish` is blocked by `publish = false`.

All files in this package are authored in ReadOS under Apache-2.0. The package must never include
`MSP/`, `.git`, private workspace roots, generated binaries, or legacy `msp-core` source. The
retained-handle slice is an H3.1 implementation and still requires product adoption, write/trash
policy integration, and broader target evidence before it is used by ReadOS commands.
