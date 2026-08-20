# ReadOS Development Plan

## Purpose

This plan advances ReadOS as an MSP-first vertical application service. The WinUI workbench, portable runtime, host-neutral Hosting layer, durable workspace, policy/audit path, and document workflows are now the product architecture; the next phase is to prove that architecture through a repeatable product-level workflow and align its Windows runtime-neutral core with upstream MSP behavior.

Day-to-day execution is tracked in [docs/DEVELOPMENT_TRACKER.md](docs/DEVELOPMENT_TRACKER.md). Update that tracker whenever a planned item starts, changes scope, passes verification, or is deferred.

## Current Implementation Progress

Verified baseline (2026-08-12):

- `ReadOS.Msp.Tests`: 69 passed.
- `ReadOS.Msp.Hosting.Tests`: 318 passed (including real release-DLL adapter operations; the native-DLL tests skip only when run standalone without `READOS_MSP_NATIVE_DLL`).
- `ReadOS.App.Tests`: 411 passed.
- Managed total: 798 passed (under the full verifier).
- Rust `native/msp-core`: 108 passed; native binary and FFI smoke verification passed. The candidate release DLL SHA256 is `2CFD14246FA963AC284B158903ADC910A782AFEDFEA4EC5F247692BF4613E49A`.
- `ReadOS.sln`: 0 warnings, 0 errors through the fail-fast verification path.

Trustworthy-boundary work is complete for the current architecture: artifact/session/transcript path and identifier confinement, terminal audit evidence for parse/unknown/policy/exception/cancel paths, fail-fast verification and packaging scripts, pinned .NET SDK plus Windows CI, and DPAPI `CurrentUser` provider credentials outside workspace JSON and exports. This closes T92; it does not close the in-progress T93 product E2E, T94 Workbench hardening, or T95 upstream-aligned Windows core work.

T93 service-level recovery evidence now covers cancellation with exit 130 and no partial artifact, invalid outline-page diagnostics with no artifact after restart, and provider failure/restart/retry without API-key or sensitive-request leakage while preserving lineage and audit. Visible WinUI, a real provider/network boundary, and the flagship workflow across a packaged-process restart remain open.

T95 now includes the handle-based read-only fixed-NTFS WorkspaceFS, Rust path sanitizer, stable `reados-msp-native/1` Hosting adapter, static MSVC CRT, real release-DLL tests, the completed `native_pwd_echo_adoption_v1` product slice, the completed `native_abi_v2_handshake_v1` boundary/release slice, and the completed `native_command_core_registry_v1` runtime/release slice, and the completed `native_mixed_workspace_read_v1` bridge slice (ABI v2 operation 4 serving virtual/mixed reads through the callback host table, verified via the real release DLL), and the completed `native_stream_core_v1` stream slice (bounded byte-stream primitives mirroring `MSPCommandStream.swift`), and the completed `mutable_workspace_write_v1`/`recoverable_workspace_trash_v1` slices (TOCTOU-safe writes + hidden `.msp/trash`), and the completed pipelines/redirection slice (byte-stream pipeline execution with `&&`/`||`/`!` exit rules and WorkspaceFS file redirections), and the completed model-facing exec sessions slice (`exec_command`/`write_stdin`), and the completed Windows ConPTY/Job Object process backend + UTF-16LE sanitizer slice, and the completed ConPTY session integration slice (external processes wired into exec sessions). Managed `MspRuntime` remains the control plane for parse, policy/approval, dry-run, streaming events, terminal results, and exactly-once product audit; only canonical lowercase `pwd` and `echo` execute through Rust. Case variants fail closed without managed fallback, while `ls`, `cat`, `help`, and app-domain commands remain managed. ABI v2 uses a verified 32-byte `2.0` handshake (`0x324D534F44414552`, capabilities `0x3F` with required `0xF`) and explicit pointer/length buffers; only total absence of its three exports allows the isolated v1 fallback. The full verifier and 12-case baseline/candidate ABI v1/v2 differential pass. The latest completed no-skip package is `0.1.0-native-command-registry-verified`; staged/package smoke reports `LengthDelimitedV2` 2.0, 532 entries, `RawMSP`/PDB/`.git` counts of zero, five native commands exiting 0, and three proxy audit counts of 1. The next work is broader product adoption and wider conformance.

Implemented:

- WinUI 3 app frame with library, reader, chat, settings, and workspace persistence.
- PDF import, rendering, text extraction, search, page labels, outlines, region attachments, and per-document conversations.
- OpenAI-compatible chat service with offline fallback.
- Provider API key abstraction backed by Windows DPAPI `CurrentUser`, with provider scoping, legacy plaintext migration, missing-credential behavior, and workspace export exclusion.
- Portable .NET MSP runtime in `src/ReadOS.Msp`.
- Shared namespace/path/identifier validation that confines artifact operations, workflow session/transcript reads, and manifest lookups to their virtual namespaces.
- Complete terminal result/audit behavior for success, parse failure, unknown command, policy confirmation/denial, command exception, and cancellation.
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
- Typed timeline rendering for message, evidence, approval, running command, result, failure, and artifact records.
- Timeline item action routing for artifacts, evidence, MSP approvals, diagnostics, and command records is extracted into a separately tested app service.
- Durable MSP session records that group transcript entries, artifacts, approvals, and last command state.
- MSP transcript persistence and session projection use separately tested app adapters over the established Hosting session-store boundary.
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
- Bottom Runtime Drawer with transcript status, output previews, progress/approval/cancel actions, open/close, and in-memory pin state.
- Compact sidebar and Review Dock flyouts plus remembered non-compact pane widths, with responsive layout service coverage.
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
- Restart-capable flagship service integration coverage for PDF import, outline-driven extraction, approval restoration, synthesis, durable session/transcript/artifact/manifest state, denied refinement, lineage, and source-page navigation.
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
- Three managed test projects covering the core runtime, host-neutral Hosting layer, and ReadOS app services.
- Fail-fast Rust/native/managed/solution verification, fail-fast release packaging across all three managed test projects, a pinned .NET SDK, and Windows CI using the same verifier.

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
- **Runtime Drawer** — The bottom drawer renders transcript state, output previews, progress, approval/deny/cancel actions, and open/close/pin controls. T94 owns resizing, persisted height/pin state, selected-command details, and full stdout/stderr/diagnostic supervision.

### XAML Compiler Bug Workarounds

Two WinUI 3 / .NET 10 XAML compiler issues were discovered and fixed during the redesign:

1. **WMC9999 internal crash** — Mixing `{StaticResource}` markup extensions with literal values inside `Thickness`-typed properties (e.g. `Padding="{StaticResource SpacingMd},0"`) crashes the XAML compiler. Fixed by using hardcoded values where a Thickness is needed.

2. **Runtime XamlParseException** — Assigning a `{StaticResource x:Double}` to a `Thickness` property (e.g. `Padding="{StaticResource SpacingMd}"`) compiles but fails at runtime because WinUI 3 `Thickness` has no implicit conversion from `Double`. The compiler generates a direct property assignment without invoking the type converter. All such occurrences in `MainWindow.xaml`, `ChatTimelineView.xaml`, and `InspectorView.xaml` were replaced with literal values.

The main gap is not another reader feature or another Hosting split. The remaining near-term product gaps are the T93 product-level workflow proof, T94 Workbench hardening, and T95 upstream-aligned Windows Rust core plus stable .NET adapter.

## Target Solution Shape

Current layout:

```text
src/
  ReadOS.App/        WinUI operator workbench and domain host adapters
    Controls/          SplitPane reusable control
    Models/            LayoutConfiguration, LayoutBreakpoint
    Services/          LayoutService, PdfDocumentService, WorkspaceStore, AiChatService, MSP host
    Views/             ChatTimelineView, InspectorView, WorkspaceSidebarView, SettingsView
    ViewModels/        ShellViewModel
  ReadOS.Msp/        .NET MSP runtime, SDK contracts, command model
  ReadOS.Msp.Hosting/
    Artifacts/       host-neutral artifact catalog and provenance services
    Commands/        active command cancellation registry
    Native/          stable internal managed/native adapter
    Policy/          approval grant store
    Runtime/         command host contract, string-command facade, runtime command-host adapter, command-pack descriptor, command composition, diagnostics projection, runtime host factory
    Sessions/        session projection contract, projection service, and virtual workspace path projection
tests/
  ReadOS.Msp.Tests/  parser/runtime/workspace tests
  ReadOS.Msp.Hosting.Tests/
                    hosting contract/service tests
  ReadOS.App.Tests/  app services, workflows, persistence, security, and UI projection tests
native/
  msp-core/          Windows runtime-neutral Rust core mainline
MSP/                 nested local Apache-2.0 reference only; ignored and not packaged
docs/
  UI_UX_DESIGN.md, MSP_PLAN.md, MSP_SDK_DEVELOPMENT_PLAN.md, MSP_AGENT_COMMAND_LOOP.md, ENVIRONMENT_SETUP.md
```

Next contract target:

```text
src/
  ReadOS.App/          WinUI workbench and document-domain UI
  ReadOS.Msp/          temporary managed product runtime and compatibility oracle
  ReadOS.Msp.Hosting/  stable native adapter plus service host/session/policy/artifact layer
tests/
  ReadOS.Msp.Tests/
  ReadOS.Msp.Hosting.Tests/
  ReadOS.App.Tests/
native/
  msp-core/            Windows-compatible Rust runtime-neutral MSP core
MSP/                   local reference checkout only; absent from Git/package output
```

The Hosting project boundary is established. Grow it only with host-neutral contracts and services that are covered by separate tests. Keep document/PDF/chat adapters, provider credential storage, workspace persistence, and observable WinUI state in `ReadOS.App`. T95 must preserve this ownership boundary while moving runtime-neutral behavior behind a stable .NET-to-Rust adapter.

The root `MSP/` repository is a nested, local-only Apache-2.0 source-package/reference input, not a ReadOS source subtree, CI dependency, runtime input, parent Git payload, or package payload. Its 2026-08-18 Windows source package contains a 24-crate/two-example Cargo workspace, while `SOURCE-PACKAGE-README.md` excludes Mac/Swift implementation source; this checkout contains only `Implementations/Swift/Sources/Tools`. The documented parity inventory runner/report and required release-gate Python dependencies are absent, so the versioned [Windows capability manifest](conformance/msp-upstream/windows-capability-manifest.json) records source inventory and full upstream release evidence as `blocked`. Raw `MSP/` content must not enter ReadOS package output. Copied or derived Apache-2.0 code/fixtures require NOTICE, modification, and source-provenance records in the ReadOS distribution surface that contains them.

## Architecture Principles

- The model-facing bridge should stay small: `exec_command({ "cmd": "..." })`.
- The service host owns sessions, cancellation, policy, audit, artifacts, and command transcripts.
- The runtime owns parsing, dispatch, stdout/stderr, exit codes, and workspace resolution.
- The app owns domain services: documents, PDF rendering, chat, provider settings, and UI state.
- The virtual workspace is the canonical agent read model.
- Mutating commands must declare side effects before execution and pass policy.
- Generated outputs should become artifacts with paths, media types, provenance, and previews.
- Runtime-neutral behavior should be compared against `MSP/Spec`, available `MSP/Conformance` fixtures/reference outputs, and the committed `conformance/msp-upstream/` snapshots. Swift implementation paths cited by older plan text are not present in this source package; do not claim local Swift-source evidence that the checkout cannot provide.
- ReadOS integrates the Rust core through a stable .NET adapter; domain commands and UI services do not call an unstable FFI surface directly.
- Provider secrets must remain outside virtual workspace JSON, exports, audit, transcript, artifacts, adapter payloads, and conformance inputs.
- Upstream reference use must remain traceable and license-correct; the local `MSP/` checkout is never a ReadOS package payload.
- Virtual namespace membership must be checked after normalization; string-prefix checks and unvalidated record IDs are not security boundaries.

## Milestones

### Milestone 0: Direction Alignment

Status: done for the current repository baseline; ongoing documents must stay aligned with the tracker.

- Reframe README, product goal, development plan, and MSP docs around the vertical service direction.
- Keep historical reader docs only where they still explain the first domain.
- Make command examples and future backlog use current repository paths and C#/Rust reality.

### Milestone 1: MSP Contract Hardening

Status: implemented for the current .NET runtime boundary. Upstream behavior mapping and a stable .NET/Rust adapter remain in progress under T95.

- Add command metadata: name, summary, argument shape, mutability, external effects, and artifact outputs.
- Keep `MspCommandResult` structured with exit code, stdout/stderr, artifacts, audit records, and diagnostics.
- Add test coverage for quoting, unknown commands, command failure, audit records, and workspace path normalization.
- Keep managed request/result/audit/artifact serialization stable where the .NET adapter needs it, without treating a ReadOS-only JSON envelope as the MSP specification.

Current status: command metadata, previews, artifacts, audit records, streaming events, and structured diagnostics are implemented in the .NET runtime. Failure diagnostics use stable codes and recovery hints for parse errors, unknown commands, policy confirmation/denial, cancellation, policy/command exceptions, and runtime exceptions. Every command attempt now produces terminal result/audit evidence, with `NotEvaluated` used when parsing or pre-policy cancellation prevents authorization. Artifact, session, transcript, and workflow manifest paths use normalized namespace/identifier validation. T95 must map these semantics to upstream MSP contracts and conformance evidence rather than freezing the current managed shape as a new protocol.

### Milestone 2: Service Host Layer

Status: implemented as an active host-neutral project boundary.

- Introduce session IDs and command transcript records.
- Add cancellation and progress event surfaces.
- Replace app-level allow-all execution with a policy service.
- Add approval requests for write-capable commands.
- Persist transcripts under the local workspace.

Current status: command transcripts are visible in the workbench, persisted with workspace state, readable under `/transcripts`, grouped into durable session records under `/sessions`, and replayable through one-shot approval tokens for approved mutating commands. Runtime requests now carry a session ID into policy, audit, command execution context, transcripts, artifacts, and sessions. `MspRuntime.ExecuteStreamingAsync` and `ReadOsMspHost.ExecuteStreamingAsync` publish command lifecycle/progress events, and `pdf text`, `pdf search`, and `chat ask` report progress during long-running work. The workbench consumes those events to update transcript progress and cancel the active MSP command. Failed commands now carry structured diagnostics and recovery hints into audit records, transcript summaries, agent reports, and session failure counts. `ReadOS.Msp.Hosting` now exists with host-neutral contracts plus tested approval-grant, active-command cancellation, command request/approval orchestration, string-command execution facade, command-pack metadata, command registry composition, command-host diagnostics projection, runtime host composition, runtime command-host adapter, session projection, session/transcript workspace projection, artifact catalog, artifact provenance classification, and session store surfaces; app adapters, operator policy mode, workspace persistence, app command construction, UI projection, and runtime dispatch stay in their current projects. The app readiness report now treats command-host diagnostics as internal host metadata until loaded command packs, overridden core commands, startup validation, or plugin loading become actionable workbench state.

### Milestone 3: Artifact System

Status: implemented for the current artifact lifecycle and virtual workspace.

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

Status: implemented for the named workflows listed below; product-level restart/recovery acceptance is in progress under T93.

- Add command scripts or named workflows once single commands are reliable.
- Support document-centered workflows such as "summarize this chapter", "extract evidence", and "build review notes".
- Keep workflow outputs inspectable as artifacts.

Current status: `workflow summary current|<session-id>` creates a workflow-level Markdown report from durable `/sessions` and `/transcripts` projections. `workflow run summarize-current` produces the same inspectable report through the workflow-run command shape. `workflow run review-failures` produces a focused recovery report for failed transcript entries, including diagnostics and recovery hints. With `--artifact`, session workflow reports are approval-gated, persist Markdown under `/artifacts`, and record session/transcript source provenance. Named session workflow paths additionally inherit source document/page provenance from upstream artifact manifests referenced by the current session. The ReadOS app command pack now overrides `workflow` for `workflow run explain-section`, `workflow run extract-evidence`, `workflow run review-evidence`, `workflow run synthesize-evidence`, and `workflow run refine-artifact`, delegates existing session workflows back to the core runtime command, and implements document outline resolution, page extraction, model explanation, structured evidence output, evidence review notes, evidence-to-model synthesis, steer/resume artifact refinement, artifact writing, and source-page provenance in the app domain layer. The Inspector can compose these workflow commands from selected outline sections, artifacts, and failed transcript diagnostics without executing them immediately, and the Run inspector keeps a compact in-memory history so prepared drafts can be restored after other commands are reviewed or run.

### Milestone 7: Upstream-Aligned Windows Native Core And Adapter

Status: in progress as T95.

- Treat local `MSP/Spec`, available `MSP/Conformance` fixtures/reference outputs, and committed `conformance/msp-upstream/` snapshots as upstream behavior references; the Windows source package is a read-only inventory input, not a ReadOS dependency.
- Implement a Windows-compatible runtime-neutral MSP core in Rust under `native/msp-core`; the local Windows source package is available for capability inventory, but missing upstream parity/release tooling remains blocked and does not become a ReadOS build dependency.
- Connect the Rust core to ReadOS through a stable .NET adapter while retaining document/PDF/chat/workflow behavior in app/domain code.
- Run applicable upstream conformance cases plus ReadOS compatibility tests, documenting deliberate Windows/platform deviations.
- Exclude the raw local `MSP/` repository from ReadOS packages and maintain Apache-2.0 NOTICE/provenance for copied or derived material.

Current verified slice: backend-neutral paths, handle-confined read-only NTFS WorkspaceFS, Rust `ls`/binary `cat`, chunk-safe host-path sanitization, panic-contained internal ABI, stable Hosting adapter, static CRT, native binary/content checks, bounded product execution through Rust, the negotiated length-delimited ABI v2, and validated deterministic Rust command registry/pack composition all pass the full verifier. The product host proxies only canonical lowercase `pwd`/`echo`; managed `MspRuntime` still owns the surrounding lifecycle and audit, and all other commands retain their prior managed ownership.

Completed adoption gate, `native_pwd_echo_adoption_v1`: native Parse validates the original command text as one pipeline and one command with no operator, redirection, assignment, negation, newline, or unquoted ampersand; managed/native argument disagreement, missing DLL, native audit drift, state changes, and unrepresentable binary output fail closed. Native audit is used only as execution evidence and is not appended to the managed result. Package smoke proves `pwd`, `echo ''`, and `echo -n reados-native-proxy` through the real host proxy with one managed audit each.

Completed boundary gate, `native_abi_v2_handshake_v1`: the DLL now has seven verified exports and a fixed 32-byte ABI-info layout reporting major `2`, minor `0`, contract `0x324D534F44414552`, and capabilities `0x3F` (required `0xF`). V2 invoke/free use explicit pointer/`ulong` length values and preserve embedded NUL bytes. Parse/Execute/Normalize request caps are 128 KiB/1 MiB/1 MiB and response caps are 16 MiB/64 MiB/1 MiB. A bounded writer stops before reserve/copy, closing the measured 54.5x Parse amplification path. Hosting falls back to v1 only when all three v2 exports are absent; partial exports, handshake drift, or missing capabilities fail closed. V1/v2 allocators never mix, every native-return path frees exactly once, invoke/dispose share one lock, and runtime ABI information is exposed for package evidence. The full verifier, 3/3 real-DLL operations, staged v2/v1 FFI smoke, and the no-skip package gate pass.

Completed runtime gate, `native_command_core_registry_v1`: small Rust `Command`, `Invocation`, `Context`, `Registry`, and `CommandPack` contracts now replace the hard-coded command list and dispatch. Registration validation, duplicate rejection, unknown lookup, deterministic ordering, pack composition, and registry-derived `help` are covered. The 12-case baseline/candidate ABI v1/v2 differential passes, preserving command bytes, exit codes, diagnostics, audit evidence, state changes, fixtures, ABI behavior, and product routing without adding commands or capabilities.

Completed adoption gate, `native_mixed_workspace_read_v1`: capability-based read-only WorkspaceFS traits, deterministic longest-prefix mount routing/rebasing, and a lifetime-safe reverse-P/Invoke callback bridge to the app-owned virtual workspace are implemented. ABI v2 operation 4 (`WORKSPACE_INVOKE`) carries the opaque `MspNativeWorkspaceHostV1` table and mount topology into Rust; `CallbackReadOnlyWorkspace` serves `ReadOnlyWorkspaceFileSystem` through invoke/free/is_cancelled callbacks with strict bounds, panic containment, cancellation, free-exactly-once ownership, and host-path disclosure rejection; `workspace_invoke.rs` composes a `CompositeReadOnlyWorkspace` from a callback base plus mounts and serializes bounded JSON results. Rust unit tests (181 total), managed adapter tests, and a real release-DLL differential read test prove direct, virtual, and mixed reads; product `ls`/`cat` are not routed through Rust. The verified fixed-local-NTFS backend, VirtualPath, path sanitization, ABI v2 negotiation/ownership, and managed lifecycle/audit authority are preserved. The `native_stream_core_v1` (bounded byte streams), `mutable_workspace_write_v1` (TOCTOU-safe writes), `recoverable_workspace_trash_v1` (hidden `.msp/trash`), pipelines/redirection, model-facing exec sessions, the Windows ConPTY/Job Object process backend + UTF-16LE sanitizer, and the ConPTY session integration slices are also complete; the next work is broader product adoption and wider conformance. Synchronous native invocation still cannot preempt work already in flight, so current cancellation checks remain bounded around side-effect-free `pwd`/`echo` calls.

T95 compatibility gates:

| Stage | Compatibility focus | Acceptance gate |
| --- | --- | --- |
| 0. Reference inventory | `MSP/Spec`, `MSP/Conformance`, the Windows source-package workspace, license, provenance, and release-tool availability | The versioned capability manifest maps the 26 workspace members and selected capabilities; package exclusion and NOTICE/provenance rules are reviewable; missing inventory/release runners remain explicitly `blocked`. |
| 1. Rust core semantics | WorkspaceFS paths, commands/results/streams, policy, audit, diagnostics | Rust tests pass the selected MSPCore conformance cases without host-path leakage and document every intentional deviation. |
| 2. Shell and command profile | MSPShell parsing/execution plus an explicit MSPPOSIXCore command subset on Windows | Per-feature/command matrix records `conformant`, `partial`, `deferred`, or `not applicable`; applicable upstream fixtures pass in CI. |
| 3. Stable .NET adapter | Request, streaming, cancellation, result, policy/audit, workspace, and artifact interop | Managed adapter contract tests and C#-vs-Rust differential tests pass; ReadOS business services depend only on the adapter, never raw FFI. |
| 4. Product/release adoption | Existing ReadOS workflows over the Rust core | T92/T93 safety and workflow tests pass through the adapter; package contains only required native binaries/notices and excludes the raw `MSP/` tree. |

## Immediate Backlog

The authoritative item-level backlog is [docs/DEVELOPMENT_TRACKER.md](docs/DEVELOPMENT_TRACKER.md). T1-T91 are retained there as completed execution history; they are no longer presented here as the current backlog.

1. **T93 — Product-Level End-to-End Acceptance (in progress):** deterministic restart/lineage/denial plus cancellation, invalid-page, secret-safe provider failure, restart, and retry are implemented; visible WinUI, real provider/network, and the flagship workflow across a packaged-process restart remain.
2. **T94 — Workbench Hardening (in progress):** compact flyout access and width restoration are implemented; Runtime Drawer resizing/persistence/selection/details, latest-request-wins cancellation, deprecated presenter cleanup, and feature-driven ViewModel/service splits remain.
3. **T95 — Upstream-Aligned Windows MSP Core (in progress):** the read-only handle-based NTFS core, sanitizer, stable Hosting adapter, static CRT, bounded canonical-lowercase `pwd`/`echo` product adoption, length-delimited ABI v2 handshake/release gate, zero-behavior-change Rust command registry, the `native_mixed_workspace_read_v1` callback bridge, the `native_stream_core_v1` bounded byte-stream core, the mutable WorkspaceFS/trash slice, pipelines/redirection execution, model-facing exec sessions, the Windows ConPTY/Job Object process backend + UTF-16LE sanitizer, and ConPTY session integration are complete. Next add broader product adoption and wider conformance without moving ReadOS PDF/chat/workflow behavior into Rust.

Recently closed: **T92 — Trustworthy MSP Boundary**, covering namespace confinement, terminal audit consistency, fail-fast verification/package/CI, pinned SDK, and DPAPI provider credentials. See the tracker for its exact acceptance and verification evidence.

## Verification

Managed tests (verified 2026-08-12: 69 + 318 + 411 = 798):

```powershell
dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj
dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj
dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj
```

Full MSP verification:

```powershell
.\scripts\verify-msp.ps1
```

The verifier fails immediately on Rust fmt/test/clippy/release build, native binary/FFI smoke, restore, any of the three managed test projects (including real release-DLL adapter tests), or solution build. `.github/workflows/windows-ci.yml` runs the same path with the SDK selected from `global.json`.

Release package:

```powershell
.\scripts\package-windows.ps1 -StopExisting
```

The package script restores from a clean state, runs all three managed test projects unless explicitly skipped, propagates restore/test/publish failures, verifies the static-CRT native binary and package contents, runs staged ABI v2/v1 FFI, and runs the staged executable through isolated app state plus a separate fixed-NTFS native WorkspaceFS smoke unless `-SkipSmoke` is explicitly supplied. The latest completed no-skip gate is `0.1.0-native-command-registry-verified`, producing `artifacts/releases/ReadOS-0.1.0-native-command-registry-verified-win-x64.zip`. Staged FFI and package smoke pass with `LengthDelimitedV2` 2.0 runtime evidence, 532 ZIP entries, `RawMSP`/PDB/`.git` counts of zero, five `nativeCommands` all exiting 0, three proxy audit counts of 1, cleanup, redaction, and required license/NOTICE/provenance. The packaged DLL retains SHA256 `2CFD14246FA963AC284B158903ADC910A782AFEDFEA4EC5F247692BF4613E49A`.

Workbench build/run:

```powershell
.\scripts\run.ps1 -BuildOnly
.\scripts\run.ps1
```
