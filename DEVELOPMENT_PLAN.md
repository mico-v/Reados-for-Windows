# ReadOS Development Plan

## Purpose

This plan turns ReadOS into an MSP-first vertical application service. The current WinUI document workbench and MSP runtime are the starting point; the next phase is to make MSP the product architecture instead of a hidden helper inside a reader.

Day-to-day execution is tracked in [docs/DEVELOPMENT_TRACKER.md](docs/DEVELOPMENT_TRACKER.md). Update that tracker whenever a planned item starts, changes scope, passes verification, or is deferred.

## Current Implementation Progress

Implemented:

- WinUI 3 app frame with library, reader, chat, settings, and workspace persistence.
- PDF import, rendering, text extraction, search, page labels, outlines, region attachments, and per-document conversations.
- OpenAI-compatible chat service with offline fallback.
- Portable .NET MSP runtime in `src/ReadOS.Msp`.
- First `ReadOS.Msp.Hosting` project boundary with host-neutral command host, session projection, artifact catalog, approval grant, and active-command cancellation contracts.
- Host-neutral session summary projection in `ReadOS.Msp.Hosting`, with app workspace models adapted back into observable `MspSessionEntry` state.
- Host-neutral artifact catalog filtering, lookup, content fallback, and evidence metadata checks in `ReadOS.Msp.Hosting`, with app workspace artifacts adapted back into UI models.
- Host-neutral artifact provenance source classification in `ReadOS.Msp.Hosting`, with App mapping source references into inspector lineage rows.
- Workspace-backed session projection store adapter in `ReadOS.App` that routes transcript refresh, persist, remove, and rebuild operations through the Hosting session store contract.
- Host-neutral `/sessions` and `/transcripts` virtual workspace path, listing, lookup, and size projection in `ReadOS.Msp.Hosting`.
- Host-neutral MSP command request construction and approved-command execution orchestration around `IMspCommandHost` and approval grants.
- Host-neutral command request metadata normalization for default and per-request actor, session, and working-directory values.
- Host-neutral approved-command actor normalization before approval grants and request creation.
- Host-neutral approval-grant metadata normalization for actor matching and environment keys.
- Host-neutral approval token normalization for approval environments, revocation, and policy consumption.
- Host-neutral command registry composition builder that combines the core MSP registry with app-provided command packs.
- Host-neutral command registry composition input validation for core registry factories and host command packs.
- Host-neutral command-name validation shared by command packs and custom core registry composition.
- App-side ReadOS command-pack descriptor/factory that owns document, PDF, chat, attachment, and workflow command construction before Hosting registry composition.
- Host-neutral command-pack descriptor metadata for pack names, command names, and core command override reporting.
- Host-neutral command-pack validation for pack names, null commands, empty, untrimmed, whitespace-containing, or token-unsafe command names, duplicate host command names, and explicit core command override support.
- Host-neutral runtime host factory that composes workspace, registry, policy, audit sink, context, and runtime instances.
- Host-neutral runtime host constructor guards for context, runtime, and command-host dependencies.
- Host-neutral runtime host factory validation for audit sink factory inputs and default audit sink creation.
- Host-neutral runtime host factory working-directory normalization and required dependency contract coverage.
- Host-neutral runtime command-host adapter that exposes `MspRuntime` through `IMspCommandHost` for normal and streaming execution.
- Host-neutral runtime command-host adapter request guards for normal and streaming execution.
- Host-neutral string-command host facade that owns default request creation plus approved normal/streaming string-command execution.
- Host-neutral command-host facade contract coverage for constructor guards and actor normalization.
- Host-neutral command-host diagnostics projection for request defaults, command-pack names, command counts, host command names, and core override reporting.
- App-owned MSP host dependency object that groups workspace, PDF, chat, state providers, attachment hooks, and chat sinks before host construction.
- App-owned MSP host runtime factory that assembles virtual workspace, app command pack, command composition, request defaults, approval grants, operator policy, and runtime host.
- Command parser, command registry, runtime context, command results, policy interface, audit sink, and virtual workspace abstraction.
- Core MSP command context dependency guards for workspace, registry, policy, and audit components.
- Core MSP command context working-directory normalization for direct context construction and context updates.
- Core MSP command registry validation for null commands, command names, and empty lookup keys.
- Core MSP runtime constructor and request guards for normal and streaming execution.
- Core commands: `help`, `pwd`, `echo`, `ls`, `cat`.
- ReadOS MSP host adapter in `src/ReadOS.App/Services/Msp`, with operator approval policy split into a separately tested component.
- ReadOS virtual workspace paths for settings, projects, library, documents, pages, outlines, and conversations.
- ReadOS MSP host dependencies grouped behind an app-owned dependency object before virtual workspace, command-pack, policy, and runtime composition.
- Domain commands: `workspace info`, `library list`, `pdf inspect`, `pdf text`, `pdf search`.
- Argument-specific command metadata, effect-based policy checks, and operator approval retry for mutating MSP commands.
- Core transcript record contract, persisted workbench transcript state, and `/transcripts` virtual workspace projection.
- Approval-gated `attach page` and `attach range` commands for queueing evidence into the active chat.
- Approval-gated `chat ask` command for writing model answers into the active document conversation.
- Command preview diagnostics that flow through policy requests, audit records, and persisted transcript entries.
- Runtime command invocation metadata for actor, command text, dry-run state, session ID, and start time.
- Artifact provenance fields, result metadata, and `.manifest.json` sidecar projections under `/artifacts`.
- Approval-gated artifact rename/delete MSP commands with explicit policy previews, audit effects, manifest projection handling, and ReadOS workspace persistence.
- Streaming MSP command events for started, policy decision, progress, completed, and canceled states.
- Agent/MSP bridge instruction text, command request parsing, and MSP execution report formatting are extracted into a separately tested app service.
- Workbench transcript progress display and operator cancellation for running MSP commands.
- Thread timeline projection for chat messages, MSP transcript records, and artifacts is extracted into a separately tested app service.
- Timeline item action routing for artifacts, evidence, MSP approvals, diagnostics, and command records is extracted into a separately tested app service.
- Durable MSP session records that group transcript entries, artifacts, approvals, and last command state.
- MSP transcript persistence and session projection are extracted into a separately tested app service before a future hosting-layer split.
- MSP transcript workspace refresh, persist, remove, and full-session rebuild entry points are wrapped by a separately tested app service.
- MSP command transcript projection for running, streaming events, completion, cancellation, diagnostics, and artifact summaries is extracted into a separately tested app service.
- Active MSP command cancellation ownership is extracted into a separately tested app service that tracks current entry IDs and linked cancellation tokens.
- Structured MSP command diagnostics with stable codes, recovery hints, transcript summaries, and session failure counts.
- MSP session context rows now show running, pending-approval, failed/review, and completed states, and selecting a session routes the latest relevant transcript into the Run or Policy inspector.
- MSP session visibility, filtering, stale-selection detection, and session-to-transcript inspector routing are extracted into a separately tested app service.
- Composer queued evidence is now visible as removable chips, so operators can edit the pending evidence set before sending a prompt.
- Composer attachment queue snapshots, queued prompt restore behavior, remove-by-id logic, and artifact attachment creation are extracted into a separately tested app service.
- Chat turn prompt resolution and user/assistant/MSP/failure message construction are extracted into a separately tested app service.
- Conversation list projection, message projection, and create-if-needed conversation rules are extracted into a separately tested app service.
- Pending MSP approval review eligibility, visible transcript removal, and denied transcript generation are extracted into a separately tested app service.
- Composer primary action now switches between sending prompts and stopping the active MSP command based on runtime state.
- Pending approval indicators in the global toolbar and composer now open the Policy inspector with the pending MSP transcript selected.
- Pending approval navigation selection, Policy inspector target, and status messaging are extracted into a separately tested app service.
- Active document context projection for selected documents, page jump text, label drafts, and load decisions is extracted into a separately tested app service.
- Document collection refresh projection for ordered outlines and visible conversations is extracted into a separately tested app service.
- Presenter load preparation for missing, text, empty, and PDF documents is extracted into a separately tested app service.
- Thumbnail load preparation and thumbnail label/selection projection are extracted into a separately tested app service.
- Page navigation target resolution, page-label parsing, and persistence intent are extracted into a separately tested app service.
- Page-label save and auto-map editing rules are extracted into a separately tested app service.
- Outline text parsing, generated-outline fallback, manual add, and delete rules are extracted into a separately tested app service.
- Document search preparation, result status projection, and hit-to-page routing are extracted into a separately tested app service.
- Reader attachment preparation for current page, page ranges, text files, and region selections is extracted into a separately tested app service.
- Reader navigation, workspace layout preset, sidebar/tab selection, outline toggle, and run drawer pinning decisions are extracted into a separately tested app service.
- Hosting-split readiness is captured in a separately tested app service, and the first `ReadOS.Msp.Hosting` boundary now owns reusable approval-grant, active-command cancellation, command request/approval orchestration, command registry composition, command-host diagnostics, session projection, session/transcript workspace projection, artifact catalog, artifact provenance classification, and session store contracts while App remains responsible for document/PDF/chat adapters, operator policy mode, workspace persistence, app command construction, export/clipboard/chat artifact actions, and WinUI projection.
- Composer queue state now captures draft text plus evidence while MSP work is running and restores queued items without sending automatically.
- Composer and Policy inspector approval-mode controls now switch between policy approval, confirm-all, and allow-workspace modes, and those choices affect subsequent MSP policy decisions.
- Operator approval policy is extracted from the MSP host and independently tested for one-shot approval tokens, confirm-all behavior, and allow-workspace effect handling.
- Composer steer/resume controls can turn the current draft plus selected artifact into a reviewable `workflow run refine-artifact` command without sending or executing it automatically.
- Unresolved PDF/document targets return stable ReadOS diagnostics with command targets and recovery hints that point agents toward `library list` and selected-document state.
- `chat ask` model-provider failures return stable ReadOS diagnostics instead of generic runtime exceptions, preserving queued evidence and avoiding partial conversation/artifact writes.
- Document metadata failures such as invalid PDF pages and missing outline selectors return ReadOS diagnostics that point operators back to `pdf inspect current`.
- `pdf text --artifact` and `pdf search --artifact` command outputs that create durable evidence artifacts with automatic source document/page provenance.
- `chat ask --artifact` command output that writes model answers into durable Markdown artifacts with queued-evidence provenance.
- `workflow summary current|<session-id>` command output that summarizes session commands, failures, artifacts, and recovery hints, with optional Markdown artifact output carrying session/transcript provenance.
- `workflow run summarize-current` and `workflow run review-failures` named workflow commands that read session/transcript projections and can persist generated workflow reports as provenance-backed Markdown artifacts, including inherited source document/page provenance from session artifact manifests.
- App-side `workflow run explain-section` document workflow that resolves a PDF outline section, extracts source pages, calls the configured chat model, and persists a Markdown artifact with document/page provenance.
- App-side `workflow run extract-evidence` document workflow that resolves a PDF outline section and persists page-level structured JSON evidence with document/page provenance.
- App-side `workflow run review-evidence` artifact composition workflow that reads structured evidence JSON and writes Markdown review notes with inherited source provenance.
- App-side `workflow run synthesize-evidence` model workflow that reads structured evidence JSON, calls the configured chat model, and writes source-grounded Markdown synthesis with inherited provenance.
- App-side `workflow run refine-artifact` steer/resume workflow that refines an existing artifact with an operator instruction while inheriting citation provenance.
- Artifacts inspector lineage that labels source artifacts, sidecar manifests, virtual page paths, source documents, and page ranges, with navigation back to openable source artifacts.
- Guided Inspector actions that compose exact `workflow run ...` command drafts from selected PDF outline sections or artifacts, then route operators to the Run inspector for review and execution.
- Guided workflow MSP command composition for document, evidence, refinement, and failure-review drafts is extracted into a separately tested app service.
- Guided workflow validation and artifact path-selection decisions are extracted into a separately tested app service.
- Artifact reuse actions in the Artifacts inspector for preview, clipboard copy, local export, and chat attachment.
- Selected-artifact preview, copy, export, and chat-attachment preparation are extracted into a separately tested app service.
- Artifact catalog, lineage, content preview, export metadata, evidence detection, and workflow path rules are extracted into a separately tested app service.
- Artifact lineage open-source routing is extracted into a separately tested app service.
- Policy inspector failure-review presets that compose `workflow run review-failures --artifact ...` from selected failed transcript context.
- Run inspector prepared-command history that keeps the latest workflow draft presets available for restore without executing them automatically.
- Prepared MSP command history recording, duplicate promotion, title normalization, and cap behavior are extracted into a separately tested app service.
- Rust `native/msp-core` prototype and FFI smoke script.
- MSP test project with parser/runtime/audit coverage and app-level virtual workspace coverage.

### UI Layout Redesign (2026-07-03)

The app frame was reworked from a nested 5-column layout into a flat 3-column responsive shell:

- **MainWindow.xaml** — 3-column layout: Sidebar | Chat thread | Unified Inspector, replacing the old nested ChatSurface+Inspector+PresenterSurface stacking.
- **Title bar** — Right-side toolbar buttons removed (no longer overlap with native caption buttons). Import, new thread, theme, and settings moved to compact 28×28 icon buttons left of the drag region.
- **Splitter interaction** — Drag columns widened from 1px to 6px, Thumb from 10px to 16px. Auto-collapse threshold raised from `Min/2` to full `Min` (Sidebar 200px, Inspector 280px).
- **Collapse icons** — Replaced `&#xE8BB;`/`&#xE8A0;` with `&#xE76B;` (ChevronLeft) and `&#xE76C;` (ChevronRight) for clearer direction affordance.
- **InspectorView** — New unified review panel that merges the old inspector (context/actions/evidence/attachments tabs) with the presenter surface (preview/outline/search tabs).
- **LayoutService** — Centralised responsive column-width computation with four breakpoints (Compact/Medium/Wide/ExtraWide), replacing ad-hoc code-behind width logic.
- **LayoutModels** — `LayoutConfiguration` and `LayoutBreakpoint` types for the computed layout state.
- **SplitPane control** — Reusable `Controls/SplitPane.xaml` UserControl with `PrimaryContent`/`SecondaryContent` dependency properties and built-in drag-to-collapse.
- **Thread timeline projection** — Timeline records now expose visible record-type labels and distinguish MSP approval, running, canceled, completed, failed, artifact, evidence, and message states at the model layer. The chat surface renders separate body sections for MSP approvals, running commands, results, failures, artifacts, evidence, and messages, and timeline/session actions route records into the matching inspector context with selected transcript detail cards in Run and Policy.

### XAML Compiler Bug Workarounds

Two WinUI 3 / .NET 10 XAML compiler issues were discovered and fixed during the redesign:

1. **WMC9999 internal crash** — Mixing `{StaticResource}` markup extensions with literal values inside `Thickness`-typed properties (e.g. `Padding="{StaticResource SpacingMd},0"`) crashes the XAML compiler. Fixed by using hardcoded values where a Thickness is needed.

2. **Runtime XamlParseException** — Assigning a `{StaticResource x:Double}` to a `Thickness` property (e.g. `Padding="{StaticResource SpacingMd}"`) compiles but fails at runtime because WinUI 3 `Thickness` has no implicit conversion from `Double`. The compiler generates a direct property assignment without invoking the type converter. All such occurrences in `MainWindow.xaml`, `ChatSurfaceView.xaml`, and `InspectorView.xaml` were replaced with literal values.

The main gap is not another reader feature. The remaining near-term product gaps are composer/runtime ergonomics, broader named workflows, and eventual extraction of the stable host/session/policy/artifact layer.

## Target Solution Shape

Current layout:

```text
src/
  ReadOS.App/        WinUI operator workbench and domain host adapters
    Controls/          SplitPane reusable control
    Models/            LayoutConfiguration, LayoutBreakpoint
    Services/          LayoutService, PdfDocumentService, WorkspaceStore, AiChatService, MSP host
    Views/             InspectorView, ChatSurfaceView, WorkspaceSidebarView, SettingsView, PresenterSurfaceView
    ViewModels/        ShellViewModel
  ReadOS.Msp/        .NET MSP runtime, SDK contracts, command model
  ReadOS.Msp.Hosting/
    Artifacts/       host-neutral artifact catalog and provenance services
    Commands/        active command cancellation registry
    Policy/          approval grant store
    Runtime/         command host contract, string-command facade, runtime command-host adapter, command-pack descriptor, command composition, diagnostics projection, runtime host factory
    Sessions/        session projection contract, projection service, and virtual workspace path projection
tests/
  ReadOS.Msp.Tests/  parser/runtime/workspace tests
  ReadOS.Msp.Hosting.Tests/
                    hosting contract/service tests
native/
  msp-core/          Rust native core prototype
docs/
  UI_UX_DESIGN.md, MSP_PLAN.md, MSP_SDK_DEVELOPMENT_PLAN.md, MSP_AGENT_COMMAND_LOOP.md, ENVIRONMENT_SETUP.md
```

Near-term target:

```text
src/
  ReadOS.App/          WinUI workbench and document-domain UI
  ReadOS.Msp/          core .NET SDK contracts and runtime
  ReadOS.Msp.Hosting/  service host/session/policy/artifact layer
tests/
  ReadOS.Msp.Tests/
  ReadOS.Msp.Hosting.Tests/
native/
  msp-core/
```

Grow `ReadOS.Msp.Hosting` only with host-neutral contracts and services that are covered by separate tests. Keep document/PDF/chat adapters and observable WinUI state in `ReadOS.App`.

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

Current status: command transcripts are visible in the workbench, persisted with workspace state, readable under `/transcripts`, grouped into durable session records under `/sessions`, and replayable through one-shot approval tokens for approved mutating commands. Runtime requests now carry a session ID into policy, audit, command execution context, transcripts, artifacts, and sessions. `MspRuntime.ExecuteStreamingAsync` and `ReadOsMspHost.ExecuteStreamingAsync` publish command lifecycle/progress events, and `pdf text`, `pdf search`, and `chat ask` report progress during long-running work. The workbench consumes those events to update transcript progress and cancel the active MSP command. Failed commands now carry structured diagnostics and recovery hints into audit records, transcript summaries, agent reports, and session failure counts. `ReadOS.Msp.Hosting` now exists with host-neutral contracts plus tested approval-grant, active-command cancellation, command request/approval orchestration, string-command execution facade, command-pack metadata, command registry composition, command-host diagnostics projection, runtime host composition, runtime command-host adapter, session projection, session/transcript workspace projection, artifact catalog, artifact provenance classification, and session store surfaces; app adapters, operator policy mode, workspace persistence, app command construction, UI projection, and runtime dispatch stay in their current projects. The app readiness report now treats command-host diagnostics as internal host metadata until loaded command packs, overridden core commands, startup validation, or plugin loading become actionable workbench state.

### Milestone 3: Artifact System

Status: started.

- Add `/artifacts` to the virtual workspace.
- Persist generated Markdown, JSON, extracted snippets, summaries, and exported attachments.
- Attach provenance: command text, source paths, document IDs, page ranges, timestamps, and actor.
- Add `artifact list`, `artifact show`, and `artifact write`.

Current status: `/artifacts` is projected in the ReadOS virtual workspace, text artifacts are persisted in workspace state, `artifact list/show/write/rename/delete` are available through MSP, and artifact-producing or destructive commands are treated as mutating operations by policy. Artifact rename/delete expose explicit preview targets and audited `DeleteWorkspace` effects before execution. Artifact results and persisted records now include source command, actor, session ID, timestamps, preview, media type, and source reference fields, with readable sidecar manifests such as `/artifacts/summary.md.manifest.json`. `pdf text --artifact` writes extracted PDF text directly to artifacts and records source document IDs, virtual page paths, and page ranges. `pdf search --artifact` writes tab-separated hit lists and records the matched source pages. `chat ask --artifact` writes model answers to Markdown artifacts and records queued evidence attachments as source document/page provenance. `workflow summary --artifact`, `workflow run summarize-current --artifact`, and `workflow run review-failures --artifact` write workflow-level Markdown reports with session/transcript source paths. Named session workflow artifact output also inherits source document/page provenance from artifact manifests referenced by the current session. `workflow run explain-section --artifact` writes a document-section explanation artifact with source document ID, page range, and virtual page paths. `workflow run extract-evidence --artifact` writes `application/json` page-level evidence records with the same source document/page provenance. `workflow run review-evidence --artifact` reads those evidence artifacts and writes Markdown review notes that inherit the evidence manifest's source document/page provenance. `workflow run synthesize-evidence --artifact` reads the same evidence artifacts, calls the configured model, and writes Markdown synthesis artifacts with inherited citation provenance. `workflow run refine-artifact --artifact` reads any existing artifact and its manifest, applies an operator instruction through the configured model, and writes a derived Markdown artifact with inherited provenance. The Artifacts inspector now projects those provenance fields as lineage rows, can navigate from a derived artifact back to any matching source artifact, and can preview, copy, export, or attach selected artifacts. Artifact catalog filtering, virtual path lookup, content fallback, evidence metadata checks, and source-reference classification now run through tested host-neutral services.

### Milestone 4: Vertical Document Commands

Read-only:

- `workspace info`
- `library list`
- `pdf inspect current`
- `pdf text current 12 14`
- `pdf search current "query"`
- `cat /documents/{id}/pages/12.txt`
- `workflow summary current`

Mutating or approval-gated:

- `page-label set current 12 "iii"`
- `outline add current 42 "Chapter 3" --level 1`
- `attach page current 12`
- `attach range current 12 18`
- `chat ask current "explain attached pages"`
- `artifact write /artifacts/summary.md`
- `artifact rename /artifacts/summary.md /artifacts/archive/summary.md`
- `artifact delete /artifacts/archive/summary.md`
- `pdf text current 12 14 --artifact /artifacts/excerpts/chapter.md`
- `pdf search current "query" --artifact /artifacts/search/query.tsv`
- `chat ask current "explain attached pages" --artifact /artifacts/chat/explanation.md`
- `workflow summary current --artifact /artifacts/workflows/current.md`

### Milestone 5: Agent Bridge

- Expose one application bridge for command execution.
- Return exit code, stdout, stderr, artifacts, audit records, and approval state.
- Show command transcript and evidence in the workbench.
- Support retry and cancellation for long-running document/model work.

Current status: `ReadOsMspHost` exposes normal and approval-token streaming execution APIs, implements the host-neutral `IMspCommandHost` contract, groups app services/providers/sinks behind `ReadOsMspHostDependencies`, delegates app runtime assembly to `ReadOsMspHostRuntimeFactory`, delegates string-command defaults, request construction, request-level command-host execution, approved-command orchestration, host command-pack metadata, command-pack validation, command-host diagnostics, core-plus-host registry composition, and runtime context construction to Hosting, delegates ReadOS command construction to an app-side command-pack factory, and keeps document/PDF/chat adapters in the app. The app readiness projection marks those extracted command-host boundaries as hosted, keeps operator policy and app command construction in App, and defers command-host diagnostics UI surfacing until the data becomes operator-actionable. The workbench consumes runtime streams for live transcript progress, final result recording, and operator cancellation.

### Milestone 6: Workflow Runtime

Status: started.

- Add command scripts or named workflows once single commands are reliable.
- Support document-centered workflows such as "summarize this chapter", "extract evidence", and "build review notes".
- Keep workflow outputs inspectable as artifacts.

Current status: `workflow summary current|<session-id>` creates a workflow-level Markdown report from durable `/sessions` and `/transcripts` projections. `workflow run summarize-current` produces the same inspectable report through the workflow-run command shape. `workflow run review-failures` produces a focused recovery report for failed transcript entries, including diagnostics and recovery hints. With `--artifact`, session workflow reports are approval-gated, persist Markdown under `/artifacts`, and record session/transcript source provenance. Named session workflow paths additionally inherit source document/page provenance from upstream artifact manifests referenced by the current session. The ReadOS app command pack now overrides `workflow` for `workflow run explain-section`, `workflow run extract-evidence`, `workflow run review-evidence`, `workflow run synthesize-evidence`, and `workflow run refine-artifact`, delegates existing session workflows back to the core runtime command, and implements document outline resolution, page extraction, model explanation, structured evidence output, evidence review notes, evidence-to-model synthesis, steer/resume artifact refinement, artifact writing, and source-page provenance in the app domain layer. The Inspector can compose these workflow commands from selected outline sections, artifacts, and failed transcript diagnostics without executing them immediately, and the Run inspector keeps a compact in-memory history so prepared drafts can be restored after other commands are reviewed or run.

### Milestone 7: Native Core And SDK Extraction

- Move parser/runtime-neutral contracts into `native/msp-core` after .NET behavior stabilizes.
- Keep host adapters in .NET.
- Add conformance fixtures shared by Rust and .NET.
- Package native binaries through a future .NET binding layer only after FFI behavior is stable.

## Immediate Backlog

The active backlog is maintained in [docs/DEVELOPMENT_TRACKER.md](docs/DEVELOPMENT_TRACKER.md). Current priority order:

1. Keep document/PDF/chat adapters and observable workbench UI projection in `ReadOS.App` while Hosting grows.
2. Avoid moving ReadOS-specific command implementations into Hosting; only extract contracts or services that can be tested without app models.
3. Keep command-host diagnostics as internal host metadata until they become actionable startup, support, or plugin-loading state.

Recently closed: core MSP command context working-directory normalization.
Recently closed: core MSP runtime constructor and request guards.
Recently closed: core MSP command registry validation.
Recently closed: core MSP command context dependency guards.
Recently closed: host-neutral runtime host factory working-directory normalization.
Recently closed: host-neutral runtime host constructor guards.
Recently closed: host-neutral runtime command-host adapter request guards.
Recently closed: host-neutral command-host facade contract coverage.
Recently closed: host-neutral approval token normalization.
Recently closed: host-neutral approval-grant metadata normalization.
Recently closed: host-neutral approved-command actor normalization.
Recently closed: host-neutral command request metadata normalization.
Recently closed: host-neutral runtime host audit sink factory validation.
Recently closed: host-neutral core registry command-name validation during command composition.
Recently closed: host-neutral command registry composition input validation.
Recently closed: host-neutral command-name token character validation at the command-pack boundary.
Recently closed: host-neutral command-name whitespace validation at the command-pack boundary.
Recently closed: host-neutral command-name trim validation at the command-pack boundary.
Recently closed: host-neutral command-pack validation for null, empty, duplicate, and core-override command names.
Recently closed: app-side hosting readiness projection refresh and internal command-host diagnostics visibility decision.
Recently closed: host-neutral command-host diagnostics projection for request defaults, command-pack, command counts, and override reporting.
Recently closed: host-neutral string-command host facade for default request creation and approved string execution.
Recently closed: host-neutral runtime command-host adapter for normal and streaming `IMspCommandHost` execution.
Recently closed: app-owned MSP host runtime factory for virtual workspace, command-pack, policy, request, approval, and runtime assembly.
Recently closed: app-owned `ReadOsMspHostDependencies` object for grouped host construction inputs.
Recently closed: host-neutral command-pack descriptor metadata for pack names, command names, and core override reporting.
Recently closed: host-neutral runtime host factory for workspace, registry, policy, audit, context, and runtime composition.
Recently closed: app command-pack descriptor/factory boundary for ReadOS command construction.
Recently closed: host-neutral command registry composition builder.
Recently closed: host-neutral command request factory and approved-command orchestration service.
Recently closed: host-neutral session/transcript virtual workspace projection path service.
Recently closed: host-neutral artifact provenance source classifier with App lineage row mapping.
Recently closed: workspace-backed session projection store adapter for transcript persistence entry points.
Recently closed: host-neutral artifact catalog/read metadata service with app workspace adapter.
Recently closed: host-neutral session projection service with app workspace adapter.
Recently closed: first `ReadOS.Msp.Hosting` project boundary with tested approval-grant and active-command cancellation primitives.
Recently closed: hosting-split readiness projection with tested ownership boundaries.
Recently closed: reader layout/toggle command extraction from the ShellViewModel.
Recently closed: reader attachment command extraction from the ShellViewModel.
Recently closed: document search routing extraction from the ShellViewModel.
Recently closed: outline editing extraction from the ShellViewModel.
Recently closed: page-label editing extraction from the ShellViewModel.
Recently closed: page navigation persistence extraction from the ShellViewModel.
Recently closed: thumbnail load preparation extraction from the ShellViewModel.
Recently closed: presenter load preparation extraction from the ShellViewModel.
Recently closed: document collection refresh extraction from the ShellViewModel.
Recently closed: active document context refresh extraction from the ShellViewModel.
Recently closed: pending approval navigation extraction from the ShellViewModel.
Recently closed: compact command-preset history for prepared workflow drafts.
Recently closed: composer approval-mode controls wired into MSP policy decisions.
Recently closed: composer steer/resume draft action for selected artifacts.
Recently closed: artifact rename/delete MSP commands with explicit destructive policy and audit behavior.
Recently closed: operator approval policy extraction from the ReadOS MSP host.
Recently closed: MSP transcript/session store extraction from the ShellViewModel.
Recently closed: artifact catalog/lineage/reuse service extraction from the ShellViewModel.
Recently closed: MSP command transcript projection extraction from the ShellViewModel.
Recently closed: prepared MSP command history extraction from the ShellViewModel.
Recently closed: active MSP command cancellation extraction from the ShellViewModel.
Recently closed: attachment queue service extraction from the ShellViewModel.
Recently closed: pending approval review flow extraction from the ShellViewModel.
Recently closed: agent MSP bridge instruction, command parsing, and report formatting extraction from the ShellViewModel.
Recently closed: chat turn prompt and message construction extraction from the ShellViewModel.
Recently closed: conversation list/message projection and create-if-needed rules extraction from the ShellViewModel.
Recently closed: MSP session filtering, stale-selection detection, and inspector routing extraction from the ShellViewModel.
Recently closed: MSP transcript workspace refresh/persist/remove/rebuild bridge extraction from the ShellViewModel.
Recently closed: thread timeline projection extraction from the ShellViewModel.
Recently closed: timeline item action routing extraction from the ShellViewModel.
Recently closed: guided workflow command composition extraction from the ShellViewModel.
Recently closed: guided workflow validation and path-selection extraction from the ShellViewModel.
Recently closed: selected artifact preview/copy/export/attachment preparation extraction from the ShellViewModel.
Recently closed: artifact lineage open-source routing extraction from the ShellViewModel.

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
