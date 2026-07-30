# Repository Guidelines

## Project Structure & Module Organization
ReadOS is a Windows MSP vertical service workbench. The WinUI host lives in `src/ReadOS.App`, with XAML views under `Views/`, view models under `ViewModels/`, services under `Services/`, and MSP host adapters under `Services/Msp/`. The portable .NET MSP runtime is in `src/ReadOS.Msp`, grouped by `Commands/`, `Runtime/`, `Workspace/`, `Policy/`, `Audit/`, `Parsing/`, and `Models/`. The host-neutral layer is `src/ReadOS.Msp.Hosting` (`Artifacts/`, `Commands/`, `Native/`, `Policy/`, `Runtime/`, `Sessions/`). MSP xUnit tests live in `tests/ReadOS.Msp.Tests`, `tests/ReadOS.Msp.Hosting.Tests`, and `tests/ReadOS.App.Tests`. The Rust native core prototype is in `native/msp-core`. Design and setup notes are in `docs/`; generated outputs belong under `artifacts/`.

## Architecture (big picture — read across files)
The codebase enforces a strict three-layer ownership boundary; understanding the split matters more than any single file.

- `ReadOS.Msp` is runtime-neutral: command parsing, registry, runtime context, result/audit model, policy interface, and the virtual-workspace abstraction. It must not reference WinUI, PDF, chat, or provider code.
- `ReadOS.Msp.Hosting` is host-neutral: session/transcript projection, artifact catalog + provenance classification, approval-grant store, active-command cancellation registry, command-pack/registry composition, the `IMspCommandHost` facade, and the native adapter (`Native/`). It owns reusable contracts only.
- `ReadOS.App` is the WinUI workbench + domain host: PDF rendering/extraction, chat/provider services, workspace persistence, app command-pack construction, operator approval policy, and observable UI state. It assembles the runtime via `ReadOsMspHost` + `ReadOsMspHostRuntimeFactory`.

### Virtual workspace is the canonical read model
Commands read/write through a virtual workspace (`/artifacts`, `/sessions`, `/transcripts`, `/documents`, `/library`). It — not the file system — is the agent-facing source of truth. Namespace membership is validated AFTER path normalization via `MspNamespacePathUtility`; string-prefix checks are NOT a security boundary. Artifact ops, session/transcript reads, and manifest lookups must resolve through it.

### Command lifecycle & terminal audit
Every command flows parse → policy/approval → execute (managed `MspRuntime`, or a Rust proxy for `pwd`/`echo`) → terminal result + audit. Every attempt yields a terminal audit record; pre-policy failures use `MspPolicyDecision.NotEvaluated`. Mutating commands declare effects and pass policy/approval first. Native execution returns exactly one `Allow` audit as evidence only and never appends to the managed result.

### Native Rust core & stable adapter
`native/msp-core` is the Windows-compatible Rust core (read-only NTFS WorkspaceFS, command registry, length-delimited ABI v2) reached through `ReadOS.Msp.Hosting.Native` (`reados-msp-native/1`) with strict bounds, sanitization, and fail-closed fallback. Only canonical lowercase `pwd`/`echo` route to Rust; everything else stays managed. Business services must depend on the adapter, never raw FFI.

### Current work
T92 (trustworthy boundary) is done. T93 (product E2E), T94 (workbench hardening), and T95 (upstream-aligned Rust core) are in progress; `docs/DEVELOPMENT_TRACKER.md` is the authoritative item-level status, including the phased T95 compatibility matrix.

## Build, Test, and Development Commands
- `.\scripts\run.ps1 -BuildOnly`: build the WinUI workbench in Debug.
- `.\scripts\run.ps1`: build and launch `ReadOS.App`.
- `.\scripts\run.ps1 -StopExisting`: stop a running app instance before rebuilding.
- `dotnet build .\ReadOS.sln`: build the full solution.
- `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj`: run MSP tests.
- `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj --filter "FullyQualifiedName~Runtime_records_audit"`: run a single test or name subset.
- `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj --filter "ClassName~ParserTests"`: run all tests in one test class.
- `.\scripts\verify-msp.ps1`: run Rust fmt/tests/clippy/build, native smoke test, MSP tests, and solution build.
- `.\scripts\package-windows.ps1 -StopExisting`: create a self-contained Windows x64 release under `artifacts/`.

## Coding Style & Naming Conventions
Use four-space indentation, file-scoped namespaces, nullable reference types, and implicit usings for C#. Types and public members use `PascalCase`; private fields use `camelCase`. Async methods end with `Async`. Keep WinUI UI definitions in XAML, app/service orchestration in focused services, and runtime-neutral MSP contracts in `ReadOS.Msp`. Rust code uses edition 2021 conventions, `snake_case`, and `cargo fmt`.

## Testing Guidelines
Current automated coverage is xUnit for `ReadOS.Msp`. Name test files after the unit under test and use descriptive method names such as `Runtime_records_audit_for_each_command`. Add parser, runtime, policy, audit, and workspace tests for each new command. For MSP/native work, prefer `.\scripts\verify-msp.ps1`; for quick managed checks, run `dotnet test`.

## Commit & Pull Request Guidelines
Git history uses short imperative subjects, often with prefixes like `feat:` and `Fix`. Prefer concise subjects such as `feat: Add MSP artifact command` or `Fix WinUI build setup`. PRs should include the user-visible change, test commands run, linked issues or plan items, and screenshots or short recordings for UI/workbench changes.

## Security & Configuration Tips
Do not commit API keys, imported documents, local workspace data, or generated artifacts. ReadOS stores user data under `%LOCALAPPDATA%\ReadOS`; treat that directory as private local state.
