# MSP SDK Development Plan

This document describes how ReadOS should turn the in-app MSP runtime into a reusable SDK and native core without losing the vertical application feedback loop.

## Product Direction

ReadOS is now the vertical service host for MSP. The SDK should emerge from working product needs, not from an abstract standard designed in isolation.

The model-facing boundary remains small:

```text
exec_command({ "cmd": "library list" })
```

The SDK and host layers own:

- virtual workspace paths;
- command registration;
- command metadata;
- policy decisions;
- audit records;
- artifact records;
- app-specific host adapters.

## Current Package Shape

```text
src/
  ReadOS.App/        WinUI workbench and ReadOS host adapters
  ReadOS.Msp/        portable .NET MSP runtime and command SDK
tests/
  ReadOS.Msp.Tests/  parser/runtime/workspace tests
native/
  msp-core/          Rust native core prototype
```

## Target Package Shape

```text
native/
  msp-core/             Rust parser/protocol/conformance core
src/
  ReadOS.Msp/           .NET contracts, runtime, command SDK
  ReadOS.Msp.Hosting/   service sessions, policy, audit, artifacts
  ReadOS.App/           vertical workbench and document host adapter
tests/
  ReadOS.Msp.Tests/
  ReadOS.Msp.Hosting.Tests/
```

Create new projects only when the boundary has enough tests to justify the split.

## Phase 1: In-Process Runtime

Status: started.

Implemented:

- `MspCommandRequest`
- `MspCommandResult`
- `MspArtifact`
- `MspAuditRecord`
- `MspCommandTranscriptRecord`
- `MspCommandRegistry`
- `MspRuntime`
- parser and workspace path utility
- request environment passed into policy
- argument-specific command metadata through `IMspCommand.GetMetadata(arguments)`
- read/write workspace abstraction
- in-memory workspace
- policy interface and allow-all policy
- effect-based policy for mutating/external command confirmation
- audit sink interface and in-memory sink
- core commands: `help`, `pwd`, `echo`, `ls`, `cat`
- artifact commands: `artifact list`, `artifact show`, `artifact write`
- ReadOS host adapter
- ReadOS virtual workspace
- ReadOS commands: `workspace`, `library`, `pdf`, `windows`, `page-label`, `outline`
- WinUI transcript approval actions using one-shot app-host approval tokens
- persisted workbench transcript state and `/transcripts/{id}.json` workspace projection

Next:

- richer command result diagnostics;
- artifact provenance beyond the first persisted content fields;
- more ReadOS virtual workspace tests.

## Phase 2: Hosting Layer

Add a service host above `MspRuntime`.

Responsibilities:

- create and track sessions;
- execute commands with cancellation;
- publish progress and transcript events;
- request policy authorization;
- persist audit and artifact records;
- expose a model-facing `exec_command` bridge.

Current app status: `ReadOsMspHost` now sits above the raw runtime with an effect-based policy, one-shot approval token path, and durable workspace-backed transcript records. It still needs durable session records plus cancellation/progress events before it should become a separate `ReadOS.Msp.Hosting` project.

Candidate APIs:

```csharp
ValueTask<MspCommandResult> ExecuteAsync(
    MspCommandRequest request,
    CancellationToken cancellationToken = default);

IAsyncEnumerable<MspCommandEvent> ExecuteStreamingAsync(
    MspCommandRequest request,
    CancellationToken cancellationToken = default);
```

## Phase 3: Policy And Mutating Commands

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

Current status: generic mutating-command confirmation is wired into the workbench transcript. Per-command preview text and recovery guidance are still pending.

## Phase 4: Artifact And Workspace Contracts

Add `/artifacts` to the workspace and make artifacts first-class SDK objects.

Required behavior:

- generated artifacts are addressable by virtual path;
- artifact manifests contain provenance;
- commands can return artifact references;
- artifacts can be listed, shown, exported, and reused by later commands.

## Phase 5: Native Core Extraction

Move only runtime-neutral pieces to `native/msp-core`:

- command request/result JSON protocol;
- parser and quoting rules;
- policy request/decision schema;
- audit record schema;
- artifact manifest schema;
- conformance fixture runner.

Do not move ReadOS document services, WinUI state, PDF libraries, provider secrets, or domain commands into the native core.

## Validation Rules

Every command added to MSP must have:

- parser coverage when it depends on quoting or argument shape;
- runtime coverage for success and failure exit codes;
- argument-specific metadata coverage when subcommands have different side effects;
- audit coverage;
- policy behavior if it can mutate user state;
- artifact coverage if it creates durable output;
- at least one ReadOS host integration check if it touches workspace, PDF, chat, or artifacts.

## Verification

Run managed tests:

```powershell
dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj
```

Run the full MSP verification path:

```powershell
.\scripts\verify-msp.ps1
```

The full script runs Rust format/tests/clippy/build, a native FFI smoke test, .NET MSP tests, and a solution build.
