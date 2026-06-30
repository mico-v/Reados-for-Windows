# ReadOS Development Plan

## Purpose

This plan turns ReadOS into an MSP-first vertical application service. The current WinUI document workbench and MSP runtime are the starting point; the next phase is to make MSP the product architecture instead of a hidden helper inside a reader.

## Current Implementation Progress

Implemented:

- WinUI 3 app frame with library, reader, chat, settings, and workspace persistence.
- PDF import, rendering, text extraction, search, page labels, outlines, region attachments, and per-document conversations.
- OpenAI-compatible chat service with offline fallback.
- Portable .NET MSP runtime in `src/ReadOS.Msp`.
- Command parser, command registry, runtime context, command results, policy interface, audit sink, and virtual workspace abstraction.
- Core commands: `help`, `pwd`, `echo`, `ls`, `cat`.
- ReadOS MSP host adapter in `src/ReadOS.App/Services/Msp`.
- ReadOS virtual workspace paths for settings, projects, library, documents, pages, outlines, and conversations.
- Domain commands: `workspace info`, `library list`, `pdf inspect`, `pdf text`, `pdf search`.
- Argument-specific command metadata, effect-based policy checks, and operator approval retry for mutating MSP commands.
- Core transcript record contract, persisted workbench transcript state, and `/transcripts` virtual workspace projection.
- Approval-gated `attach page` and `attach range` commands for queueing evidence into the active chat.
- Approval-gated `chat ask` command for writing model answers into the active document conversation.
- Command preview diagnostics that flow through policy requests, audit records, and persisted transcript entries.
- Runtime command invocation metadata for actor, command text, dry-run state, session ID, and start time.
- Artifact provenance fields, result metadata, and `.manifest.json` sidecar projections under `/artifacts`.
- Streaming MSP command events for started, policy decision, progress, completed, and canceled states.
- Rust `native/msp-core` prototype and FFI smoke script.
- MSP test project with parser/runtime/audit coverage and app-level virtual workspace coverage.

The main gap is not another reader feature. The remaining service-layer gaps are durable sessions, UI-facing cancellation controls, richer diagnostics, and workflow orchestration that can safely drive vertical document work.

## Target Solution Shape

Current layout:

```text
src/
  ReadOS.App/        WinUI operator workbench and domain host adapters
  ReadOS.Msp/        .NET MSP runtime, SDK contracts, command model
tests/
  ReadOS.Msp.Tests/  parser/runtime/workspace tests
native/
  msp-core/          Rust native core prototype
docs/
  MSP plans, environment notes, app frame notes
```

Near-term target:

```text
src/
  ReadOS.App/          WinUI workbench and document-domain UI
  ReadOS.Msp/          core .NET SDK contracts and runtime
  ReadOS.Msp.Hosting/  planned service host/session/policy/artifact layer
tests/
  ReadOS.Msp.Tests/
  ReadOS.Msp.Hosting.Tests/
native/
  msp-core/
```

Create `ReadOS.Msp.Hosting` only when the service concepts are stable enough to test separately. Until then, evolve `ReadOsMspHost` inside the app.

## Architecture Principles

- The model-facing bridge should stay small: `exec_command({ "cmd": "..." })`.
- The service host owns sessions, cancellation, policy, audit, artifacts, and command transcripts.
- The runtime owns parsing, dispatch, stdout/stderr, exit codes, and workspace resolution.
- The app owns domain services: documents, PDF rendering, chat, provider settings, and UI state.
- The virtual workspace is the canonical agent read model.
- Mutating commands must declare side effects before execution and pass policy.
- Generated outputs should become artifacts with paths, media types, provenance, and previews.
- Runtime-neutral behavior should be proven in .NET before extraction into Rust.

## Milestones

### Milestone 0: Direction Alignment

Status: in progress.

- Reframe README, product goal, development plan, and MSP docs around the vertical service direction.
- Keep historical reader docs only where they still explain the first domain.
- Make command examples and future backlog use current repository paths and C#/Rust reality.

### Milestone 1: MSP Contract Hardening

- Add command metadata: name, summary, argument shape, mutability, external effects, and artifact outputs.
- Extend `MspCommandResult` with policy decision and structured diagnostics where useful.
- Add test coverage for quoting, unknown commands, command failure, audit records, and workspace path normalization.
- Define stable JSON examples for command request/result/audit/artifact.

### Milestone 2: Service Host Layer

Status: started.

- Introduce session IDs and command transcript records.
- Add cancellation and progress event surfaces.
- Replace app-level allow-all execution with a policy service.
- Add approval requests for write-capable commands.
- Persist transcripts under the local workspace.

Current status: command transcripts are visible in the workbench, persisted with workspace state, readable under `/transcripts`, and replayable through one-shot approval tokens for approved mutating commands. Runtime requests now carry a session ID into policy, audit, and command execution context. `MspRuntime.ExecuteStreamingAsync` and `ReadOsMspHost.ExecuteStreamingAsync` publish command lifecycle/progress events, and `pdf text`, `pdf search`, and `chat ask` report progress during long-running work.

### Milestone 3: Artifact System

Status: started.

- Add `/artifacts` to the virtual workspace.
- Persist generated Markdown, JSON, extracted snippets, summaries, and exported attachments.
- Attach provenance: command text, source paths, document IDs, page ranges, timestamps, and actor.
- Add `artifact list`, `artifact show`, and `artifact write`.

Current status: `/artifacts` is projected in the ReadOS virtual workspace, text artifacts are persisted in workspace state, `artifact list/show/write` are available through MSP, and only `artifact write` is treated as a mutating artifact operation by policy. Artifact results and persisted records now include source command, actor, session ID, timestamps, preview, media type, and source reference fields, with readable sidecar manifests such as `/artifacts/summary.md.manifest.json`.

### Milestone 4: Vertical Document Commands

Read-only:

- `workspace info`
- `library list`
- `pdf inspect current`
- `pdf text current 12 14`
- `pdf search current "query"`
- `cat /documents/{id}/pages/12.txt`

Mutating or approval-gated:

- `page-label set current 12 "iii"`
- `outline add current 42 "Chapter 3" --level 1`
- `attach page current 12`
- `attach range current 12 18`
- `chat ask current "explain attached pages"`
- `artifact write /artifacts/summary.md`

### Milestone 5: Agent Bridge

- Expose one application bridge for command execution.
- Return exit code, stdout, stderr, artifacts, audit records, and approval state.
- Show command transcript and evidence in the workbench.
- Support retry and cancellation for long-running document/model work.

Current status: `ReadOsMspHost` exposes normal and approval-token streaming execution APIs. The workbench still needs UI controls that consume those events for visible progress and operator cancellation.

### Milestone 6: Workflow Runtime

- Add command scripts or named workflows once single commands are reliable.
- Support document-centered workflows such as "summarize this chapter", "extract evidence", and "build review notes".
- Keep workflow outputs inspectable as artifacts.

### Milestone 7: Native Core And SDK Extraction

- Move parser/runtime-neutral contracts into `native/msp-core` after .NET behavior stabilizes.
- Keep host adapters in .NET.
- Add conformance fixtures shared by Rust and .NET.
- Package native binaries through a future .NET binding layer only after FFI behavior is stable.

## Immediate Backlog

- Display streaming command progress in the workbench transcript and add an operator cancel action.
- Add richer previews and recovery guidance for approval-gated commands.
- Populate source document/page provenance automatically from future document workflow commands.

## Verification

Managed tests:

```powershell
dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj
```

Full MSP verification:

```powershell
.\scripts\verify-msp.ps1
```

Workbench build/run:

```powershell
.\scripts\run.ps1 -BuildOnly
.\scripts\run.ps1
```
