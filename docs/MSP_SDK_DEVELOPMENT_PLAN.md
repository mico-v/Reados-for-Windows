# MSP SDK Development Plan

This document describes how ReadOS should align its working in-app runtime with upstream MSP behavior, implement the Windows-compatible runtime-neutral core in Rust, and connect that core through a stable .NET adapter without losing the vertical application feedback loop.

## Product Direction

ReadOS is now the vertical service host for MSP. Its Windows runtime must be informed by working product needs and by the local upstream MSP contract/conformance evidence; it must not invent a separate minimal JSON protocol and call it MSP.

The model-facing boundary remains small:

```text
exec_command({ "cmd": "library list" })
```

The runtime-neutral MSP layer is Rust-first. `native/msp-core` owns, in staged form:

- virtual workspace paths;
- WorkspaceFS and host-path sanitization;
- binary command streams and shell parsing/execution;
- command registration;
- command metadata;
- policy decisions;
- audit records;
- runtime state, sessions, cancellation, pipelines/redirection, and Windows process/ConPTY integration.

`ReadOS.Msp.Hosting` owns the managed/native adapter, DLL lifecycle, dependency injection, approved app-command bridging, and host-neutral command-host/session/artifact projections. `src/ReadOS.Msp` remains a temporary managed product runtime and compatibility oracle while selected generic behavior migrates; new runtime-neutral MSP semantics should not be added only in C#.

`ReadOS.App` owns app-specific document/PDF/chat adapters, workflow orchestration, provider credentials, workspace persistence, command-pack construction, artifact/lineage presentation, and WinUI projection.

## Upstream Reference Boundary

The root `MSP/` directory is a nested, local-only Apache-2.0 upstream reference input. T95 uses:

- `MSP/Spec` for portable behavior, security, WorkspaceFS, command, audit, AgentBridge, and profile contracts;
- `MSP/Conformance` for fixtures, inventories, oracles, and executable behavior evidence;
- `MSP/Implementations/Swift/Sources/MSPCore` for runtime-neutral command/workspace/policy/audit semantics;
- `MSP/Implementations/Swift/Sources/MSPShell` for shell execution behavior;
- `MSP/Implementations/Swift/Sources/MSPPOSIXCore` for the portable command-profile reference.

`MSP/Implementations/Windows` currently contains only `.gitkeep`. It is not a usable Windows implementation. ReadOS therefore implements the Windows-compatible general runtime in `native/msp-core` and reaches it through a stable .NET adapter.

The local `MSP/` checkout is not a ReadOS runtime dependency, parent-repository source tree, CI prerequisite, or package payload. Never add the raw checkout to ReadOS history or package output. If code, fixtures, or documentation are copied or derived, record the source revision/snapshot, local modifications, Apache-2.0 license, NOTICE obligations, and downstream provenance beside the distributed derivative.

## Current Package Shape

```text
src/
  ReadOS.App/        WinUI workbench and ReadOS host adapters
  ReadOS.Msp/        portable .NET MSP runtime and command SDK
  ReadOS.Msp.Hosting/ host-neutral command host, sessions, policy/approval, and artifact services
tests/
  ReadOS.Msp.Tests/  parser/runtime/workspace tests
  ReadOS.Msp.Hosting.Tests/ hosting contract/service tests
  ReadOS.App.Tests/  app adapters, workflows, persistence, security, and UI projection tests
native/
  msp-core/          Windows runtime-neutral Rust mainline
MSP/                 nested local reference only; ignored and not packaged
```

Verified on 2026-08-12: 77 Rust tests, 69 Core tests, 303 Hosting tests (including 3/3 real release-DLL operations), and 411 App tests (783 managed total under the full verifier), plus native binary/FFI verification and a zero-warning/zero-error solution build. The Hosting count includes 3/3 real release-DLL operations; the 3 native-DLL tests skip only when run standalone without `READOS_MSP_NATIVE_DLL`. The registry candidate release DLL SHA256 is `2CFD14246FA963AC284B158903ADC910A782AFEDFEA4EC5F247692BF4613E49A`.

The latest completed full package gate is `package-windows.ps1 -StopExisting -Version 0.1.0-native-command-registry-verified`. It verifies publish, the seven-export static-CRT native binary, staged ABI v2/v1 FFI, package contents, direct packaged Rust `ls /` and binary `cat /workspace.json`, and product-host Rust proxy execution for `pwd`, `echo ''`, and `echo -n reados-native-proxy`. Packaged runtime evidence reports `LengthDelimitedV2` 2.0, 532 ZIP entries, `RawMSP`/PDB/`.git` counts of zero, all five `nativeCommands` exiting 0, an audit count of 1 for each proxy, cleanup with zero native-run residue, and ZIP creation at `artifacts/releases/ReadOS-0.1.0-native-command-registry-verified-win-x64.zip`.

## Target Package Shape

```text
native/
  msp-core/             Windows-compatible Rust runtime-neutral MSP core
src/
  ReadOS.Msp/           temporary managed compatibility oracle/contracts during migration
  ReadOS.Msp.Hosting/   stable native adapter; service sessions, policy, audit, artifacts
  ReadOS.App/           vertical workbench and document host adapter
tests/
  ReadOS.Msp.Tests/
  ReadOS.Msp.Hosting.Tests/
  ReadOS.App.Tests/
MSP/                    local reference checkout only; absent from Git/package output
```

The Hosting project split is complete and separately tested. Do not create another package merely to move code: T95 should first establish upstream compatibility, the Rust Windows core, and the stable managed adapter while document/PDF/chat/workflow behavior remains in the C# app domain.

## Phase 1: In-Process Runtime

Status: implemented for the managed control plane with bounded Rust adoption for canonical lowercase `pwd`/`echo`; broader Rust adoption remains incomplete.

Implemented:

- `MspCommandRequest`
- `MspCommandResult`
- `MspCommandDiagnostic`
- `MspArtifact`
- `MspAuditRecord`
- `MspCommandTranscriptRecord`
- `MspSessionRecord`
- `MspCommandPreview`
- `MspCommandRegistry`
- `MspRuntime`
- `MspCommandInvocation`
- `MspCommandEvent`
- parser and workspace path utility
- request environment passed into policy
- request session ID passed into policy, audit, and command execution context
- argument-specific command metadata through `IMspCommand.GetMetadata(arguments)`
- command previews through `IMspCommand.GetPreview(arguments)`
- read/write workspace abstraction
- command event sink and `MspCommandContext.ReportProgressAsync`
- structured command diagnostics with stable codes and recovery hints
- in-memory workspace
- policy interface and allow-all policy
- effect-based policy for mutating/external command confirmation
- audit sink interface and in-memory sink
- core commands: `help`, `pwd`, `echo`, `ls`, `cat`
- artifact commands: `artifact list`, `artifact show`, `artifact write`
- ReadOS host adapter
- ReadOS virtual workspace
- ReadOS commands: `workspace`, `library`, `pdf`, `windows`, `page-label`, `outline`, `attach`, `chat`
- WinUI transcript approval actions using one-shot app-host approval tokens
- persisted workbench transcript state and `/transcripts/{id}.json` workspace projection
- durable session state and `/sessions/{id}.json` workspace projection
- policy/audit/transcript preview diagnostics for approval-gated commands
- transcript/session diagnostic summaries and recovery hints for failed commands
- artifact provenance fields and `/artifacts/*.manifest.json` sidecar projections
- PDF text extraction and search results to durable artifacts with automatic source document/page provenance
- chat answers to durable Markdown artifacts with queued attachment provenance
- workflow summaries from `/sessions` and `/transcripts`, with optional Markdown artifact output and session/transcript provenance
- streaming execution events for command start, policy decision, progress, completion, and cancellation
- workbench transcript consumption of streaming progress plus operator command cancellation
- shared normalized namespace/path/identifier validation for artifacts, sessions, transcripts, and workflow source manifests
- terminal result and audit evidence for success, parse failure, unknown command, policy confirmation/denial/exception, command exception, and cancellation
- explicit `NotEvaluated` policy state for attempts that fail before authorization

Remaining runtime-alignment work:

- continue expanding the upstream Spec/Profile/Conformance/Swift compatibility matrix and Windows adaptation decisions;
- migrate selected generic execution from the managed runtime into the verified Rust/Hosting boundary while retaining differential evidence;
- avoid adding new generic runtime semantics only to the managed compatibility oracle;
- keep adapter/conformance inputs free of provider secrets and app-private workspace data.

## Phase 2: Hosting Layer

Status: implemented as `src/ReadOS.Msp.Hosting`.

The service host sits above `MspRuntime` without taking ownership of ReadOS document-domain behavior.

Responsibilities:

- create and track sessions;
- execute commands with cancellation;
- publish progress and transcript events;
- request policy authorization;
- persist audit and artifact records;
- expose a model-facing `exec_command` bridge.

Current status: `ReadOS.Msp.Hosting` contains tested host-neutral command-host interfaces/adapters, string-command facade, request construction, approved-command orchestration, command-pack composition/validation/diagnostics, runtime-host assembly, approval grants, cancellation, session projection/storage contracts, session/transcript workspace paths, and artifact catalog/provenance services. Its `Native` boundary implements `reados-msp-native/1`, safe explicit/package-directory DLL loading, ABI v2 negotiation with isolated all-exports-missing v1 fallback, allocation/free ownership, strict UTF-8/JSON/Base64 validation, operation-specific request/response limits, deep-frozen result/audit/AST projections, host-path disclosure validation, runtime ABI evidence, and real release-DLL tests. `ReadOsMspHost` composes those services with the app-owned PDF/chat/workflow command pack, operator policy mode, workspace persistence, and UI projection. The product composition now protects `pwd`/`echo` from case-insensitive host-pack overrides and routes only their exact canonical lowercase execution bodies through one shared lazy native provider.

Candidate APIs:

```csharp
ValueTask<MspCommandResult> ExecuteAsync(
    MspCommandRequest request,
    CancellationToken cancellationToken = default);

IAsyncEnumerable<MspCommandEvent> ExecuteStreamingAsync(
    MspCommandRequest request,
    CancellationToken cancellationToken = default);
```

ReadOS also exposes `ExecuteApprovedStreamingAsync(...)` to replay an operator-approved mutating command while retaining the same lifecycle/progress event stream.

`native_command_core_registry_v1` is complete: Rust `Command`, `Invocation`, `Context`, `Registry`, and `CommandPack` contracts replace hard-coded dispatch without changing output bytes, exit codes, diagnostics, audit evidence, fixtures, ABI behavior, or product routing. The next Hosting/core requirement is `native_mixed_workspace_read_v1`: define the lifetime-safe callback/handle boundary needed to bridge the app-owned virtual workspace while raw P/Invoke, native handles, and native-library paths remain outside App/domain code.

## Phase 3: Policy And Mutating Commands

Status: implemented for the current command pack.

Add policy-aware commands:

- `page-label set`
- `outline add`
- `outline delete`
- `attach page`
- `attach range`
- `chat ask`
- `artifact write`

All mutating commands should support:

- dry run;
- approval preview;
- allow/confirm/deny policy;
- audit record;
- rollback note or recovery guidance where practical.

Current status: generic mutating-command confirmation is wired into the workbench transcript, and page-label, outline, artifact write/rename/delete, attachment, chat, and artifact-producing workflow commands run through the same policy/audit path. Per-command preview text is present in policy/audit/transcripts. Confirmation, denial, provider/document failures, exceptions, and cancellation carry stable diagnostics and terminal audit evidence. T93 product integration now proves cancellation with exit 130 and no partial artifact, invalid outline-page diagnostics with no artifact after restart, and provider failure/restart/retry without API-key or sensitive-request leakage while preserving lineage and audit. Visible UI, real-provider, and packaged-process restart acceptance remain open.

## Phase 4: Artifact And Workspace Contracts

Status: implemented for the current ReadOS workspace and artifact lifecycle.

Add `/artifacts` to the workspace and make artifacts first-class SDK objects.

Required behavior:

- generated artifacts are addressable by virtual path;
- artifact manifests contain provenance;
- commands can return artifact references;
- artifacts can be listed, shown, exported, and reused by later commands.

Current status: artifact write/rename/delete, PDF extraction/search artifacts, chat answers, session summaries, document explanation, structured evidence, evidence review/synthesis, failure review, and artifact refinement all produce durable provenance-aware outputs. The ReadOS workspace persists source command, actor, session, timestamps, preview, media type, source artifacts/manifests/documents/pages, and exposes manifest JSON sidecars beside content. Shared normalized namespace checks confine artifact content and manifests to `/artifacts` and validate session/transcript record paths before workflows follow them.

## App Credential Boundary

Provider credentials are not MSP workspace files or runtime-adapter data. `ReadOS.App` uses `IProviderCredentialStore`; the Windows implementation protects per-provider API keys with DPAPI `CurrentUser`, excludes them from `workspace.json` and workspace exports, and migrates a legacy plaintext key only after protected persistence succeeds. Missing credentials start safely in offline behavior without issuing a provider request. Native code, artifacts, transcripts, audit records, diagnostics, adapter payloads, and conformance inputs must never receive the key.

## Phase 5: Upstream-Aligned Windows Rust Core And .NET Adapter

Status: in progress as T95.

Implement the general Windows-compatible runtime-neutral surface in `native/msp-core` using the upstream reference boundary above. The Rust mainline includes, in staged form:

- WorkspaceFS path resolution, virtual filesystem contracts, and host-path sanitization;
- command registry, command metadata, results, streams, cancellation, and diagnostics;
- policy requests/decisions and audit/evidence semantics;
- MSPShell parsing and execution semantics selected for the Windows profile;
- an explicit MSPPOSIXCore-compatible command subset operating on virtual WorkspaceFS rather than arbitrary host binaries;
- an upstream conformance runner plus ReadOS differential/adapter tests.

Do not move ReadOS document services, WinUI state, PDF libraries, provider secrets, workspace persistence, or domain workflows into the native core. Those remain C# business-layer concerns. The .NET adapter owns all managed/native lifecycle, request, streaming, cancellation, workspace, policy/audit, result, artifact, and error translation; app services must not call raw FFI directly.

Structured adapter payloads may be versioned where necessary, but they are an implementation boundary, not the definition of MSP. MSP behavior comes from the selected upstream Spec/Profile/Conformance contracts and is checked against the Swift reference implementation where fixtures alone are insufficient.

Current verified foundation:

- backend-neutral `VirtualPath` plus a fixed-local-NTFS, read-only Windows WorkspaceFS using retained root/target handles, final-handle containment, stable directory enumeration, binary range reads, and final-path `.msp` hiding;
- Rust `ls` and binary-safe `cat`, bounded output/resources, panic-contained C exports, and a stateful Windows path sanitizer for DOS/verbatim/slash/JSON/file-URL/percent-encoded variants across chunk splits;
- an internal `workspaceRoot` authorization input that is never projected into result or audit data;
- static MSVC CRT release output with native export/dependency verification;
- the Hosting native adapter described above, including real release-DLL `pwd`/parse/path/WorkspaceFS integration tests with no skip under the full verifier;
- `native_pwd_echo_adoption_v1`: managed `MspRuntime` owns parse, policy/approval, dry-run, streaming, terminal result, cancellation projection, and exactly-once product audit; only canonical lowercase `pwd`/`echo` execute through one lazy Rust proxy, case variants fail closed without managed fallback, and `ls`/`cat`/`help` plus app-domain commands remain managed;
- `native_abi_v2_handshake_v1`: seven required DLL exports, a fixed 32-byte `2.0` handshake with contract `0x324D534F44414552` and capabilities `0xF`, length-delimited pointer/`ulong` buffers, embedded-NUL preservation, fail-closed partial-export/handshake handling, allocator separation, free-exactly-once ownership, serialized invoke/dispose, and runtime ABI evidence;
- `native_command_core_registry_v1`: validated `Command`, `Invocation`, `Context`, `Registry`, and `CommandPack` contracts, deterministic registry names and `help`, duplicate/invalid registration rejection, unknown lookup, pack composition, and a passing 12-case baseline/candidate ABI v1/v2 differential with no observable command or product-routing drift;
- package automation that builds and copies `msp_core.dll`, supplies license/NOTICE/provenance, rejects raw `MSP/`, `.git`, credentials/private state, PDBs, and dynamic CRT markers, runs staged v2/v1 FFI, direct native `ls`/`cat` on an isolated fixed-NTFS temporary root, exercises host-proxy `pwd`, `echo ''`, and `echo -n reados-native-proxy` with one managed audit each, requires ABI v2 runtime evidence, cleans in `finally`, and produces the verified ABI v2 ZIP.

Still incomplete:

- product execution through Rust beyond the bounded canonical-lowercase `pwd`/`echo` slice;
- mutable WorkspaceFS and recoverable trash;
- pipelines/redirection, expansion, and broader command conformance;
- `exec_command`/`write_stdin` sessions, controlled processes, and Windows ConPTY/Job Object cleanup;
- native UTF-16LE process-output sanitization before external processes are enabled;
- the mixed-workspace/stream/runtime slices described below, beginning with a lifetime-safe read-only bridge to the app-owned virtual workspace.

### Completed adoption gate: `native_pwd_echo_adoption_v1`

- No command-name `HybridHost` was added. Managed `MspRuntime` remains the control plane and authoritative owner of parse, policy/approval, dry-run, streaming lifecycle, terminal result, cancellation projection, and exactly-once product audit.
- Only exact lowercase `pwd` and `echo` execution bodies use the lazy Hosting Rust proxy. Case variants fail closed without native load or managed fallback. Product command packs cannot override either protected name in any casing; existing app overrides such as `workflow` remain supported.
- Native Parse validates the original `commandText` and must agree on raw input, command name, explicit-empty-aware arguments, one pipeline, one command, and the absence of operators, redirection, assignment, negation, raw newline, or unquoted ampersand. Missing DLL, parser disagreement, native audit drift, non-`Allow` evidence, state change, and non-text output all fail closed.
- Native audit is execution evidence only and is not appended to the managed result. Native requests carry raw command text, virtual cwd, actor, and session only; they do not carry approval environment, credentials, or a workspace root.
- `ls`/`cat` remain managed until a Rust virtual/mixed bridge can preserve `ReadOsVirtualWorkspace`; `help` and all PDF/chat/workflow/credential/persistence/WinUI behavior remain in C#.
- Full verification and the no-skip `0.1.0-native-pwd-echo-verified` package gate pass. Package smoke proves `pwd`, `echo ''`, and `echo -n reados-native-proxy` through the real host proxy with one managed audit each, in addition to direct native `ls`/`cat`.

Current limitations are explicit: the synchronous v1 FFI cannot interrupt work already executing inside native code, so cancellation checks surround only the bounded side-effect-free calls. The raw-form filter deliberately rejects raw CR/LF and unquoted expandable ampersands; quoted or escaped operator literals are covered positive cases and must remain accepted. Product override protection is enforced by `ReadOsMspHostRuntimeFactory`; product paths must not bypass it through the generic composition builder.

### Completed boundary gate: `native_abi_v2_handshake_v1`

- `MspAbiInfoV2` is a fixed 32-byte layout reporting major `2`, minor `0`, contract `0x324D534F44414552`, and required capabilities `0xF`.
- `msp_invoke_v2` and `msp_free_buffer_v2` use explicit pointer/`ulong` lengths. Embedded NUL requests are not truncated, and v1/v2 allocations and free exports never mix.
- Hosting selects v1 only if all three v2 exports are absent. Any partial set, ABI size/version/contract/capability drift, malformed response, or oversized result fails closed. Every native-return path frees exactly once, and invoke/dispose use the same lock.
- Parse/Execute/Normalize request caps are 128 KiB/1 MiB/1 MiB; response caps are 16 MiB/64 MiB/1 MiB. The Rust bounded writer stops before reserve/copy, closing the measured 54.5x Parse amplification denial-of-service path.
- Runtime ABI information is exposed for package smoke. Rust/native verification, 303 Hosting tests, 3/3 real release-DLL operations, static CRT, the full solution verifier, staged v2/v1 FFI, and the no-skip ABI v2 packaged-process gate pass.

### Completed runtime gate: `native_command_core_registry_v1`

- Small Rust `Command`, `Invocation`, `Context`, `Registry`, and `CommandPack` contracts replace the hard-coded command list and dispatch.
- Registry lookup and pack composition validate names, reject duplicates, handle unknown commands, expose deterministic names, and generate `help` from the registry.
- Currently implemented output bytes, exit codes, diagnostics, audit evidence, state changes, conformance fixtures, ABI behavior, and managed product routing remain unchanged.
- Sixty-six Rust tests and the 12-case baseline/candidate ABI v1/v2 differential pass against candidate DLL SHA256 `2CFD14246FA963AC284B158903ADC910A782AFEDFEA4EC5F247692BF4613E49A` without adding commands, streams, sessions, pipelines, processes, or filesystem behavior.

### Next adoption gate: `native_mixed_workspace_read_v1`

- Add capability-based read-only WorkspaceFS traits and deterministic longest-prefix mount routing with virtual-path rebasing.
- Define an opaque-handle and callback boundary for the app-owned virtual workspace, including ownership, disposal, cancellation, concurrency, and .NET delegate-lifetime rules before enabling managed callbacks.
- Preserve direct fixed-local-NTFS reads, `VirtualPath`, path sanitization, ABI v2 negotiation/ownership, and managed policy/audit authority.
- Do not migrate product `ls`/`cat` until direct, virtual, and mixed read semantics pass focused, differential, real-DLL, and package-level evidence.

Dependency-ordered follow-up slices are `native_stream_core_v1` before pipeline execution, then mutable WorkspaceFS/trash, pipelines/redirection, exec sessions, and finally Windows ConPTY/Job Objects. These are separate review and release gates, not one migration PR.

### Compatibility Matrix

Maintain a versioned matrix with one row per adopted feature or command and these fields:

- upstream Spec/Profile section;
- upstream fixture/oracle/reference implementation;
- Rust module and test;
- .NET adapter contract/test;
- status: `conformant`, `partial`, `deferred`, or `not applicable`;
- Windows-specific deviation and rationale;
- security/path-output expectations;
- source/license/provenance record when material is copied or derived.

### Acceptance Gates

| Stage | Status | Deliverable | Required evidence |
| --- | --- | --- | --- |
| 0. Reference inventory | complete for the selected snapshot | Selected Spec/Profile/Conformance/Swift source map and provenance ledger | Reference revision/snapshot recorded; Windows placeholder confirmed; package exclusion and NOTICE rules reviewed. |
| 1. MSPCore parity | partial | Rust WorkspaceFS, command/result/stream, policy, audit, and diagnostic core | The read-only handle-based WorkspaceFS slice and selected built-ins pass; mutable FS, streams/sessions, and broader parity remain. |
| 2. Shell/POSIXCore compatibility | partial | MSPShell semantics plus explicit Windows-compatible MSPPOSIXCore subset | Selected six-case command snapshot and AST coverage pass; expansion, execution graphs, and broader fixtures remain. |
| 3. Stable .NET adapter | partial | Managed/native lifecycle and full execution translation | Length-delimited ABI v2, fail-closed negotiation, isolated complete-v1 fallback, allocator/free ownership, operation caps, runtime evidence, real-DLL tests, and bounded `pwd`/`echo` product translation pass; streaming and broader translation remain. |
| 4. ReadOS adoption | partial | Existing host/workflow paths execute through the adapter | Canonical lowercase `pwd`/`echo` now execute through Rust under the managed control plane with fail-closed routing and differential evidence; broader core/workflow adoption and duplicated-runtime retirement remain. |
| 5. Release | complete for the ABI v2 slice | Native runtime and adapter ship safely | The no-skip package gate proves staged v2/v1 FFI, packaged ABI 2.0 negotiation, fixed-NTFS direct-native and product-host proxy smoke, exactly-once managed audit, cleanup, raw `MSP/` exclusion, seven-export static-CRT binary, ZIP, license/NOTICE/provenance, and log redaction. |

## Validation Rules

Every command added to MSP must have:

- parser coverage when it depends on quoting or argument shape;
- runtime coverage for success and failure exit codes;
- argument-specific metadata coverage when subcommands have different side effects;
- preview coverage for approval-gated commands;
- audit coverage;
- diagnostic coverage for failure code and recovery hints;
- policy behavior if it can mutate user state;
- artifact coverage if it creates durable output;
- session coverage if it affects transcript grouping, artifacts, approvals, or persisted host state;
- at least one ReadOS host integration check if it touches workspace, PDF, chat, or artifacts.
- boundary coverage for any virtual path or persisted identifier, including traversal, rooted/drive paths, backslashes, prefix siblings, and sidecar aliases;
- proof that provider secrets cannot enter workspace serialization, exports, logs, audit, transcript, artifacts, adapter payloads, native logs, or conformance inputs when credentials are involved;
- an upstream compatibility-matrix entry and applicable Spec/Conformance/Swift evidence for runtime-neutral Rust behavior;
- a recorded Apache-2.0 NOTICE/provenance decision for copied or derived upstream material;
- a release/package check proving the raw local `MSP/` reference tree is absent.

## Verification

Run managed tests (verified baseline: 69 + 303 + 411 = 783):

```powershell
dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj
dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj
dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj
```

Run the full MSP verification path:

```powershell
.\scripts\verify-msp.ps1
```

The full script fails immediately on Rust format/test/clippy/release build, native binary/FFI smoke, restore, any of the three managed test projects, real release-DLL adapter tests, or solution build. `.github/workflows/windows-ci.yml` runs this verifier on Windows using the SDK pinned by `global.json`. The release package script must additionally prove that the raw `MSP/` reference repository is absent while required native binary, license, NOTICE, and provenance files are present.

Current verified result on 2026-08-12: 77 Rust tests, native binary/FFI smoke, 69 Core tests, 303 Hosting tests, 411 App tests (783 managed total under the full verifier), and a zero-warning/zero-error solution build. The static-CRT release DLL exposes all seven required exports, has SHA256 `2CFD14246FA963AC284B158903ADC910A782AFEDFEA4EC5F247692BF4613E49A`, real-DLL ABI v2 Execute/Parse/Normalize passes 3/3, and the 12-case baseline/candidate ABI v1/v2 differential passes. The latest completed no-skip package gate is `0.1.0-native-command-registry-verified`, producing `artifacts/releases/ReadOS-0.1.0-native-command-registry-verified-win-x64.zip` with staged/package smoke, `LengthDelimitedV2` 2.0, 532 entries, `RawMSP`/PDB/`.git` counts of zero, five successful native commands, and three proxy audit counts of 1.
