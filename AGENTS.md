# Repository Guidelines

## Project Structure & Module Organization
ReadOS is a Windows MSP vertical service workbench. The WinUI host lives in `src/ReadOS.App`, with XAML views under `Views/`, view models under `ViewModels/`, services under `Services/`, and MSP host adapters under `Services/Msp/`. The portable .NET MSP runtime is in `src/ReadOS.Msp`, grouped by `Commands/`, `Runtime/`, `Workspace/`, `Policy/`, `Audit/`, `Parsing/`, and `Models/`. MSP xUnit tests live in `tests/ReadOS.Msp.Tests`. The Rust native core prototype is in `native/msp-core`. Design and setup notes are in `docs/`; generated outputs belong under `artifacts/`.

## Build, Test, and Development Commands
- `.\scripts\run.ps1 -BuildOnly`: build the WinUI workbench in Debug.
- `.\scripts\run.ps1`: build and launch `ReadOS.App`.
- `.\scripts\run.ps1 -StopExisting`: stop a running app instance before rebuilding.
- `dotnet build .\ReadOS.sln`: build the full solution.
- `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj`: run MSP tests.
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
