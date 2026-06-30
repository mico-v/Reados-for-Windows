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
- Workbench transcript progress display and operator cancellation for running MSP commands.
- Durable MSP session records that group transcript entries, artifacts, approvals, and last command state.
- Structured MSP command diagnostics with stable codes, recovery hints, transcript summaries, and session failure counts.
- `pdf text --artifact` and `pdf search --artifact` command outputs that create durable evidence artifacts with automatic source document/page provenance.
- `chat ask --artifact` command output that writes model answers into durable Markdown artifacts with queued-evidence provenance.
- Rust `native/msp-core` prototype and FFI smoke script.
- MSP test project with parser/runtime/audit coverage and app-level virtual workspace coverage.

The main gap is not another reader feature. The remaining service-layer gaps are automatic evidence provenance, workflow-level diagnostics, and orchestration that can safely drive vertical document work.

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
- Keep `MspCommandResult` structured with exit code, stdout/stderr, artifacts, audit records, and diagnostics.
- Add test coverage for quoting, unknown commands, command failure, audit records, and workspace path normalization.
- Define stable JSON examples for command request/result/audit/artifact.

Current status: command metadata, previews, artifacts, audit records, streaming events, and structured diagnostics are implemented in the .NET runtime. Failure diagnostics use stable codes and recovery hints for parse errors, unknown commands, policy confirmation, cancellation, and runtime exceptions.

### Milestone 2: Service Host Layer

Status: started.

- Introduce session IDs and command transcript records.
- Add cancellation and progress event surfaces.
- Replace app-level allow-all execution with a policy service.
- Add approval requests for write-capable commands.
- Persist transcripts under the local workspace.

Current status: command transcripts are visible in the workbench, persisted with workspace state, readable under `/transcripts`, grouped into durable session records under `/sessions`, and replayable through one-shot approval tokens for approved mutating commands. Runtime requests now carry a session ID into policy, audit, command execution context, transcripts, artifacts, and sessions. `MspRuntime.ExecuteStreamingAsync` and `ReadOsMspHost.ExecuteStreamingAsync` publish command lifecycle/progress events, and `pdf text`, `pdf search`, and `chat ask` report progress during long-running work. The workbench consumes those events to update transcript progress and cancel the active MSP command. Failed commands now carry structured diagnostics and recovery hints into audit records, transcript summaries, agent reports, and session failure counts.

### Milestone 3: Artifact System

Status: started.

- Add `/artifacts` to the virtual workspace.
- Persist generated Markdown, JSON, extracted snippets, summaries, and exported attachments.
- Attach provenance: command text, source paths, document IDs, page ranges, timestamps, and actor.
- Add `artifact list`, `artifact show`, and `artifact write`.

Current status: `/artifacts` is projected in the ReadOS virtual workspace, text artifacts are persisted in workspace state, `artifact list/show/write` are available through MSP, and artifact-producing commands are treated as mutating operations by policy. Artifact results and persisted records now include source command, actor, session ID, timestamps, preview, media type, and source reference fields, with readable sidecar manifests such as `/artifacts/summary.md.manifest.json`. `pdf text --artifact` writes extracted PDF text directly to artifacts and records source document IDs, virtual page paths, and page ranges. `pdf search --artifact` writes tab-separated hit lists and records the matched source pages. `chat ask --artifact` writes model answers to Markdown artifacts and records queued evidence attachments as source document/page provenance.

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
- `pdf text current 12 14 --artifact /artifacts/excerpts/chapter.md`
- `pdf search current "query" --artifact /artifacts/search/query.tsv`
- `chat ask current "explain attached pages" --artifact /artifacts/chat/explanation.md`

### Milestone 5: Agent Bridge

- Expose one application bridge for command execution.
- Return exit code, stdout, stderr, artifacts, audit records, and approval state.
- Show command transcript and evidence in the workbench.
- Support retry and cancellation for long-running document/model work.

Current status: `ReadOsMspHost` exposes normal and approval-token streaming execution APIs. The workbench consumes those streams for live transcript progress, final result recording, and operator cancellation.

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

- Add command-specific recovery previews for document metadata and model-provider failures.
- Expand automatic source document/page provenance to named workflows.
- Add workflow grouping and workflow-level failure summaries.

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
