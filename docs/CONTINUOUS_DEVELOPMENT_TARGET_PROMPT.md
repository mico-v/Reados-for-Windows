# ReadOS Continuous Development Target Prompt

Use the prompt below as the durable target for an autonomous coding session. It is intentionally outcome-oriented: each session must leave the repository with a tested vertical slice, not only a plan or an architecture note.

```text
You are continuing development in the ReadOS repository at:

V:\github.com\mico-v\reados

Primary objective

Turn ReadOS into a secure, verifiable, demonstrable, and package-ready Windows MSP workbench. The generic MSP execution layer must become a Windows-compatible Rust implementation whose behavior is aligned with the local upstream MSP reference checkout. Keep ReadOS-specific document, PDF, chat, workflow, credential, persistence, and WinUI behavior in managed application code.

Authoritative inputs

1. Read and follow the repository AGENTS.md instructions and the current working tree before changing anything.
2. Treat the nested MSP/ repository as a read-only design and conformance reference pinned to its checked-out commit. Its important sources are:
   - MSP/Spec/Profiles/MSPModelWorkspaceExecutionSDKProfile.md
   - MSP/Spec/Profiles/MSPV1LinuxCommandLayerProfile.md
   - MSP/Spec/WorkspaceFS/WorkspaceFSProfile.md
   - MSP/Spec/AgentBridge/ExecCommandProfile.md
   - MSP/Conformance/Fixtures/
   - MSP/Implementations/Swift/Sources/MSPCore/
   - MSP/Implementations/Swift/Sources/MSPShellLanguage/
   - MSP/Implementations/Swift/Sources/MSPShellExpansion/
   - MSP/Implementations/Swift/Sources/ModelShellProxy/
3. MSP/ is a separate Apache-2.0 repository and is not a parent-repository source directory or package payload. Do not add the whole checkout to ReadOS history. Prefer clean-room implementation from documented behavior and fixtures. If any source or asset is copied or derived, preserve the required license, NOTICE, modification notice, and source provenance.
4. The current ReadOS tracker and plans are DEVELOPMENT_PLAN.md, docs/DEVELOPMENT_TRACKER.md, and docs/MSP_SDK_DEVELOPMENT_PLAN.md. Update them only with evidence actually produced in the current work.
5. The verified 2026-07-11 baseline is Rust 66, Core 69, Hosting 259, App 300, managed total 628, native binary/FFI verification, 3/3 real release-DLL operations, and a zero-warning/zero-error solution build. The registry candidate release DLL SHA256 is 2CFD14246FA963AC284B158903ADC910A782AFEDFEA4EC5F247692BF4613E49A. Treat these as a floor, not a reason to repeat completed implementation work.

Current truthful release state

- scripts/verify-msp.ps1 passes at the baseline above. The static-CRT DLL exposes all seven required exports.
- The handle-based read-only Windows WorkspaceFS, Hosting native adapter, static CRT, and package-content checks are implemented and verified.
- `native_pwd_echo_adoption_v1` is complete. Managed MspRuntime remains authoritative for parse, policy/approval, dry-run, streaming events, terminal result, cancellation projection, and exactly-once product audit. Only canonical lowercase pwd/echo execution bodies use one shared lazy Hosting Rust proxy. Case variants fail closed without managed fallback; ls/cat/help and all app-domain commands remain managed.
- `native_abi_v2_handshake_v1` is complete at the implementation/full-verifier gate. The fixed 32-byte ABI info reports major 2, minor 0, contract 0x324D534F44414552, and capabilities 0xF. Invoke/free use pointer/ulong lengths and preserve embedded NUL. Hosting falls back to v1 only if all three v2 exports are absent; partial exports or handshake drift fail closed. V1/v2 allocators never mix, every native-return path frees exactly once, invoke/dispose share one lock, and runtime ABI information is exposed for package evidence.
- `native_command_core_registry_v1` is complete at the implementation/full-verifier gate. Validated Rust Command, Invocation, Context, Registry, and CommandPack contracts replace the hard-coded command list/dispatch. Invalid and duplicate registration, unknown lookup, deterministic names/order, pack composition, and registry-derived help are covered. The 12-case baseline/candidate ABI v1/v2 differential passes without changing command bytes, exit codes, diagnostics, audit, state changes, fixtures, ABI behavior, or product routing, and no new command or capability was added.
- Parse/Execute/Normalize request caps are 128 KiB/1 MiB/1 MiB and response caps are 16 MiB/64 MiB/1 MiB. The Rust bounded writer stops before reserve/copy and closes the measured 54.5x Parse amplification denial-of-service path.
- The corrected package harness keeps marker/log/app state under artifacts, creates a separate native fixture root under a dedicated fixed-local-NTFS temporary directory, runs direct packaged Rust ls / and binary cat /workspace.json, exercises product-host pwd, echo '', and echo -n reados-native-proxy through Rust with one managed audit each, and removes the native run directory in finally.
- The latest completed no-skip package gate is `package-windows.ps1 -StopExisting -Version 0.1.0-native-command-registry-verified`, which produces artifacts/releases/ReadOS-0.1.0-native-command-registry-verified-win-x64.zip. Staged FFI and packaged-process smoke pass with seven exports, static CRT, `LengthDelimitedV2` 2.0, 532 ZIP entries, `RawMSP`/PDB/`.git` counts of zero, all five `nativeCommands` exiting 0, an audit count of 1 for each of the three product proxies, cleanup, content/license/provenance checks, and redaction.

Non-negotiable architecture

- native/msp-core is the home of runtime-neutral MSP semantics implemented in Rust 2021. It must compile and run on Windows x64 without assuming a POSIX host.
- Rust owns, in staged form: protocol/types, binary-safe streams, shell lexer/parser/AST, expansion, virtual workspace paths, WorkspaceFS, command registry/executor, runtime state, policy hooks, audit hooks, pipelines/redirection, sessions, output truncation, path sanitization, and Windows process/ConPTY integration.
- src/ReadOS.Msp remains temporarily as the product runtime and compatibility oracle while behavior is migrated. Do not add new generic runtime semantics only in C# when they belong in Rust.
- src/ReadOS.Msp.Hosting owns the stable internal reados-msp-native/1 adapter, DLL/allocation lifecycle, dependency injection, approved app-command bridging, limits/validation, and projections needed by .NET callers. App/domain code must not own P/Invoke or native handles.
- src/ReadOS.App owns PDF/document services, chat/provider integration, workflow/domain commands, artifacts and lineage presentation, credential storage, workspace persistence, and WinUI state. Do not move these domain features into Rust.
- Keep the internal SDK/native boundary separate from the model-facing AgentBridge. A versioned internal C ABI/JSON or equivalent structured boundary is allowed. Model-facing exec_command/write_stdin must use the upstream tool shapes and return plain terminal text, never an MSP-added JSON result envelope.

Verified foundation to preserve

- Backend-neutral VirtualPath plus a read-only Windows WorkspaceFS for explicit fixed local NTFS roots.
- Root/target handle-based containment, stable handle-based listing, binary range reads, Rust ls/cat, final-path case-insensitive .msp hiding, and rejection of UNC/mapped/removable/non-NTFS/drive/ADS/device injection.
- Stateful Rust host-path sanitization for DOS/verbatim/slash/JSON/file-URL/percent-encoded forms across chunk splits, bounded output/resources, and C ABI panic containment.
- Hosting native adapter with strict UTF-8/JSON/Base64, deep-frozen result/audit/AST graphs, request/response limits, contract and disclosure validation (including UTF-16LE variants), safe error strings, and real release-DLL tests.
- Length-delimited ABI v2 with fixed handshake/capability checks, all-or-none v1 fallback, separate allocators, free-exactly-once ownership, serialized invoke/dispose, operation-specific caps, bounded pre-allocation response writing, and package-visible runtime ABI evidence.
- Validated deterministic Rust command registry/pack composition with registry-derived `help`, stable unknown-command behavior, and a passing 12-case baseline/candidate ABI v1/v2 differential.
- Static MSVC CRT native output and binary verification; package content gates for raw MSP/, .git, secrets/private state, PDBs, dynamic CRT markers, and required license/NOTICE/provenance.
- Bounded product adoption for canonical lowercase pwd/echo. Native Parse must agree with the original command text, command name, explicit-empty-aware arguments, and one simple AST with no operator, redirection, assignment, negation, raw newline, or unquoted ampersand. Native execute evidence must contain exactly one matching Allow audit and no state change; native audit is never duplicated into the managed result. Native requests receive no approval environment, credentials, or workspace root.
- Packaged direct-native ls/cat smoke on a separate fixed-NTFS temporary root plus real product-host pwd, echo '', and echo -n marker smoke, with host-path-safe evidence, one managed audit per proxied result, cleanup in finally, and a verified release ZIP.

Required MSP behavior

- Model-visible paths are slash-separated workspace paths rooted at `/`. Host drive, UNC, sandbox, materialization, launcher, cache, and broker paths are never model-visible.
- Normalize `.` and clamp `..` at `/`; reject NUL and invalid path forms. On Windows, explicitly defend against drive/UNC injection, alternate separators, case-insensitive prefix mistakes, symlinks, junctions, and other reparse-point escapes.
- Support direct host-backed, virtual, and mixed workspace backends behind one WorkspaceFS contract. Direct host-backed access must not require staging all files through temporary copies.
- Hide internal paths such as `.msp`; omit them from listings and express access errors using virtual paths.
- Agent-facing remove is recoverable by default through hidden trash. Hard deletion or trash emptying requires explicit host authorization.
- stdin, stdout, and stderr are byte streams. Do not make UTF-8 String the only internal representation. Preserve separate stdout/stderr for pipe execution and one ordered byte stream for PTY execution.
- Command context includes cwd, environment, umask, stdin/stream handles, command/subcommand runners, policy, and audit. Command result includes byte output, exit code, runtime state changes, and optional model content.
- Long-running execution, yield timing, write_stdin polling/writes, cancellation, concurrent sessions, truncation metadata, process-tree termination, and PTY/pipe distinction are first-class contracts.
- Sanitize host paths in complete and chunked stdout/stderr, diagnostics, exceptions, tracebacks, process launch failures, environment values, and file URLs. A path split across chunks must not leak.
- Linux-like command behavior is implemented by the controlled runtime or explicitly allowed virtualized processes; never expose cmd.exe or PowerShell semantics as if they were MSP conformance.
- Use upstream fixture/oracle files as behavioral inputs. Record intentional Windows differences in a compatibility matrix instead of silently changing expected behavior.

Dependency-ordered implementation sequence

Work in small vertical slices. Start at the first incomplete gate; do not rebuild the verified foundation:

1. native_mixed_workspace_read_v1: add capability-based WorkspaceFS traits, longest-prefix mount routing/rebasing, and a lifetime-safe read-only bridge to the app-owned virtual workspace. Define opaque-handle, callback ownership, disposal, cancellation, concurrency, and .NET delegate-lifetime rules before enabling managed callbacks. Preserve direct fixed-local-NTFS behavior, ABI v2, sanitization, and managed policy/audit authority. Only after this gate may product `ls`/`cat` migrate to Rust.
2. native_stream_core_v1: add only bounded binary stdin/stdout/stderr contracts, backpressure, close/broken-pipe behavior, cancellation, and deterministic truncation metadata. Do not execute pipelines or sessions in this slice.
3. mutable_workspacefs_trash_v1: add handle-confined writes, same-volume recoverable hidden `.msp/trash`, restoration collision rules, and host-authorized hard deletion/emptying with policy/audit/security tests.
4. native_pipeline_execution_v1: add expansion, byte pipes, descriptor routing, redirection, stage status, close/broken-pipe semantics, cancellation, and deterministic diagnostics over the verified stream core.
5. native_exec_sessions_v1: implement model-facing `exec_command` and `write_stdin`, exact yield policy, bounded one-time reads, concurrent pipe sessions, cancellation, and process-tree termination without adding PTY behavior yet.
6. windows_conpty_job_v1: add controlled external processes, Windows Job Object cleanup, ConPTY, and one ordered terminal byte stream, including UTF-16LE process-output sanitization before enablement.
7. Broader conformance and backends: grow attributed fixtures/oracles, command coverage, controlled subprocess/Python behavior, session stress, and package-level Windows acceptance.

Current product priorities

- Preserve the completed trustworthy-boundary work: namespace confinement, terminal audit evidence, fail-fast verification/packaging, pinned toolchains/CI, and DPAPI CurrentUser credential storage outside workspace/export data.
- Preserve completed T93 service-level recovery evidence: cancellation exits 130 with one terminal record/no partial artifact; invalid outline pages return stable diagnostics/no artifact; provider failure with sensitive exception content leaks nothing and restart/retry preserves lineage/audit.
- Continue T93 until visible WinUI interaction, a real provider/network boundary, and the full flagship workflow across a packaged-process restart are proven.
- Continue T94 workbench hardening for Runtime Drawer details/persistence, latest-request-wins loading, deprecated presenter cleanup, responsive/manual evidence, and package smoke.
- Treat the former “MSP Protocol v1” item as the upstream-MSP-aligned Rust/Windows migration and conformance workstream, not as permission to invent an unrelated minimal protocol.
- Preserve the current synchronous-invocation limitation honestly: the FFI cannot interrupt a native call already in flight, so cancellation checks only surround the bounded side-effect-free pwd/echo calls. The raw-form guard deliberately rejects raw CR/LF and unquoted expandable ampersands; quoted or escaped operator literals are covered positive cases and must not regress. Product override protection lives in ReadOS composition, so product paths must continue to use `ReadOsMspHostRuntimeFactory` rather than a generic builder that can replace core commands.

Autonomous execution loop

1. Inspect git status, active diffs, tests, plans, and relevant upstream specification/fixture files. Preserve user changes and avoid overlapping edits.
2. Select the highest-priority incomplete acceptance item that can be delivered safely as one coherent vertical slice. State the upstream behavior and Windows constraint it proves.
3. Implement production code plus focused tests. Reuse canonical fixtures instead of duplicating expected values. Keep public APIs small and avoid speculative abstractions.
4. Run the narrowest relevant tests first, then the full release gate when the slice is stable.
5. Review security boundaries, model-visible output, licensing/provenance, generated files, and git diff. Fix warnings and formatting failures rather than documenting around them.
6. Update tracker/plan/readme text with exact commands, test counts, limitations, and remaining acceptance gaps. Never mark an item done from component tests alone.
7. If time remains and no real blocker exists, immediately choose the next dependency-ordered slice. Do not stop after planning, scaffolding, or a partial happy path.

Verification commands

- dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj --filter FullyQualifiedName~ReadOsFlagshipWorkflowIntegrationTests
- .\scripts\verify-msp.ps1
- dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj
- dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj
- dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj
- dotnet build .\ReadOS.sln
- .\scripts\verify-msp-native-binary.ps1 against the release DLL when native exports/dependencies change
- .\scripts\verify-windows-package.ps1 against the staged package when package content changes
- .\scripts\package-windows.ps1 -StopExisting
- .\scripts\smoke-windows-package.ps1 against the generated package when available
- git diff --check

Definition of a successful development turn

- At least one real acceptance gap is closed with implementation and automated evidence.
- Rust and managed ownership boundaries remain intact.
- Windows-specific path/process behavior is tested where relevant.
- No host path, secret, private workspace data, or raw nested MSP checkout is added to model output, fixtures, exports, packages, or parent Git history.
- Targeted tests pass; the full verifier/package smoke is run whenever proportionate to the change, with any unrun gate reported honestly.
- The handoff names changed files, exact verification results, remaining risks, and the next highest-priority slice.
```
