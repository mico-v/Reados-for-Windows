# Linux MSP backend boundary

Architecture authority: [MSP_HYBRID_ARCHITECTURE.md](MSP_HYBRID_ARCHITECTURE.md). This document describes the Linux platform backend only.

`native/msp-backend-linux` is a narrow ReadOS-owned host workspace adapter over the neutral
`native/msp-backend` contract. It is not a process runtime, PTY implementation, or port of the
legacy MSP implementation.

## Binding and resolution

`LinuxWorkspaceBackend::open` is the only API accepting a trusted host root. It immediately opens
the root as an owned `O_PATH|O_DIRECTORY|O_CLOEXEC|O_NOFOLLOW` descriptor and probes `openat2`.
The pathname is not retained or included in diagnostics. A missing or unsupported `openat2` returns
`LinuxBackendOpenError::UnsupportedPlatform`; there is no canonicalize/check/reopen fallback.

After construction, virtual paths are restricted to `/workspace` and descendants. Every target and
parent resolution uses the owned descriptor and:

- `RESOLVE_BENEATH`
- `RESOLVE_NO_MAGICLINKS`
- `RESOLVE_NO_SYMLINKS`

Operations use descriptor-relative `fstat`, `openat2`, `read_at`, and `write_at`. Bounded mutation
uses `renameat2` and `unlinkat` against descriptor-opened parents; rename supports files and
directories with optional no-replace semantics, while delete supports regular files and empty
directories only. Recursive delete, trash, directory creation, and metadata-preserving copy are
outside this backend contract. If `renameat2` is unavailable or rejected by the kernel, the
operation returns a typed unsupported/failure result and never falls back to a pathname
implementation. Symlinks and special files are rejected, hidden `.msp` and host-shaped components are
not exposed, traversal is rejected by the neutral virtual-path contract, and operation bounds are
validated before buffer allocation. Errors retain only stable generic categories and never host
pathnames or payload bytes.

The capability report advertises WorkspaceRead and WorkspaceWrite only after successful binding and
probe. Process, PTY, event streaming, and cancellation remain unsupported. Non-Linux builds keep
the public API as a compile-only stub that returns `UnsupportedPlatform` without evaluating the
supplied root.

Ubuntu portable CI runs the Linux fixture tests and Linux-target Clippy/build checks. The focused
Linux fixture suite covers binary nested reads, bounded replacement writes, descriptor-relative
rename/delete (including directory subtrees and empty directories), collision handling,
symlink/special-file rejection, hidden/traversal rejection, path-free diagnostics, and injected
`openat2` error classification (`ENOSYS`/`EINVAL` fail closed as `UnsupportedPlatform`; other
probe failures remain `Operation`). On 2026-08-26 the same suite passed in an Ubuntu 24.04.4 LTS
container (`sha256:33ceb71981b602c1a7443a53469e4dba065f7503eab3078a2d7a57a2ab987517`), together
with workspace tests, Linux-target check, Clippy, and package-content checks.
This is local Ubuntu container evidence, not a claim that GitHub Actions has run.
