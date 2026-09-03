# `msp-backend-linux`

ReadOS-owned Linux host-backed implementation of the neutral `msp-backend` workspace contract.

## Narrow fail-closed boundary

The trusted host root is accepted only by `LinuxWorkspaceBackend::open`. A successful constructor
opens the root as an owned directory descriptor and probes Linux `openat2`; the root pathname is
never stored, logged, returned, or included in errors. If `openat2` is unavailable, construction
returns the typed `LinuxBackendOpenError::UnsupportedPlatform` and does not fall back to a
pathname-based implementation.

Virtual paths are mounted below `/workspace`; `/` is not a valid target. `stat`, `list`,
`read_range`, `write_file`, `rename`, and `delete` resolve through the owned root descriptor with
`RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS | RESOLVE_NO_SYMLINKS`. No `canonicalize`, string
containment check, or check-then-reopen sequence is used as a security boundary. Directory listing
uses descriptor-relative metadata and excludes hidden `.msp`, symlinks, special files, traversal,
and host-specific path syntax. Reads and writes enforce bounds before allocating buffers. Rename is
descriptor-relative `renameat2` for files or directories, with optional `RENAME_NOREPLACE`; delete
is descriptor-relative `unlinkat` for regular files or empty directories. Recursive deletion,
trash, directory creation, and unsafe fallbacks are unsupported.

Only WorkspaceRead and WorkspaceWrite are supported after a successful probe. Process, PTY, event
streaming, and cancellation capabilities remain unsupported. The non-Linux build exports the same
public names as a compile-only stub that returns `UnsupportedPlatform` without evaluating the
supplied root.

## Verification

```text
cargo fmt --all -- --check
cargo test -p msp-backend-linux --locked
cargo clippy -p msp-backend-linux --all-targets --locked -- -D warnings
cargo package --workspace --list --allow-dirty --locked
```

Linux fixture tests cover binary/NUL bytes, nested files, listing, traversal, symlink escape,
hidden paths, bounds-before-allocation, anchored writes, descriptor-relative file and directory
rename, regular-file and empty-directory delete, unsupported-kernel classification, and redacted
errors. Linux `openat2`
runtime evidence belongs on the Ubuntu portable CI runner; this Windows development checkout may
not have a Linux target or Linux runtime.

This package contains only ReadOS-authored code, under Apache-2.0. It does not modify or depend on
`native/msp-core`, `native/msp-ffi`, managed product code, or the raw `MSP/` source tree.
