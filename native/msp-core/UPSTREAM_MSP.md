# Upstream MSP compatibility boundary

This Rust crate is a clean-room Windows implementation informed by the public
MSP specification and observable conformance fixtures at upstream commit
`982baa54e9093e39f828d8827be6c75aed7502ff`.

Semantic references used for this slice:

- `Spec/Profiles/MSPModelWorkspaceExecutionSDKProfile.md`
- `Spec/WorkspaceFS/WorkspaceFSProfile.md`
- `Spec/AgentBridge/ExecCommandProfile.md`
- `Conformance/Fixtures/MSPV1LinuxCommandLayer.*.json`
- the public Swift MSPCore, MSPShellLanguage, MSPShellExpansion, MSPShell, and
  MSPPOSIXCore interfaces, when available as external review evidence

The 2026-08-18 local source package explicitly excludes Mac/Swift implementation
source and contains only `MSP/Implementations/Swift/Sources/Tools`. Therefore
those Swift paths are provenance/design references, not locally inspectable source
evidence; the committed ReadOS snapshot and available Spec/Conformance inputs are
the clean-checkout test boundary.
The Rust source does not copy Swift implementation code. Default tests consume
the attributed, committed snapshot under `../../conformance/msp-upstream` so a
clean ReadOS checkout does not depend on a nested clone. The local `../../MSP`
clone is a read-only review/drift-check input only: it is not a parent-repository
source directory, CI prerequisite, linked runtime input, Git payload, or package
payload. MSP is Apache-2.0, Copyright 2026 Nian; preserve the upstream license,
NOTICE, modification record, and provenance when distributing copied or derived
upstream source or fixture material.

The JSON C ABI in this crate is an internal ReadOS SDK boundary. It is not the
agent-facing MSP protocol. Agent-facing `exec_command` remains a `cmd` request
with plain terminal-text output as required by the upstream profile.

`native/msp-ffi` is a separate ReadOS-owned public C ABI wrapper around the
portable core. It is not the internal `reados-msp-native/1` adapter and is not a
claim of upstream `msp-ffi` source parity. Its release contract is recorded in
`native/msp-ffi/release-metadata.json`: a reproducible x86_64 release DLL with
29 exports, the versioned `msp_ffi.h` header, a recorded SHA-256, and static MSVC
CRT verification. It is an optional distribution artifact only; the default
ReadOS package and app smoke do not load it.

## `windows_workspacefs_read_v1`

The verified host-backed Rust WorkspaceFS slice is deliberately narrow:

- `VirtualPath` performs only backend-neutral `/`, current-directory, NUL, and
  root-clamped `..` handling. Windows drive/UNC/device/ADS/name rules are
  applied separately at the Windows backend boundary.
- The accepted host root must be an absolute drive-letter path on a
  `DRIVE_FIXED` volume whose opened root handle reports NTFS. UNC/SMB,
  mapped-network, removable, and non-NTFS roots fail closed.
- `stat`, stable `list_directory`, and binary `read_file_range` open a target
  handle first. The target handle's final normalized path is compared with the
  retained root handle's final path using Windows ordinal case-insensitive
  component boundaries. Reads and metadata use that same checked handle.
- Directory enumeration uses `GetFileInformationByHandleEx` on the checked
  directory handle. It does not call `read_dir` on a previously checked path.
- Final long-path components are checked again against hidden policy, closing
  short-name/alias and internal-reparse routes into `.msp`/`.MSP`.
- `workspaceRoot` is accepted only as an optional internal execute-request
  authorization input. It is skipped during request serialization and never
  enters result or audit JSON. `ls`, `cat`, and `pwd` emit virtual paths and
  byte-safe/base64 command output.
- Native stdout is capped at 2 MiB, stderr at 64 KiB, and directory collection
  at 65,536 entries/8 MiB metadata. Limit hits are deterministic failures with
  `msp.output.limit` or a virtual-path-only WorkspaceFS error.
- DOS, extended, slash, JSON-escaped, ordinary file-URL, and percent-encoded
  file-URL root variants are sanitized with chunk-split protection and
  sibling-prefix boundaries. UTF-16LE byte-stream sanitization is deferred;
  this slice does not launch external processes that can produce UTF-16LE
  output. Any future external runner must add it before enablement.

This slice does not implement writes, trash/delete, external processes,
pipelines, PTY/ConPTY, or sessions. `stat` currently follows an inward reparse
target after containment instead of exposing complete lstat-style link
metadata. ABI v2 now provides independent fixed-width negotiation and
length-delimited buffers; the legacy NUL-terminated exports remain only as an
isolated compatibility fallback. All JSON exports catch Rust panics and return
contract-shaped failures so unwinding never crosses the C ABI.

## `native_abi_v2_handshake_v1`

The verified ABI boundary is deliberately explicit:

- The release DLL exposes seven required exports. `MspAbiInfoV2` is exactly 32
  bytes and reports ABI major `2`, minor `0`, contract
  `0x324D534F44414552`, and capabilities `0xF`.
- `msp_invoke_v2` and `msp_free_buffer_v2` use pointer/`u64` lengths. Embedded
  NUL bytes are part of the request rather than terminators.
- Hosting may use v1 only when all three v2 exports are absent. Any partial v2
  export set or size/version/contract/capability drift fails closed.
- V1/v2 result pointers and free exports never mix. Every native-return path
  frees exactly once, and invoke/dispose share one transport lock.
- Parse/Execute/Normalize request caps are 128 KiB/1 MiB/1 MiB; response caps
  are 16 MiB/64 MiB/1 MiB. The bounded response writer refuses growth before
  reserve/copy, closing the measured 54.5x Parse amplification denial-of-service
  path.
- Runtime ABI information is projected for staged-package evidence. Rust/native
  verification, all three real release-DLL operations, staged v2/v1 FFI, and
  the no-skip packaged-process ABI v2 gate pass.

## Managed and release boundary

`src/ReadOS.Msp.Hosting/Native` is the only managed/native ownership boundary.
Its internal `reados-msp-native/1` adapter loads an explicit absolute DLL or the
packaged app-directory DLL, negotiates ABI v2, owns allocation/free calls,
validates strict UTF-8/JSON/Base64 and contract/audit/AST structure, freezes
returned graphs, enforces operation-specific limits, reports runtime ABI
evidence, and rejects host-path disclosures including UTF-8 and UTF-16LE
variants. App/domain code does not own P/Invoke declarations or native handles.
`native_pwd_echo_adoption_v1` routes only canonical lowercase
`pwd` and `echo` execution bodies through one shared lazy adapter. Managed
`MspRuntime` still owns parse, policy/approval, dry-run, streaming, terminal
results, cancellation projection, and exactly-once product audit. Case variants
fail closed without managed fallback; `ls`, `cat`, `help`, and app-domain
commands remain managed.

Release builds use the static MSVC CRT. Native verification checks all seven
required exports, FFI behavior, and the absence of dynamic CRT markers. The
verified 2026-07-11 baseline is 66 Rust tests, 69 Core tests, 259 Hosting tests
including 3/3 real release-DLL operations with no skip, 300 App tests (628
managed total), a passing native binary/FFI gate, and a zero-warning/zero-error
solution build. The registry candidate release DLL SHA256 is
`2CFD14246FA963AC284B158903ADC910A782AFEDFEA4EC5F247692BF4613E49A`.

Package automation copies `msp_core.dll` plus license/NOTICE/provenance and
rejects raw `MSP/`, `.git`, credentials/private state, source files, linker
outputs, PDBs, and dynamic CRT markers. With the explicit
`-IncludePublicMspFfi` option it additionally builds from
`native/msp-ffi/Cargo.toml`, verifies the 29-export public DLL/header/hash/static
CRT contract, and copies only `msp_ffi.dll` plus `include/msp_ffi.h`. The
optional public artifact is not part of the existing core DLL or app smoke
path. The latest completed no-skip package gate is
`0.1.0-native-command-registry-verified`.
Staged FFI and packaged-process smoke verify `LengthDelimitedV2` 2.0, 532 ZIP
entries, `RawMSP`/PDB/`.git` counts of zero, five native commands all exiting 0,
and one managed audit for each of the three proxied results, with cleanup and
redacted logs. The verified release ZIP is
`artifacts/releases/ReadOS-0.1.0-native-command-registry-verified-win-x64.zip`.

## `native_command_core_registry_v1`

The verified Rust command core now uses small `Command`, `Invocation`,
`Context`, `Registry`, and `CommandPack` contracts instead of hard-coded command
enumeration and dispatch. Registration validation, duplicate rejection, unknown
lookup, deterministic command names/order, pack composition, and registry-derived
`help` are covered. The 12-case baseline/candidate ABI v1/v2 differential passes
against the candidate DLL hash above, preserving output bytes, exit codes,
diagnostics, audit evidence, state changes, fixtures, ABI behavior, and product
routing. This slice adds no commands, capabilities, streams, sessions,
pipelines, processes, or filesystem behavior.

The next bounded slice is `native_mixed_workspace_read_v1`: add capability-based
read-only WorkspaceFS traits, deterministic longest-prefix mount routing and
virtual-path rebasing, plus a lifetime-safe bridge to the app-owned virtual
workspace. Opaque handle and callback ownership, disposal, cancellation,
concurrency, and .NET delegate lifetime must be explicit before managed callbacks
are enabled. The fixed-local-NTFS backend, `VirtualPath`, path sanitization, ABI
v2, and managed lifecycle/policy/audit authority must remain unchanged; product
`ls`/`cat` routing waits for this gate. Synchronous invocation still cannot
preempt native work already in flight; current cancellation checks only surround
the bounded side-effect-free `pwd`/`echo` calls.

After the mixed read bridge is verified, keep later work split into
dependency-ordered gates: a bounded byte-stream core, mutable WorkspaceFS/trash,
pipelines/redirection, exec sessions, and finally external processes with
Windows Job Objects and ConPTY.
