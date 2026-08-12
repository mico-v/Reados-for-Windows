# ReadOS

ReadOS is an MSP-first vertical application service and Windows workbench. Its purpose is to prove how an AI agent can operate inside real product software through an app-owned command runtime, virtual workspace, domain command packs, policy, audit, and durable artifacts.

The existing document/PDF reading experience is now the first vertical domain for MSP rather than the final product boundary. It gives the runtime rich materials, conversations, page evidence, search, attachments, and user-visible workflows to operate on.

The product direction is maintained in [PRODUCT_GOAL.md](PRODUCT_GOAL.md), and the implementation path is maintained in [DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md).

## Current Status

- WinUI 3 Windows workbench under `src/ReadOS.App`.
- Portable .NET MSP runtime under `src/ReadOS.Msp`.
- Host-neutral service/session/policy/artifact composition under `src/ReadOS.Msp.Hosting`.
- MSP parser, command registry, runtime context, workspace abstraction, policy, terminal audit, structured diagnostics, streaming events, and core commands.
- ReadOS host adapter and document command pack under `src/ReadOS.App/Services/Msp`.
- ReadOS virtual workspace exposing settings, projects, library, documents, artifacts, sessions, and transcripts.
- Domain commands and named workflows for PDF inspection/extraction/search, chat, evidence extraction/review/synthesis, artifact refinement, and failure review.
- Document services for PDF rendering, text extraction, search, page labels, outlines, attachments, and per-document conversations.
- OpenAI-compatible chat service and offline fallback.
- Typed thread timeline, approval surfaces, artifact lineage, compact sidebar/Review Dock flyouts with remembered wide-layout widths, and a bottom Runtime Drawer for command supervision.
- Artifact/session/transcript namespace confinement, complete terminal audit coverage, fail-fast verification/package scripts, and Windows CI.
- Provider API keys stored outside workspace JSON and exports with Windows DPAPI (`CurrentUser`), including legacy plaintext migration.
- Upstream-aligned Rust/Windows MSP core under `native/msp-core`, including byte-safe internal results, shell AST, selected fixture parity, a validated deterministic command registry/pack composition layer, handle-based read-only NTFS WorkspaceFS, path sanitization, static CRT, and a negotiated, length-delimited ABI v2 alongside the temporary internal-v1 compatibility exports.
- Stable `reados-msp-native/1` Hosting adapter with strict operation-specific limits, host-path disclosure protection, fail-closed ABI negotiation, real release-DLL tests, and bounded product adoption: only canonical lowercase `pwd` and `echo` execute through a shared lazy Rust proxy while managed `MspRuntime` retains parse, policy/approval, dry-run, streaming, terminal-result, and exactly-once product-audit authority.
- Three managed test projects under `tests/`.

Verified baseline on 2026-08-12: 69 `ReadOS.Msp.Tests`, 348 `ReadOS.Msp.Hosting.Tests` (including real release-DLL adapter operations; the native-DLL tests skip only when run standalone without `READOS_MSP_NATIVE_DLL`), 411 `ReadOS.App.Tests` (828 managed tests total under the full verifier), plus 181 Rust tests, native binary/FFI verification, and a zero-warning/zero-error solution build. The candidate release DLL has SHA256 `2CFD14246FA963AC284B158903ADC910A782AFEDFEA4EC5F247692BF4613E49A`, exposes all seven required exports, and uses the static MSVC CRT. ABI v2 reports a 32-byte `2.0` layout, contract `0x324D534F44414552`, capabilities `0x3F` (required `0xF`), and length-delimited request/response buffers that preserve embedded NUL bytes; partial v2 availability or handshake drift fails closed, while complete absence of all three v2 exports alone permits the isolated v1 fallback.

The service-level flagship integration path proves PDF import, outline-driven evidence extraction, approval across restart, synthesis, persistence, denial without partial output, lineage back to a source PDF page, cancellation with no partial artifact, invalid-page failure, and secret-safe provider failure/restart/retry. `native_abi_v2_handshake_v1` remains implementation-, verifier-, and package-complete. `native_command_core_registry_v1` is implementation-, verifier-, and package-complete: validated `Command`, `Invocation`, `Context`, `Registry`, and `CommandPack` contracts replace the hard-coded list/dispatch, and the 12-case baseline/candidate ABI v1/v2 differential passes without changing command bytes, exit codes, diagnostics, audit, fixtures, ABI behavior, or product routing. The latest completed no-skip release is `artifacts/releases/ReadOS-0.1.0-native-command-registry-verified-win-x64.zip`; staged FFI and packaged-process smoke pass with `LengthDelimitedV2` 2.0, 532 ZIP entries, `RawMSP`/PDB/`.git` counts of zero, five `nativeCommands` all exiting 0, and an audit count of 1 for each of the three product proxies. T93 still needs visible WinUI, real provider/network, and packaged-process flagship restart evidence; T94 (Runtime Drawer resize/pin/persistence, latest-request-wins loading, deprecated presenter removal) is complete; T95 has verified `native_mixed_workspace_read_v1` (ABI v2 operation 4 — a lifetime-safe callback bridge that lets a managed `IMspNativeReadOnlyWorkspace` serve the Rust composite, proven by real release-DLL differential reads without migrating product `ls`/`cat`), `native_stream_core_v1` (bounded byte-stream primitives with backpressure/close semantics), the mutable WorkspaceFS/trash slice (TOCTOU-safe writes + a recoverable hidden `.msp/trash`), and pipelines/redirection execution, and model-facing exec sessions (`exec_command`/`write_stdin` with unique session ids, bounded one-time terminal-text reads, and `MSPExecCommandYieldPolicy` timing), and the Windows ConPTY/Job Object process backend + UTF-16LE sanitizer, and ConPTY session integration (external processes now run through `exec_command`/`write_stdin` with stdin continuation and kill-on-expiry); next needs broader product adoption and wider conformance.

## Architecture Direction

ReadOS follows this vertical MSP service-host architecture; current work strengthens its product proof and upstream runtime compatibility rather than introducing a different layer model:

1. Operator workbench: WinUI surface for workspace, evidence, command transcript, and approval.
2. MSP service host: sessions, command execution, policy checks, audit records, and artifact lifecycle.
3. Command runtime: parser, command registry, exit codes, stdout/stderr, and future composition features.
4. Virtual workspace: app-owned file model projected as stable MSP paths.
5. Domain command packs: document, PDF, chat, artifact, workflow, and future app-specific commands.
6. Agent bridge: a small model-facing boundary such as `exec_command({ "cmd": "pdf search current \"policy\"" })`.
7. Native core and adapter: a Windows-compatible Rust runtime-neutral core in `native/msp-core`, behavior-aligned with upstream MSP and connected to ReadOS through a stable .NET adapter.

The root `MSP/` directory is a nested, local-only Apache-2.0 reference input, not a ReadOS source tree, CI dependency, runtime, Git payload, or package payload. T95 uses its `Spec/`, `Conformance/`, and Swift `MSPCore`, `MSPShell`, and `MSPPOSIXCore` implementations to define behavior. `MSP/Implementations/Windows` currently contains only `.gitkeep`, so the Windows-compatible implementation belongs in ReadOS `native/msp-core`; it must not be presented as a newly invented minimal JSON protocol. Any copied or derived upstream material must retain the applicable license, NOTICE, modification, and source-provenance records, while the raw `MSP/` reference tree remains excluded from ReadOS packages.

## Build, Test, And Run

Build only:

```powershell
.\scripts\run.ps1 -BuildOnly
```

Build and run the workbench:

```powershell
.\scripts\run.ps1
```

Run the managed test projects:

```powershell
dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj
dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj
dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj
```

Run the full MSP verification path:

```powershell
.\scripts\verify-msp.ps1
```

The verifier fails immediately when any Rust, native smoke, managed test, restore, or solution-build command fails. The same path runs in `.github/workflows/windows-ci.yml` with the SDK pinned by `global.json`.

Create a Windows x64 release package:

```powershell
.\scripts\package-windows.ps1 -StopExisting
```

Packaging runs an isolated hidden-start smoke before creating the ZIP. Its app evidence remains under `artifacts`, while direct native WorkspaceFS `ls`/`cat` uses a separate fixed-local-NTFS temporary root that is removed in `finally`. The same smoke also exercises the staged product host proxy with canonical lowercase `pwd`, `echo ''`, and `echo -n reados-native-proxy`, requiring one managed audit for each result. Use `-SkipSmoke` only when that gate is intentionally deferred.

## Development Documents

- [PRODUCT_GOAL.md](PRODUCT_GOAL.md): MSP vertical service product direction.
- [DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md): practical implementation roadmap.
- [docs/DEVELOPMENT_TRACKER.md](docs/DEVELOPMENT_TRACKER.md): current T92-T95 status, acceptance criteria, and verification evidence; T1-T91 are retained as history.
- [docs/MSP_PLAN.md](docs/MSP_PLAN.md): service architecture and runtime model.
- [docs/MSP_SDK_DEVELOPMENT_PLAN.md](docs/MSP_SDK_DEVELOPMENT_PLAN.md): upstream-aligned Windows Rust core, .NET adapter, compatibility, conformance, and provenance plan.
- [docs/MSP_UPSTREAM_COMPATIBILITY_MATRIX.md](docs/MSP_UPSTREAM_COMPATIBILITY_MATRIX.md): feature-by-feature upstream evidence, Rust/.NET status, Windows deviations, and the next acceptance gate.
- [docs/CONTINUOUS_DEVELOPMENT_TARGET_PROMPT.md](docs/CONTINUOUS_DEVELOPMENT_TARGET_PROMPT.md): reusable autonomous-development target for the Rust/Windows MSP migration and remaining product gates.
- [docs/UI_UX_DESIGN.md](docs/UI_UX_DESIGN.md): desktop conversation workbench UI/UX design and implementation status.
- [docs/MSP_AGENT_COMMAND_LOOP.md](docs/MSP_AGENT_COMMAND_LOOP.md): prompt-injected MSP command loop.
- [docs/ENVIRONMENT_SETUP.md](docs/ENVIRONMENT_SETUP.md): Windows, WinUI, and CLI setup.
