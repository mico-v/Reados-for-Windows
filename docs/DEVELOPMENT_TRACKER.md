# ReadOS Development Tracker

This tracker turns the current ReadOS plan into executable work. Keep it updated whenever a development item starts, changes scope, passes verification, or is deferred.

## Tracking Rules

- Update this file in the same change as the code or design decision it tracks.
- Keep statuses concrete: `planned`, `in progress`, `done`, or `blocked`.
- Every `done` item must list the verification command or review evidence that closed it.
- Prefer vertical slices that improve the MSP service model end to end: command, policy, audit, artifact, provenance, UI surface, and tests where relevant.
- Keep `ReadOS.Msp.Hosting` limited to host-neutral contracts and services; document/PDF/chat adapters and WinUI projection stay in `ReadOS.App`.

## Current Baseline

Last verified: 2026-08-12.

- `.\scripts\verify-msp.ps1` passed.
- Rust native MSP tests passed: 132 passed, 0 failed.
- Native MSP binary and FFI smoke verification passed; the release DLL exposes all seven required exports and uses the static MSVC CRT.
- The registry candidate release DLL SHA256 is `2CFD14246FA963AC284B158903ADC910A782AFEDFEA4EC5F247692BF4613E49A`.
- `ReadOS.Msp.Tests` passed: 69 passed, 0 failed.
- `ReadOS.Msp.Hosting.Tests` passed: 318 passed, 0 failed, including the real release-DLL adapter operations with no skip (the native-DLL tests skip only when run standalone without `READOS_MSP_NATIVE_DLL`).
- `ReadOS.App.Tests` passed: 411 passed, 0 failed.
- Managed total: 798 passed, 0 failed (69 + 318 + 411 under the full verifier).
- `ReadOS.sln` built with 0 warnings and 0 errors.
- Verification and packaging scripts propagate external-command failures; Windows CI runs the verifier with the SDK pinned by `global.json`.
- Provider credentials are protected with Windows DPAPI `CurrentUser` outside workspace JSON/exports, with legacy plaintext migration coverage.
- The latest completed no-skip package gate is `package-windows.ps1 -StopExisting -Version 0.1.0-native-command-registry-verified`, which produced `artifacts/releases/ReadOS-0.1.0-native-command-registry-verified-win-x64.zip`. Staged FFI and packaged-process smoke pass with `LengthDelimitedV2` 2.0, 532 ZIP entries, `RawMSP`/PDB/`.git` counts of zero, five `nativeCommands` all exiting 0, an audit count of 1 for each of the three product proxies, cleanup, content, license/provenance, and redaction checks.

## Current Roadmap

- **T92 — Trustworthy MSP Boundary:** done. Namespace confinement, terminal audit evidence, fail-fast verification/package/CI, SDK pinning, and DPAPI provider credentials are verified below.
- **T93 — Product-Level End-to-End Acceptance:** in progress. The deterministic restart/lineage/denial path plus cancellation, invalid-page, provider-failure, secret-safety, restart, and retry branches are covered. Visible UI is covered by the operator runbook and a WinUI smoke; a real provider/network boundary and packaged-process flagship restart remain open.
- **T94 — Workbench Hardening:** done. Compact flyout access and width memory, async latest-request-wins generation/cancellation, deprecated `PresenterSurfaceView` removal, and the re-homed Runtime Drawer (resizable height, pin, persisted layout, simplified MSP transcript) are complete; verified by `dotnet build ReadOS.App -c Debug` (0 warnings/errors) and `ReadOS.App.Tests` (`Run_drawer_layout_persists_height_and_pin`).
- **T95 — Upstream-Aligned Windows MSP Core:** in progress. The handle-based read-only NTFS WorkspaceFS, native adapter, static CRT, direct native `ls`/`cat`, bounded canonical-lowercase `pwd`/`echo` adoption, ABI v2 handshake/release gate, zero-behavior-change Rust command registry, and the `native_mixed_workspace_read_v1`, `native_stream_core_v1`, `mutable_workspace_write_v1`, and `recoverable_workspace_trash_v1` slices are verified. `native_mixed_workspace_read_v1` adds ABI v2 operation 4 (`WORKSPACE_INVOKE`): a lifetime-safe reverse-P/Invoke callback bridge that lets a managed `IMspNativeReadOnlyWorkspace` serve the Rust `CompositeReadOnlyWorkspace` (capabilities `0x1F`, required `0xF`); direct, virtual, and mixed reads are proven by Rust unit tests, managed adapter tests, and a real release-DLL differential test, without migrating product `ls`/`cat`. `native_stream_core_v1` adds bounded byte-stream primitives (`MspDataReader`, `BoundedBytePipe`, `MspWorkspaceFileReader`) mirroring upstream `MSPCommandStream.swift`. `mutable_workspace_write_v1` adds a TOCTOU-safe `WritableWorkspaceFileSystem` on the fixed-NTFS backend (create/write/rename/delete with handle-verified containment). `recoverable_workspace_trash_v1` adds a same-volume hidden `.msp/trash` with recoverable moves, sidecar metadata, restore, and host-authorized emptying. The next implementation slice is pipelines/redirection, followed by exec sessions, broader product adoption, ConPTY, and wider conformance.

Do not mark T93-T95 done from component-level tests alone. Each item has its own acceptance evidence below.

## Completed Execution History (T1-T91)

The detailed entries below preserve the implementation trail that produced the current Hosting, workflow, timeline, artifact, and workbench baseline. They are historical closure records, not the active priority list.

### T1: Named Workflow Orchestration

Status: done.

Goal: add the first named workflow command so ReadOS moves beyond summarizing existing sessions and can execute an inspectable document workflow through MSP.

Initial command:

```text
workflow run summarize-current --artifact /artifacts/workflows/current.md
```

Acceptance criteria:

- `workflow run summarize-current` is parsed as a named workflow, not as a hidden UI action.
- The command reads the current MSP session projection and transcript projection through the virtual workspace.
- With `--artifact`, the command creates a Markdown artifact under `/artifacts`.
- The artifact records source paths for the session and transcripts that shaped the workflow output.
- Mutating artifact output is approval-gated by existing MSP policy.
- Tests cover success, artifact provenance, invalid workflow names, and invalid artifact paths.

Verification:

- 2026-07-07: `.\scripts\verify-msp.ps1` passed.
- Added MSP tests for named workflow artifact output, invalid workflow names, invalid artifact paths, and policy confirmation.

### T2: Multi-Step Workflow Provenance

Status: done.

Goal: extend workflow outputs so artifacts produced by named workflows include automatic source document, page, session, transcript, and command provenance.

Depends on: T1.

Result: `workflow run summarize-current --artifact <path>` reads upstream artifact manifests referenced by the current session, inherits their source document/page references, and records the session, transcript, upstream artifact, manifest, and source page paths on the generated workflow artifact.

Verification:

- 2026-07-07: `.\scripts\verify-msp.ps1` passed.
- Added MSP test coverage for inherited document/page provenance from upstream artifact manifests.

### T3: Command-Specific Recovery Previews

Status: done.

Goal: add richer diagnostics and previews for document metadata failures, unresolved PDF targets, and model-provider failures so operators and agents receive actionable recovery paths.

Progress:

- Unresolved PDF/document targets now return `reados.pdf.document_not_found` with the selector as `target` and a recovery hint that points to `library list` and selected-PDF state.
- The diagnostic is preserved on the command result and audit record.
- `chat ask` model-provider failures now return `reados.chat.model_provider_failed` with provider/model target details and a recovery hint for base URL, API key, model name, network access, or offline mode.
- Failed `chat ask` model calls preserve queued evidence and avoid partial conversation or artifact writes.
- Document metadata failures now include `reados.pdf.invalid_page` for invalid/out-of-range PDF pages and `reados.pdf.outline_item_not_found` for missing outline selectors.

Verification:

- 2026-07-07: `.\scripts\verify-msp.ps1` passed for unresolved PDF/document target diagnostics.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed for model-provider failure diagnostics.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed for document metadata diagnostics.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T3.

### T4: Typed MSP Timeline

Status: done.

Goal: replace generic transcript/chat rendering with typed timeline records for MSP request, approval, running command, command result, artifact, evidence, and error states.

Progress:

- The existing `TimelineItems` projection now exposes `KindLabel` for visible record-type chips.
- MSP timeline records now project distinct titles/glyphs for approval required, running, canceled, completed, and failed states.
- `ChatTimelineView` renders a compact record-type chip in each timeline item header.
- `ChatTimelineView` now uses separate body sections for message text, evidence, MSP approval, running command, completed/canceled command result, failed command diagnostics, and artifacts.
- `ThreadTimelineItem` exposes explicit binding states for evidence body, message body, MSP approval, running, result, and error panels.
- Timeline action buttons now open artifacts in the Artifacts inspector, evidence records in the Evidence inspector, MSP results in the Run inspector, and failed MSP diagnostics in the Policy inspector with the matching transcript selected.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed for typed MSP timeline projection.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after the typed timeline projection update.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed after splitting timeline body sections.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after splitting timeline body sections.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed after adding timeline inspector actions.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding timeline inspector actions.

Result: the thread timeline now renders typed message, evidence, MSP approval, running command, result, failure, and artifact records, with actions that route each record into the matching inspector context.

### T5: Timeline Inspector Polish

Status: done.

Goal: tighten the interaction between timeline records and the inspector so keyboard focus, selected transcript visibility, and diagnostic review feel deliberate instead of incidental.

Progress:

- Run and Policy inspector tabs now show a selected transcript detail card when a timeline action selects an MSP record.
- The detail cards expose command text, status, decision, output preview, effects, diagnostics, and recovery hints without relying only on ListView selection styling.
- `InspectorViewModel` exposes `HasSelectedMspTranscriptEntry` so the selected state is directly testable and bindable.
- MSP transcript entries now expose a non-persisted `IsInspectorSelected` UI state so Run and Policy lists can show a visible accent rail and selected background.
- Run and Policy transcript lists explicitly use single selection and remain tab-focusable for keyboard review.
- Approving or denying an MSP approval keeps the resulting transcript entry selected in the inspector instead of leaving the removed approval selected.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed after adding selected transcript details.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding selected transcript details.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 30 tests after selection-marker and keyboard-focus polish.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T5.

Result: timeline actions, Run inspector selection, and Policy diagnostic review now share a visible selected transcript state with durable detail cards and passing coverage.

### T6: MSP Session Visibility

Status: done.

Goal: make MSP session state visible from the left context list so pending approvals, running commands, failures, and artifacts are discoverable before the operator opens the Run or Policy inspector.

Acceptance criteria:

- Session rows expose a compact status label for running, pending approval, needs review, and completed states.
- The MSP sessions rail/header surfaces pending approval state.
- Selecting an MSP session selects the latest relevant transcript entry and opens Run or Policy based on its state.
- Tests cover the session projection and inspector routing.

Progress:

- `MspSessionEntry` exposes non-persisted derived UI state for running commands, pending approvals, failures, status labels, status glyphs, artifact count, and summarized counts.
- MSP session rebuilding fills running and pending-approval counts from transcripts.
- Selecting an MSP session selects the latest matching transcript and opens Policy for approvals/failures or Run for normal/running records.
- The MSP sessions rail shows a pending-approval indicator, session rows show status glyphs/chips, and the sidebar footer surfaces pending approvals.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 31 tests after session visibility work.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T6.

Result: pending approvals and failed sessions are discoverable from the left context list and route directly into the matching inspector review state.

### T7: Composer Evidence Chips

Status: done.

Goal: make queued evidence in the composer inspectable and editable before sending so operators can remove individual pages, ranges, regions, or artifacts without clearing the whole evidence queue.

Acceptance criteria:

- The composer shows individual pending evidence chips with title, detail, and a remove affordance.
- Removing one pending attachment updates the queue summary and send snapshot without affecting the remaining attachments.
- The same state remains available through `ThreadViewModel` for binding and tests.
- Tests cover single-attachment removal and attachment summary updates.

Progress:

- Added `RemovePendingAttachmentCommand` so queued evidence can be removed one item at a time by attachment ID.
- Exposed the command through `ThreadViewModel` for composer binding.
- The composer now renders each pending attachment as a compact chip with title, detail, and an icon remove action while preserving the existing summary and clear-all action.
- Sending a prompt uses the remaining pending attachment snapshot after any removals.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 32 tests after composer evidence chip work.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T7.

Result: queued evidence is visible and editable directly in the composer before the operator sends the prompt.

### T8: Composer Runtime Primary Action

Status: done.

Goal: make the composer primary action reflect the current runtime state so operators do not accidentally send a prompt when the UI is signaling runtime control.

Acceptance criteria:

- When no MSP command is running, the composer primary action sends the prompt.
- When an MSP command is running, the composer primary action becomes a stop action wired to active MSP cancellation.
- The label, glyph, and tooltip update from the same runtime state.
- Tests cover command routing for idle and running states.

Progress:

- Added a computed `ComposerPrimaryCommand` that routes to prompt sending when idle and active MSP cancellation while a command is running.
- Composer primary label, glyph, and tooltip now derive from the same runtime state.
- `ThreadViewModel` exposes the primary command and visual state for XAML binding.
- `MspTranscript` collection changes now refresh runtime/composer state, so direct transcript updates do not leave stale primary action state.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 33 tests after composer runtime primary action work.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T8.

Result: the composer no longer shows runtime-control copy while still executing the send command; the primary action now matches runtime state.

### T9: Pending Approval Navigation

Status: done.

Goal: make pending approvals reachable from global and composer runtime surfaces without forcing the operator to hunt through the Run drawer or session list.

Acceptance criteria:

- The global pending-approval indicator opens the Policy inspector and selects a pending transcript entry.
- The composer pending-approval chip uses the same navigation path.
- The command opens the runtime drawer for approval context and no-ops safely when there are no pending approvals.
- Tests cover pending approval routing.

Progress:

- Added `OpenPendingApprovalCommand` to select the first pending approval transcript, open the Policy inspector, and show the runtime drawer.
- Exposed the command through `ThreadViewModel` for composer binding.
- The global pending-approval indicator and composer pending-approval chip are now actionable buttons that use the same approval navigation path.
- The command no-ops safely with a status message when no pending approvals exist.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 34 tests after pending approval navigation work.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T9.

Result: pending approvals are reachable from the global toolbar and composer and route directly into the selected Policy inspector context.

### T10: Composer Queue State

Status: done.

Goal: make the composer support explicit queue state while MSP work is running, so operators can capture follow-up prompts and evidence without interrupting the active command.

Acceptance criteria:

- While MSP work is running, the composer exposes a queue action separate from the stop primary action.
- Queueing captures the current draft and pending attachments, clears the composer, and shows a queued item chip.
- Restoring a queued item repopulates the draft and pending attachments without sending automatically.
- Tests cover queue and restore behavior.

Progress:

- Added a non-persisted `QueuedComposerPrompt` model for queued draft text plus cloned evidence attachments.
- Added composer queue state to `ShellViewModel` and `ThreadViewModel`, including queue summary, visibility state, queue command, and restore command.
- While MSP work is running, the composer now shows a separate `队列` action next to the runtime drawer control.
- Queued items render as restoreable chips in the composer context row.
- Queueing clears the current composer draft and pending attachments; restoring repopulates both without sending automatically.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 35 tests after composer queue state work.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T10.

Result: operators can capture follow-up prompts and evidence while MSP work is running, then restore them to the composer when ready.

### T11: Failure Review Workflow

Status: done.

Goal: add a second named workflow that produces an inspectable recovery report from failed MSP transcript entries.

Initial command:

```text
workflow run review-failures --artifact /artifacts/workflows/failures.md
```

Acceptance criteria:

- `workflow run review-failures` is recognized as a named workflow.
- The workflow reads the current session and transcript projections through the virtual workspace.
- The output focuses on failed commands, diagnostics, recovery hints, and referenced artifacts instead of repeating the general workflow summary.
- With `--artifact`, the workflow writes a Markdown artifact under `/artifacts`.
- The artifact preserves session/transcript source provenance and inherited artifact document/page provenance.
- Tests cover success and artifact output.

Progress:

- Added `workflow run review-failures` as a second named workflow.
- The workflow reads the current session and transcript projections through the virtual workspace and builds a focused Markdown recovery queue.
- Failure review output includes failed commands, exit codes, decisions, diagnostics, recovery hints, and referenced artifacts.
- With `--artifact`, the workflow writes a provenance-backed Markdown artifact under `/artifacts`.
- Named workflow artifact output continues to inherit upstream artifact source document/page provenance from sidecar manifests.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 19 tests after adding failure review workflow coverage.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T11.

Result: ReadOS now has two inspectable named workflows: one for general session summaries and one for focused failure recovery review.

### T12: Document Section Explanation Workflow

Status: done.

Goal: add the first document-centered named workflow so MSP can resolve a PDF outline section, extract its page evidence, call the configured chat model, and persist an inspectable Markdown artifact with document/page provenance.

Initial command:

```text
workflow run explain-section --document current --outline "3.2" --artifact /artifacts/workflows/explain-section.md
```

Acceptance criteria:

- `workflow run explain-section` is recognized in the ReadOS app command pack while existing session workflows continue to use the core workflow implementation.
- The workflow resolves `--document` and `--outline` against the current ReadOS workspace.
- The section page range is derived from the selected outline item and the next peer/higher-level outline item.
- The workflow extracts source page text, calls the configured chat model, and writes a Markdown artifact when `--artifact` is supplied.
- The artifact records source document ID, source page range, and `/documents/{id}/pages/{n}.txt` source paths.
- Missing outline selectors and invalid artifact paths return stable recovery diagnostics.
- App-level MSP host tests cover approval, artifact provenance, model input, and recovery diagnostics.

Progress:

- Added a ReadOS app-side workflow command wrapper that delegates existing core session workflows back to `WorkflowCommand`.
- Added `workflow run explain-section --document current --outline <id|title> [--artifact <path>]`.
- The workflow resolves outline sections, derives the page range from the next peer or higher-level outline item, extracts section text, and passes the section as temporary model evidence.
- With `--artifact`, the workflow writes a Markdown explanation artifact with document ID, page range, virtual page source paths, source command, actor, and session provenance.
- Missing outline selectors return `reados.pdf.outline_item_not_found`; invalid artifact targets return `reados.workflow.invalid_artifact_path`.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 37 tests after adding section workflow coverage.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 19 tests after verifying core workflow fallback behavior.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T12.

Result: ReadOS now has its first document-centered named workflow, implemented in the app domain layer while preserving the core MSP session workflow implementation for runtime-neutral behavior.

### T13: Structured Evidence Extraction Workflow

Status: done.

Goal: add a second document-centered named workflow that extracts outline-scoped PDF page evidence into a structured JSON artifact for downstream review, citation, and future workflow composition.

Initial command:

```text
workflow run extract-evidence --document current --outline "3.2" --artifact /artifacts/workflows/evidence.json
```

Acceptance criteria:

- `workflow run extract-evidence` is recognized in the ReadOS app command pack while existing session workflows continue to use the core workflow implementation.
- The workflow resolves `--document` and `--outline` against the current ReadOS workspace.
- The section page range is derived from the selected outline item and the next peer/higher-level outline item.
- The workflow extracts each source page as a separate structured evidence record.
- With `--artifact`, the workflow writes `application/json` under `/artifacts`.
- The artifact records source document ID, source page range, and `/documents/{id}/pages/{n}.txt` source paths.
- Invalid artifact paths return a stable recovery diagnostic before page extraction begins.
- App-level MSP host tests cover approval, structured JSON output, artifact provenance, and invalid-path recovery.

Progress:

- Added `workflow run extract-evidence --document current --outline <id|title> [--artifact <path>]` to the ReadOS app-side workflow command wrapper.
- The workflow delegates non-document workflow forms back to the core `WorkflowCommand`.
- The workflow resolves the outline section, derives its page range, extracts each page separately, and emits one JSON evidence record per source page.
- With `--artifact`, the workflow writes an `application/json` artifact with source command, actor, session ID, document ID, source page range, and virtual page source paths.
- Invalid artifact targets return `reados.workflow.invalid_artifact_path` before any PDF page extraction starts.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 39 tests after adding structured evidence workflow coverage.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 19 tests after verifying core workflow fallback behavior.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T13.

Result: ReadOS now has two document-centered app-side named workflows: `explain-section` for model-generated section explanations and `extract-evidence` for structured JSON page evidence artifacts.

### T14: Evidence Review Workflow

Status: done.

Goal: compose the structured evidence artifact from T13 into an inspectable Markdown review artifact with citation notes and inherited source provenance.

Initial command:

```text
workflow run review-evidence --evidence /artifacts/workflows/evidence.json --artifact /artifacts/workflows/evidence-review.md
```

Acceptance criteria:

- `workflow run review-evidence` is recognized in the ReadOS app command pack while existing session workflows continue to use the core workflow implementation.
- The workflow reads a structured evidence JSON artifact produced by `workflow run extract-evidence`.
- The workflow generates Markdown review notes and a citation table without re-extracting PDF pages.
- With `--artifact`, the workflow writes `text/markdown` under `/artifacts`.
- The review artifact records the evidence artifact path, evidence manifest path, inherited source document ID, inherited source page range, and inherited `/documents/{id}/pages/{n}.txt` source paths.
- Missing or invalid evidence artifacts return stable recovery diagnostics.
- App-level MSP host tests cover approval, review output, provenance inheritance, no PDF/model calls, and missing-evidence recovery.

Progress:

- Added `workflow run review-evidence --evidence <artifact> [--artifact <path>]` to the ReadOS app-side workflow command wrapper.
- The workflow reads structured JSON produced by `workflow run extract-evidence`.
- The workflow generates Markdown review notes and a citation table from evidence page records without re-extracting PDF pages or calling the model.
- With `--artifact`, the workflow writes a `text/markdown` artifact that includes the evidence artifact path, evidence manifest path, inherited source document/page fields, and inherited virtual page source paths.
- Missing evidence artifacts return `reados.workflow.evidence_artifact_not_found`; invalid evidence paths/artifacts return stable workflow diagnostics.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 41 tests after adding evidence review workflow coverage.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 19 tests after verifying core workflow fallback behavior.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T14.

Result: ReadOS can now compose document workflow artifacts: `extract-evidence` produces structured page evidence, and `review-evidence` turns that artifact into review notes with citation provenance.

### T15: Evidence Synthesis Workflow

Status: done.

Goal: add an evidence-to-model workflow that reads structured evidence artifacts, calls the configured chat model, and persists a source-grounded synthesis artifact with inherited citation provenance.

Initial command:

```text
workflow run synthesize-evidence --evidence /artifacts/workflows/evidence.json --artifact /artifacts/workflows/evidence-synthesis.md
```

Acceptance criteria:

- `workflow run synthesize-evidence` is recognized in the ReadOS app command pack while existing session workflows continue to use the core workflow implementation.
- The workflow reads a structured evidence JSON artifact produced by `workflow run extract-evidence`.
- The workflow calls the configured chat model with the evidence artifact as model context.
- With `--artifact`, the workflow writes `text/markdown` under `/artifacts`.
- The synthesis artifact records the evidence artifact path, evidence manifest path, inherited source document ID, inherited source page range, and inherited `/documents/{id}/pages/{n}.txt` source paths.
- Model-provider failures return stable `reados.chat.model_provider_failed` diagnostics without writing a partial synthesis artifact.
- App-level MSP host tests cover approval, model input, artifact provenance, no PDF extraction, and provider-failure recovery.

Progress:

- Added `workflow run synthesize-evidence --evidence <artifact> [--artifact <path>]` to the ReadOS app-side workflow command wrapper.
- The workflow reads structured JSON produced by `workflow run extract-evidence`.
- The workflow passes the evidence artifact as model context through the configured chat model service.
- With `--artifact`, the workflow writes a `text/markdown` synthesis artifact with the evidence artifact path, evidence manifest path, inherited source document/page fields, and inherited virtual page source paths.
- Model-provider failures reuse `reados.chat.model_provider_failed` and do not write partial synthesis artifacts.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 43 tests after adding evidence synthesis workflow coverage.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 19 tests after verifying core workflow fallback behavior.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T15.

Result: ReadOS now supports a full evidence workflow chain: extract structured page evidence, review it without model calls, and synthesize it through the configured model while preserving citation provenance.

### T16: Artifact Refinement Workflow

Status: done.

Goal: add a steer/resume workflow that lets an operator refine an existing evidence, review, or synthesis artifact without rebuilding the source extraction.

Initial command:

```text
workflow run refine-artifact --source /artifacts/workflows/evidence-synthesis.md --instruction "tighten caveats and keep citations" --artifact /artifacts/workflows/evidence-synthesis-refined.md
```

Acceptance criteria:

- `workflow run refine-artifact` is recognized in the ReadOS app command pack while existing session workflows continue to use the core workflow implementation.
- The workflow reads an existing source artifact under `/artifacts`.
- The workflow calls the configured chat model with the source artifact content and operator instruction.
- With `--artifact`, the workflow writes `text/markdown` under `/artifacts`.
- The refined artifact records the source artifact path, source manifest path, inherited source document IDs, inherited source page ranges, and inherited `/documents/{id}/pages/{n}.txt` source paths.
- Model-provider failures return stable `reados.chat.model_provider_failed` diagnostics without writing a partial refined artifact.
- App-level MSP host tests cover approval, model input, artifact provenance, no PDF extraction, and provider-failure recovery.

Progress:

- Added `workflow run refine-artifact --source <artifact> --instruction <text> [--artifact <path>]` to the ReadOS app-side workflow command wrapper.
- The workflow reads any existing source artifact under `/artifacts` and its sidecar manifest.
- The workflow passes source artifact content plus the operator instruction through the configured chat model service.
- With `--artifact`, the workflow writes a `text/markdown` derivative artifact with the source artifact path, source manifest path, inherited source document/page fields, and inherited virtual page source paths.
- Model-provider failures reuse `reados.chat.model_provider_failed` and do not write partial refined artifacts.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 45 tests after adding artifact refinement workflow coverage.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 19 tests after verifying core workflow fallback behavior.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T16.

Result: ReadOS now supports steer/resume over generated artifacts: operators can refine evidence reviews or syntheses without rebuilding the original page extraction, while preserving citation provenance.

### T17: Artifact Lineage Inspector

Status: done.

Goal: make derived workflow artifacts inspectable from the UI by surfacing their source artifacts, sidecar manifests, virtual document pages, source documents, and page ranges in the Artifacts inspector.

Acceptance criteria:

- Selecting an artifact projects `SourcePaths`, `SourceDocuments`, and `SourcePages` into a compact lineage list.
- Source artifact paths under `/artifacts` are marked as openable when the matching workspace artifact exists.
- Manifest paths, virtual page paths, source document IDs, and page ranges are labeled distinctly.
- Opening a lineage source artifact selects that artifact and keeps the inspector on the Artifacts tab.
- ViewModel tests cover lineage projection and source artifact navigation.

Progress:

- Added an `ArtifactLineageItem` UI model for source path, kind, detail, icon, and openability.
- Added selected-artifact lineage state and source-artifact navigation to `ShellViewModel`.
- Exposed lineage state and commands through `InspectorViewModel`.
- Updated the Artifacts inspector to render source lineage between the artifact header and content preview.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 46 tests after adding artifact lineage coverage.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 19 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T17.

Result: selected workflow artifacts now expose their provenance chain in the Artifacts inspector, including source artifacts, manifests, virtual pages, source documents, and page ranges, and operators can open matching source artifacts directly from the lineage list.

### T18: Guided Workflow Command Actions

Status: done.

Goal: let operators compose exact workflow commands from the current Inspector selection without manually copying outline IDs or artifact paths.

Acceptance criteria:

- A selected PDF outline item can prepare `workflow run explain-section` with a deterministic Markdown artifact path.
- A selected PDF outline item can prepare `workflow run extract-evidence` with a deterministic JSON artifact path.
- A selected JSON evidence artifact can prepare `workflow run review-evidence` and `workflow run synthesize-evidence`.
- Any selected artifact can prepare `workflow run refine-artifact` with a default refinement instruction and derived artifact path.
- Prepared commands populate the MSP command draft, open the Run inspector, and do not execute until the operator runs the command.
- ViewModel tests cover generated command strings and inspector routing.

Progress:

- Added guided workflow command state for selected outline targets, selected artifacts, and JSON evidence artifacts.
- Added prepare commands for explain-section, extract-evidence, review-evidence, synthesize-evidence, and refine-artifact.
- Added deterministic artifact path generation under `/artifacts/workflows`.
- Added Evidence and Artifacts inspector buttons that fill the MSP command draft and route to the Run inspector.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 48 tests after adding guided workflow action coverage.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 19 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T18.

Result: the Inspector can now turn selected outline sections and artifacts into reviewable MSP workflow command drafts, preserving operator approval and execution control.

### T19: Artifact Reuse Actions

Status: done.

Goal: make generated artifacts directly reusable from the Artifacts inspector without requiring operators to search virtual paths or copy text manually.

Acceptance criteria:

- A selected artifact can be opened in the Preview inspector as Markdown/text content.
- A selected artifact can be copied through a clipboard abstraction.
- A selected artifact can be exported to a local file with a suggested file name and extension.
- The existing attach action remains available next to open/copy/export.
- Tests cover preview routing, copied content, export file output, and suggested export metadata.

Progress:

- Added `IClipboardService` and a WinUI clipboard implementation for artifact content copy.
- Extended `IFileDialogService` with a single-artifact export picker.
- Added selected-artifact open, copy, and export commands to `ShellViewModel` and exposed them through `InspectorViewModel`.
- Updated the Artifacts inspector header with compact icon actions for preview, copy, export, and attach.
- Added ViewModel coverage for artifact preview, clipboard copy, and export output.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 49 tests after adding artifact reuse actions.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 19 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T19.

Result: artifacts can now be opened, copied, exported, or attached directly from the inspector while preserving the virtual artifact model as the source of truth.

### T20: Failure Review Workflow Preset

Status: done.

Goal: connect failed MSP transcript review in the Policy inspector to the existing `workflow run review-failures` command so operators can create a recovery report without hand-writing the command.

Acceptance criteria:

- The Policy inspector exposes a recovery workflow action only when the selected transcript is failed, not running, approved, or pending approval.
- The action prepares `workflow run review-failures --artifact <path>` with a deterministic artifact path under `/artifacts/workflows`.
- Prepared commands populate the MSP command draft, open the Run inspector, and do not execute until the operator runs the command.
- ViewModel tests cover failed/non-failed selection state, generated command text, and inspector routing.

Progress:

- Added selected failure review state to `ShellViewModel` and `InspectorViewModel`.
- Added `PrepareReviewFailuresWorkflowCommand` that builds a session/transcript-based failure report artifact path.
- Added a Policy inspector action that appears on selected failed diagnostics.
- Added ViewModel coverage for the failure review preset.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 50 tests after adding failure review preset coverage.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 19 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T20.

Result: failed transcript review now has a direct path into the named failure-review workflow while preserving manual command review and normal MSP policy execution.

### T21: Prepared Command History

Status: done.

Goal: keep Inspector-generated workflow command drafts recoverable after operators review or run other commands.

Acceptance criteria:

- Preparing any guided workflow draft adds it to an in-memory command history.
- The newest prepared command appears first, duplicate commands are promoted instead of duplicated, and history is capped to a compact list.
- Restoring a prepared command fills `MspCommandDraft`, opens the Run inspector, and does not execute the command.
- The Run inspector exposes the prepared-command history near the command input.
- ViewModel tests cover recording order, restore behavior, no implicit execution, de-duplication, and cap behavior.

Progress:

- Added `PreparedMspCommand` as a compact UI model for prepared workflow drafts.
- Added `PreparedMspCommands` history state to `ShellViewModel`, with newest-first order, duplicate promotion, and an 8-item cap.
- Added `RestorePreparedMspCommandCommand` and Inspector pass-through state.
- Updated the Run inspector to show prepared drafts with a restore action.
- Added ViewModel coverage for recording, restoring, no implicit execution, duplicate handling, and history cap.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 52 tests after adding prepared command history coverage.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 19 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T21.

Result: guided workflow presets are no longer a single replace-only draft; operators can revisit recent prepared commands while preserving explicit manual execution.

### T22: Composer Approval Mode Controls

Status: done.

Goal: make approval mode a first-class composer/runtime control and ensure the selected mode changes subsequent MSP policy decisions.

Acceptance criteria:

- Composer exposes approval mode as an explicit segmented control, not only a static label.
- Policy inspector exposes the same approval mode controls and current mode detail.
- Modes include policy approval, confirm-all, and allow-workspace.
- Selected mode is stored in workspace settings and restored on initialization.
- `confirm-all` requires approval for non-dry-run commands, including read-only commands.
- `allow-workspace` allows workspace/artifact writes while still requiring approval for delete/external effects.
- ViewModel and host tests cover state forwarding, persistence, and policy behavior.

Progress:

- Added `MspApprovalModeCodes`, `ApprovalModeOption`, and a persisted `WorkspaceSettings.MspApprovalMode`.
- Added approval mode options, selected-mode state, labels, details, selection command, and setting persistence to `ShellViewModel`.
- Wired `ThreadViewModel` and `InspectorViewModel` to the same selected mode state and command.
- Updated `ReadOsMspHost` operator policy so approval tokens still allow one command, then the selected approval mode decides normal authorization.
- Added segmented approval mode controls to the composer context row and Policy inspector.
- Added ViewModel and host tests for mode restoration, child ViewModel forwarding, setting persistence, allow-workspace execution, and confirm-all pending approval.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 56 tests after adding approval mode controls and policy coverage.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 19 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T22.

Result: operators can now switch approval policy from the composer or Policy inspector, and the setting directly affects whether subsequent MSP commands run, pause for approval, or continue under an approval token.

### T23: Composer Artifact Steer Action

Status: done.

Goal: let operators use the composer as a steer/resume surface for selected artifacts instead of relying only on the Artifacts inspector default refinement preset.

Acceptance criteria:

- When a workspace artifact is selected and the composer has text, the composer exposes a steer/refine action.
- The action prepares `workflow run refine-artifact --source <artifact> --instruction <composer text> --artifact <derived path>`.
- The derived artifact path is deterministic and distinct from the inspector default refinement path.
- Preparing the command opens the Run inspector and records the prepared command history.
- The action does not send chat, clear the composer, or execute the MSP command automatically.
- ViewModel tests cover visibility state, command text, Run inspector routing, no implicit execution, preserved composer text, and prepared history.

Progress:

- Added `HasComposerArtifactRefinementTarget` state to `ShellViewModel` and refreshed it when composer text or selected artifact changes.
- Added `PrepareComposerArtifactRefinementWorkflowCommand` that turns the current composer instruction plus selected artifact into a `workflow run refine-artifact` draft.
- Exposed the state and command through `ThreadViewModel`.
- Added a compact composer `精炼` action visible only for selected artifact plus non-empty composer text.
- Added ViewModel coverage for the composer artifact steer path.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 57 tests after adding the composer artifact steer action.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 19 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T23.

Result: selected artifacts can now be steered from the composer into reviewable MSP refinement workflow drafts, preserving the existing manual Run inspector execution boundary.

### T24: Artifact Rename/Delete Policy And Audit

Status: done.

Goal: add destructive artifact operations only after their MSP policy, preview, audit, and workspace persistence behavior is explicit and covered by tests.

Acceptance criteria:

- `artifact rename <source> <target>` is available through MSP and refuses to overwrite an existing target.
- `artifact delete <path>` is available through MSP and removes the artifact plus projected manifest availability.
- Rename preserves source artifact content and provenance, updates path/source command/actor/session/update metadata, and emits the renamed artifact in command results.
- Rename uses audited effects `ReadWorkspace | WriteWorkspace | DeleteWorkspace`.
- Delete uses audited effect `DeleteWorkspace`.
- Both operations expose policy previews with concrete targets before execution.
- Default/effect-based policy requires confirmation before rename/delete executes.
- ReadOS virtual workspace persists delete/rename effects against `WorkspaceState.Artifacts`.
- Tests cover core runtime execution, policy metadata/audit, ReadOS virtual workspace deletion, and ReadOS host approval behavior.

Progress:

- Added `TryDeleteAsync` to the MSP workspace contract with implementations for in-memory and ReadOS virtual workspaces.
- Extended `artifact` command with `rename` and `delete`, command-specific metadata, previews, usage text, and result handling.
- Implemented rename by reading source content/manifest, writing a target artifact with preserved provenance and updated command metadata, then deleting the source.
- Implemented delete by removing the artifact record; sidecar manifests remain projections and disappear with the artifact.
- Added MSP runtime coverage for rename/delete execution and effect-based approval metadata.
- Added ReadOS virtual workspace and host coverage for deletion, approval gating, and approved rename persistence.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests after adding artifact rename/delete command coverage.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 60 tests after adding ReadOS virtual workspace and host coverage.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T24.

Result: artifact rename/delete now exist as audited MSP operations that are visible through policy previews and require the normal approval path before mutating ReadOS workspace artifacts.

### T25: Operator Approval Policy Extraction

Status: done.

Goal: begin separating stable host/session/policy/artifact boundaries by extracting operator approval policy behavior from the ReadOS MSP host without changing runtime behavior.

Acceptance criteria:

- `ReadOsMspHost` no longer owns the approval-mode and one-shot approval-token policy implementation directly.
- The extracted policy keeps the existing approval token environment key and command/actor matching behavior.
- Approval tokens allow only the exact approved command once, then normal policy decisions resume.
- `confirm-all` still requires confirmation for non-dry-run reads.
- `allow-workspace` still allows workspace writes and artifact creation while requiring confirmation for delete, external model, and external network effects.
- Focused tests cover the extracted policy independently from host integration tests.

Progress:

- Added `ReadOsOperatorApprovalPolicy` as an app-side `IMspPolicy` implementation.
- Kept one-shot approval tokens bound to command text and actor, with host-approved execution passing the token through the MSP request environment.
- Left `ReadOsMspHost` responsible for command registration, workspace wiring, and approved execution orchestration only.
- Added focused App test coverage for token consumption, token mismatch behavior, confirm-all reads, and allow-workspace effect handling.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 68 tests after extracting the operator approval policy.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T25.

Result: the first stable host boundary has been split out: operator approval policy is now a separately testable component while the app host still owns domain command registration and execution orchestration.

### T26: MSP Session Store Extraction

Status: done.

Goal: move durable MSP transcript/session projection rules out of `ShellViewModel` into a focused app service so the host/session boundary can be tested before any `ReadOS.Msp.Hosting` project split.

Acceptance criteria:

- Transcript refresh, ordering, trimming, and default session normalization are owned by a service instead of direct ViewModel code.
- Transcript insert/update persistence still replaces existing records, caps retained records, and rebuilds affected sessions.
- Transcript removal rebuilds the affected session and keeps artifact-only sessions when artifacts still reference that session.
- Session rebuilds preserve existing behavior for command counts, running counts, pending approvals, approvals, failures, last command fields, transcript IDs, and artifact paths.
- `ShellViewModel` remains responsible for UI collections, selection markers, timeline refresh, and activity notifications.
- Focused service tests cover rebuild, trim, replace, removal, and artifact linkage.

Progress:

- Added `ReadOsMspSessionStore` under `Services/Msp` for transcript refresh, persistence, removal, and session rebuild behavior.
- Rewired `ShellViewModel` to delegate transcript/session maintenance to the store while keeping existing UI state updates in the ViewModel.
- Added service-level tests for sorted transcript refresh, stale session cleanup, record replacement, transcript cap trimming, artifact-only sessions, and session state projection.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 73 tests after extracting the MSP session store.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T26.

Result: durable transcript/session projection is now an app service with direct coverage, reducing `ShellViewModel` ownership and creating a clearer migration target for a future hosting layer.

### T27: Artifact Service Extraction

Status: done.

Goal: move stable artifact projection, lineage, export metadata, content preview, and workflow artifact path rules out of `ShellViewModel` into a focused app service before any hosting-layer split.

Acceptance criteria:

- Artifact filtering and ordering are owned by a service instead of direct ViewModel query code.
- Selected artifact preview/content and export filename/extension decisions are reusable outside the ViewModel.
- Artifact lineage generation, including source artifacts, manifests, virtual pages, sessions, transcripts, source documents, and source page ranges, is covered by service tests.
- Evidence-artifact detection and workflow artifact path generation are owned by the service.
- `ShellViewModel` remains responsible for UI selection, status messages, file dialogs, clipboard writes, attachments, and presenter state.
- Existing artifact inspector, composer steer, guided workflow, export/copy/preview, and attachment behavior remains covered by App tests.

Progress:

- Added `ReadOsArtifactService` under `Services/Msp` for visible artifact projection, content/preview selection, export metadata, lineage construction, evidence detection, artifact lookup, and workflow artifact path generation.
- Rewired `ShellViewModel` artifact refresh, lineage refresh, preview/copy/export content selection, guided artifact workflow commands, and failure-review artifact paths through the service.
- Kept clipboard, file export, attachment queueing, selected inspector routing, and presenter state in the ViewModel where they belong.
- Added service-level tests for artifact filtering/order, lineage de-duplication and typing, content/preview/export metadata, path slugging, artifact lookup, and evidence detection.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 78 tests after extracting the artifact service.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T27.

Result: artifact catalog, lineage, reuse metadata, and workflow path rules now have a separately tested service boundary while UI-specific artifact actions remain in the workbench ViewModel.

### T28: MSP Command Transcript Projection Extraction

Status: done.

Goal: move MSP command transcript state projection out of `ShellViewModel` so streaming execution remains UI-owned but running/event/completion transcript rules are separately testable.

Acceptance criteria:

- Running transcript entry creation is owned by a service and preserves actor, session, command text, timestamps, progress, and running state.
- MSP streaming events are applied by a service, including session updates, progress messages, policy decisions, effects, previews, completion, and cancellation state.
- Final `MspCommandResult` projection into transcript stdout/stderr, exit code, decision, effects, artifact summary, policy preview, diagnostics, and recovery hints is owned by the service.
- Operator cancellation state uses the same transcript projection service instead of ad hoc ViewModel mutation.
- `ShellViewModel` remains responsible for async command execution, cancellation token lifetime, UI routing, timeline refresh, persistence, and workspace save.
- Focused service tests cover running entries, policy events, progress/completed/canceled events, final result projection, cancellation mapping, and diagnostic formatting.

Progress:

- Added `ReadOsMspCommandTranscriptService` under `Services/Msp`.
- Rewired `ShellViewModel` command execution to create running entries, apply streaming events, mark cancellation, and complete entries through the service.
- Kept UI decisions such as opening the Policy inspector and refreshing timeline/activity state in the ViewModel.
- Added service-level tests for event and result projection behavior.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 84 tests after extracting the transcript projection service.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T28.

Result: MSP command transcript projection now has a tested service boundary, leaving the workbench ViewModel focused on async orchestration and UI state.

### T29: Prepared Command History Extraction

Status: done.

Goal: move prepared MSP command history rules out of `ShellViewModel` so workflow draft history can be tested and reused independently from inspector UI routing.

Acceptance criteria:

- Recording a prepared command trims command text, ignores blank commands, normalizes the display title, inserts the newest item first, and keeps command text as the detail.
- Recording an existing command removes the older entry and promotes the new prepared command without duplicating history.
- History size is capped by the configured maximum.
- Restoring/promoting an existing command moves it to the top without changing command text or running the command.
- `ShellViewModel` remains responsible for setting `MspCommandDraft`, opening the Run inspector, status messages, and explicit manual execution.
- Focused service tests cover record, blank ignore, duplicate promotion, cap behavior, restore promotion, and title normalization.

Progress:

- Added `ReadOsPreparedMspCommandHistoryService` under `Services/Msp`.
- Rewired `ShellViewModel` prepared command recording and promotion through the service.
- Kept restore UI behavior in the ViewModel so command drafts are not executed implicitly.
- Added service-level tests for prepared command history behavior.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 90 tests after extracting prepared command history.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T29.

Result: prepared MSP workflow drafts now have a tested history service while the workbench still controls when restored drafts appear and when commands execute.

### T30: Active MSP Command Orchestration Extraction

Status: done.

Goal: move active MSP command cancellation ownership out of `ShellViewModel` so command execution state is tracked by a focused service before a future hosting-layer split.

Acceptance criteria:

- The active command service owns the current entry ID and linked cancellation token source.
- Beginning a command returns a registration with the command token used by streaming execution.
- Cancellation only applies to the matching running transcript entry.
- Non-running, null, stale, or different entries do not cancel the active token.
- Disposing the active registration clears the active command, while disposing an older overwritten registration does not clear a newer active command.
- `ShellViewModel` remains responsible for UI progress text, status messages, command execution, transcript persistence, and timeline refresh.
- Focused service tests cover begin, parent-token linking, cancel validation, dispose cleanup, and older-registration ownership semantics.

Progress:

- Added `ReadOsActiveMspCommandService` under `Services/Msp`.
- Rewired `ShellViewModel` to create active command registrations through the service and use the registration token for MSP streaming execution.
- Rewired cancel commands to validate/cancel through the service while keeping UI progress/status mutation in the ViewModel.
- Added service-level tests for active command cancellation behavior.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 95 tests after extracting active command orchestration.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T30.

Result: active MSP command cancellation now has a tested ownership boundary, reducing ViewModel state before any `ReadOS.Msp.Hosting` extraction.

### T31: Attachment Queue Service Extraction

Status: done.

Goal: move composer attachment queue and queued prompt attachment snapshot rules out of `ShellViewModel` so evidence queue state is separately testable before further host/UI separation.

Acceptance criteria:

- Attachment snapshots clone attachment state without reusing attachment instances or IDs.
- Pending attachment removal is owned by a service and matches attachment IDs case-insensitively.
- Queueing a composer draft trims prompt text, clones pending attachments, appends a queued prompt, and clears pending attachments.
- Empty composer drafts without attachments are ignored by queueing logic.
- Restoring a queued prompt clones queued attachments back into pending attachments and removes the queued item only when the caller permits an empty composer state.
- Artifact attachment creation is owned by the service and preserves virtual artifact path/title/document context.
- `ShellViewModel` remains responsible for composer text assignment, status messages, selected inspector tab, send execution, and UI notifications.

Progress:

- Added `ReadOsAttachmentQueueService` under `Services/Msp`.
- Rewired `ShellViewModel` pending attachment snapshots, remove/clear, queued composer draft creation/restoration, send snapshots, MSP attachment clearing, and artifact attachment creation through the service.
- Added service-level tests for clone snapshots, remove-by-id, queue draft behavior, restore behavior, clear, and artifact attachment creation.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 102 tests after extracting the attachment queue service.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T31.

Result: composer evidence attachment queue state now has a tested service boundary while the workbench keeps UI-specific composer, status, and send orchestration.

### T32: Approval Review Flow Extraction

Status: done.

Goal: move pending MSP approval review rules out of `ShellViewModel` so approve/deny UI flow relies on a focused service for review eligibility, visible transcript removal, and denied transcript generation.

Acceptance criteria:

- A service determines whether a transcript entry is reviewable as a pending approval.
- Removing a pending approval from the visible transcript collection returns the original index for replacement workflows.
- Non-approval, null, or missing entries are ignored without mutating visible transcripts.
- Denying a command creates a new transcript entry that preserves actor, session, command text, start time, effects, artifact summary, and policy preview while recording exit code 126, `Deny`, stable denial diagnostics, and recovery hint.
- `ShellViewModel` remains responsible for approved retry execution, workspace transcript persistence/removal, selected transcript state, status messages, and saving.
- Focused service tests cover reviewability, removal, ignored inputs, and denied transcript field mapping.

Progress:

- Added `ReadOsMspApprovalReviewService` under `Services/Msp`.
- Rewired `ShellViewModel` approve and deny commands to use the service for pending approval removal and denied transcript generation.
- Kept actual approved command re-execution and UI persistence orchestration in the ViewModel.
- Added service-level tests for approval review behavior.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 106 tests after extracting approval review flow.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T32.

Result: pending approval review now has a tested service boundary while the workbench still owns explicit retry, persistence, and selected-inspector behavior.

### T33: Agent MSP Bridge Service Extraction

Status: done.

Goal: move agent-facing MSP prompt guidance, command-request parsing, and execution-report formatting out of `ShellViewModel` so chat/MSP bridge rules are independently testable before any hosting-layer split.

Acceptance criteria:

- A service builds the MSP agent instruction used for model calls.
- The service extracts requested commands from fenced `msp`, `reados-msp`, `reados_msp-sh`, `<msp>`, and `<reados-msp>` blocks.
- Command extraction ignores blank lines and comment lines, and normalizes `$ ` and `msp>` prompts.
- MSP execution reports preserve command text, exit code, decision, effects, artifacts, stdout, stderr, diagnostics, and recovery hints.
- Large stdout/stderr/diagnostic fields keep the existing report truncation limits.
- `ShellViewModel` remains responsible for chat message orchestration, command execution, transcript persistence, status messages, and inspector selection.

Progress:

- Added `ReadOsMspAgentBridgeService` under `Services/Msp`.
- Rewired `ShellViewModel` chat/model flow to use the service for MSP instructions, requested-command extraction, and agent execution report formatting.
- Removed the duplicated MSP bridge helper methods from `ShellViewModel`.
- Added service-level tests for instruction content, command block parsing, ignored unmarked text, execution report content, and large-output trimming.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 111 tests after extracting the agent MSP bridge service.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T33.

Result: agent-facing MSP bridge text, command parsing, and report formatting now have a tested service boundary while the workbench still owns chat turns and command execution.

### T34: Chat Turn Service Extraction

Status: done.

Goal: move chat turn message/prompt construction out of `ShellViewModel` so the workbench send flow relies on a focused service for deterministic user, assistant, MSP, final-answer, and failure message shaping.

Acceptance criteria:

- A service resolves the effective prompt from composer draft, attachment-only sends, and empty sends.
- User messages preserve the resolved prompt, author, role, timestamp, and pending attachment snapshot.
- ReadOS assistant, MSP report, and failure messages are created through the same service.
- The MSP final-answer prompt is owned by the service and reuses the original resolved user prompt.
- `ShellViewModel` remains responsible for model calls, MSP command execution, workspace persistence, timeline refresh, status messages, and busy state.
- Focused service tests cover prompt selection, message authors/roles/timestamps, attachment preservation, final-answer prompt construction, and failure formatting.

Progress:

- Added `ReadOsChatTurnService` under `Services/Msp`.
- Rewired `ShellViewModel.SendPromptAsync` to use the service for prompt resolution and chat message creation.
- Kept model invocation, MSP command execution, persistence, and UI state orchestration in the ViewModel.
- Added service-level tests for chat turn construction behavior.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 118 tests after extracting chat turn construction.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T34.

Result: chat turn prompt and message shaping now has a tested service boundary while the workbench still owns model/runtime execution and persistence.

### T35: Conversation Collection Service Extraction

Status: done.

Goal: move conversation list, message projection, and ensure/create conversation collection rules out of `ShellViewModel` so chat session state is separately testable before deeper chat/runtime orchestration work.

Acceptance criteria:

- A service returns visible document conversations sorted newest-first when a document is selected.
- The service returns project standalone conversations sorted newest-first when no document is selected.
- Message projection preserves the selected conversation's existing message order and returns an empty set for null selections.
- Ensuring a conversation no-ops when the current selection can be reused.
- Ensuring a missing document or project conversation creates the correct titled conversation and inserts it first.
- Existing document/project conversation lists refresh without creating a duplicate when no conversation is selected.
- Forced new project/document conversations use the next title number and provided timestamp.
- `ShellViewModel` remains responsible for selected item assignment, observable collection updates, timeline refresh, persistence, and command handling.

Progress:

- Added `ReadOsConversationService` under `Services/Msp`.
- Rewired `ShellViewModel.RefreshConversations`, `RefreshChatMessages`, and `EnsureConversation` to delegate collection rules to the service.
- Kept UI selection, observable collection mutation, timeline refresh, and save orchestration in the ViewModel.
- Added service-level tests for document/project sorting, message projection, conversation reuse, creation, refresh-without-duplicate, and forced project conversation creation.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 125 tests after extracting conversation collection rules.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T35.

Result: conversation list projection, message projection, and create-if-needed rules now have a tested service boundary while the workbench still owns UI selection and persistence.

### T36: MSP Session View Service Extraction

Status: done.

Goal: move MSP session visibility, filtering, stale-selection detection, and session-to-transcript inspector routing out of `ShellViewModel` so session UI rules are separately testable before a future hosting/view-model split.

Acceptance criteria:

- A service returns visible MSP sessions filtered by session ID, title, last command text, or last diagnostics summary.
- Visible sessions are sorted by newest update first, then title case-insensitively.
- Stale selected sessions are detected case-insensitively after filtering.
- Selecting a session chooses the first visible transcript whose ID belongs to the session.
- Pending approval and failed completed transcripts route to the Policy inspector.
- Running, successful, missing, and null selections route to the Run inspector.
- `ShellViewModel` remains responsible for observable collection updates, selected transcript assignment, inspector visibility, and session persistence/rebuilds.

Progress:

- Added `ReadOsMspSessionViewService` under `Services/Msp`.
- Rewired `ShellViewModel.RefreshMspSessions` to delegate filtering, sorting, and stale-selection checks to the service.
- Rewired `ShellViewModel.OnSelectedMspSessionChanged` to delegate transcript selection and inspector tab routing to the service.
- Added service-level tests for search fields, sort order, stale selection, pending/failure routing, running/missing/null routing, and first-visible transcript selection.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 131 tests after extracting MSP session view rules.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T36.

Result: MSP session filtering, stale selection, and inspector routing now have a tested service boundary while the workbench still owns UI mutation and persistence.

### T37: MSP Transcript Workspace Service Extraction

Status: done.

Goal: move workspace transcript/session-store entry points out of `ShellViewModel` so refresh, persist, remove, and rebuild operations pass through a focused bridge before deeper hosting-layer extraction.

Acceptance criteria:

- A service wraps transcript refresh, persistence, removal, and full session rebuild calls.
- Null workspace inputs return empty or false results without mutating UI state.
- Refresh still delegates sorting, trimming, session ID normalization, and session rebuild behavior to `ReadOsMspSessionStore`.
- Persist still normalizes session IDs and rebuilds affected sessions through the store.
- Remove returns false for null or missing entries and delegates successful removal to the store.
- Rebuild returns false for null workspace and delegates full session rebuilding when workspace state exists.
- `ShellViewModel` no longer directly stores or constructs `ReadOsMspSessionStore`; it remains responsible for observable collection updates, timeline refresh, activity notifications, and saving.

Progress:

- Added `ReadOsMspTranscriptWorkspaceService` under `Services/Msp`.
- Rewired `ShellViewModel.RefreshMspTranscript`, `PersistTranscriptEntry`, `RemoveWorkspaceTranscriptEntry`, and `RebuildAllMspSessions` to use the new service.
- Added a constructor that hides `ReadOsMspSessionStore` creation from the ViewModel.
- Added service-level tests for null workspace handling, refresh delegation, persist delegation, remove behavior, and rebuild behavior.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 139 tests after extracting the transcript workspace bridge.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T37.

Result: ViewModel transcript persistence entry points now go through a tested workspace bridge while session reconstruction rules remain in the existing store.

### T38: Timeline Projection Service Extraction

Status: done.

Goal: move thread timeline projection out of `ShellViewModel` so chat messages, MSP transcript entries, and artifacts are merged and ordered by a focused service.

Acceptance criteria:

- A service builds timeline items from chat messages, MSP transcript entries, and workspace artifacts.
- Chat, MSP, and artifact records continue to use the existing `ThreadTimelineItem` projection rules.
- Timeline items are ordered by created time, then item kind.
- Message attachments remain available on projected timeline items.
- Evidence, MSP approval, MSP error, command, and artifact item kinds remain preserved.
- `ShellViewModel` remains responsible for clearing and filling the observable timeline collection and handling item commands.

Progress:

- Added `ReadOsTimelineService` under `Services/Msp`.
- Rewired `ShellViewModel.RefreshTimelineItems` to delegate timeline item projection and ordering to the service.
- Added service-level tests for merged ordering, time ordering, evidence/approval/error projection, source references, and attachment preservation.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 143 tests after extracting timeline projection.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T38.

Result: timeline projection is now a tested service boundary while the workbench still owns observable collection mutation and timeline item actions.

### T39: Timeline Action Routing Service Extraction

Status: done.

Goal: move timeline item action routing out of `ShellViewModel` so opening artifact, evidence, MSP approval, MSP diagnostic, MSP command, and default message timeline records is driven by a focused decision service.

Acceptance criteria:

- A service resolves null timeline items as no-op actions.
- Artifact timeline items route to the Artifacts inspector, preserve the selected artifact, and produce the existing status message.
- Evidence timeline items route to the Evidence inspector and produce the existing status message.
- Failed MSP transcript items route to the Policy inspector as diagnostics.
- Pending approval MSP transcript items route to the Policy inspector as approval review.
- Normal MSP command items route to the Run inspector.
- Plain message/default items route to Evidence without changing status text.
- `ShellViewModel` remains responsible for applying selected artifact/transcript state, inspector visibility, selected tab, and status messages.

Progress:

- Added `ReadOsTimelineActionService` under `Services/Msp`.
- Rewired `ShellViewModel.OpenTimelineItem` to resolve a timeline action, then apply artifact selection, transcript selection, inspector tab, and status message from the decision.
- Added service-level tests for null, artifact, evidence, failed MSP, approval MSP, normal MSP, and plain message routing.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 150 tests after extracting timeline action routing.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T39.

Result: timeline item routing is now a tested decision service while the workbench still owns actual ViewModel state mutation.

### T40: Workflow Draft Command Service Extraction

Status: done.

Goal: move guided workflow MSP command string composition out of `ShellViewModel` so document workflows, evidence workflows, artifact refinement, and failure-review presets share a focused, tested command builder.

Acceptance criteria:

- A service builds document workflow commands with `--document current`, quoted outline selectors, and quoted artifact paths.
- The service builds review-evidence and synthesize-evidence commands with quoted evidence and artifact paths.
- The service builds refine-artifact commands with quoted source path, instruction, and artifact path.
- The service builds review-failures commands with a quoted artifact path.
- MSP argument quoting escapes quotes and backslashes consistently with the previous ViewModel helper.
- `ShellViewModel` remains responsible for selection validation, artifact path generation, prepared-command history, inspector routing, and status messages.

Progress:

- Added `ReadOsWorkflowDraftService` under `Services/Msp`.
- Rewired document workflow, evidence review/synthesis, artifact refinement, composer refinement, and failure-review draft creation to use the service.
- Removed the `QuoteMspArgument` helper from `ShellViewModel`.
- Added service-level tests for every guided workflow command shape and argument escaping.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 156 tests after extracting workflow draft command composition.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T40.

Result: guided workflow command composition is now a tested service boundary while the workbench still owns validation, path selection, and draft presentation.

### T41: Workflow Preparation Service Extraction

Status: done.

Goal: move guided workflow validation, artifact path selection, command preparation, and failure status decisions out of `ShellViewModel` so document, evidence, refinement, composer-steered, and failure-review workflow presets share a focused preparation service.

Acceptance criteria:

- Document workflow preparation rejects missing/non-PDF document context and missing outline selection with the existing status messages.
- Document workflow preparation chooses outline ID before title and builds the derived artifact path through `ReadOsArtifactService`.
- Evidence review/synthesis preparation rejects missing or non-JSON evidence artifacts with the existing status messages.
- Artifact refinement preparation rejects missing artifacts and uses the configured default instruction supplied by the caller.
- Composer refinement preparation trims the composer instruction and rejects blank instructions.
- Failure-review preparation rejects null, running, pending-approval, or successful transcript entries.
- Successful preparations return the exact command text and status message previously emitted by `ShellViewModel`.
- `ShellViewModel` remains responsible for applying prepared command drafts, prepared-command history, inspector routing, and status assignment.

Progress:

- Added `ReadOsWorkflowPreparationService` under `Services/Msp`.
- Rewired document workflow, evidence review/synthesis, artifact refinement, composer refinement, and failure-review command handlers to use preparation results.
- Added `ApplyWorkflowPreparation` to `ShellViewModel` to apply success or failure results without owning validation/path selection rules.
- Added service-level tests for document context validation, outline selector fallback, evidence validation, refine validation, composer instruction trimming, failure-review eligibility, and all successful command/status outputs.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 167 tests after extracting workflow preparation.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T41.

Result: guided workflow validation and path-selection decisions now have a tested service boundary while the workbench still owns draft application and UI state.

### T42: Artifact Reuse Service Extraction

Status: done.

Goal: move selected-artifact preview, copy, export, and chat-attachment preparation out of `ShellViewModel` so artifact reuse behavior shares a focused service while the ViewModel keeps UI state and I/O.

Acceptance criteria:

- Preview preparation rejects missing artifacts with the existing status message.
- Preview preparation returns artifact content and the existing preview-opened status message.
- Copy preparation returns artifact content and the existing copied status message.
- Export preparation rejects missing artifacts and otherwise returns artifact content plus export metadata.
- Export completion status is formatted by the service.
- Attachment preparation rejects missing artifacts and otherwise returns a virtual artifact attachment, default composer prompt, and existing attached status message.
- `ShellViewModel` remains responsible for presenter state, clipboard writes, file picker interaction, file writes, pending attachment collection updates, and inspector routing.

Progress:

- Added `ReadOsArtifactReuseService` under `Services/Msp`.
- Rewired selected artifact preview, copy, export, and attach commands to use artifact reuse preparation results.
- Kept actual clipboard, file dialog, file write, presenter, and observable collection mutation in the ViewModel.
- Added service-level tests for missing artifact handling, preview/copy content, export metadata/content/status, and attachment creation/default prompt/status.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 175 tests after extracting artifact reuse preparation.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T42.

Result: selected artifact reuse preparation now has a tested service boundary while the workbench still owns UI state and external I/O.

### T43: Artifact Lineage Action Service Extraction

Status: done.

Goal: move artifact lineage open-source routing out of `ShellViewModel` so lineage item clicks use a focused service to find openable source artifacts and produce status messages.

Acceptance criteria:

- Null lineage items are ignored as no-op actions.
- Missing workspace/artifact collections return the existing non-openable source status.
- Missing artifact paths return the existing non-openable source status.
- Artifact lookup remains case-insensitive through the artifact service.
- Openable source artifacts return the matching artifact and existing opened-source status.
- `ShellViewModel` remains responsible for selecting the artifact, opening the inspector, and assigning status text.

Progress:

- Added `ReadOsArtifactLineageActionService` under `Services/Msp`.
- Rewired `ShellViewModel.OpenArtifactLineageItem` to resolve lineage actions through the service.
- Kept artifact selection and inspector routing in the ViewModel.
- Added service-level tests for null input, missing artifact collection, missing path, case-insensitive lookup, and successful open status.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 179 tests after extracting artifact lineage action routing.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T43.

Result: artifact lineage open-source routing now has a tested service boundary while the workbench still owns UI selection and inspector state.

### T44: Pending Approval Navigation Service Extraction

Status: done.

Goal: move pending-approval navigation decisions out of `ShellViewModel` so global and composer approval indicators share a focused service for selecting the review target and status message.

Acceptance criteria:

- Empty or non-approval transcript collections return the existing no-pending-approval status without opening inspector UI.
- The first pending approval in the current visible transcript order is selected.
- Pending approvals route to the Policy inspector with the existing opened-command status message.
- `ShellViewModel` remains responsible for assigning selected transcript state, opening the inspector, and opening the run drawer.

Progress:

- Added `ReadOsMspPendingApprovalNavigationService` under `Services/Msp`.
- Rewired `ShellViewModel.OpenPendingApproval` to consume the service decision.
- Kept inspector visibility, drawer state, and selected transcript assignment in the ViewModel.
- Added service-level tests for empty transcripts, non-approval transcripts, first-pending selection, and Policy inspector/status output.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 183 tests after extracting pending approval navigation.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T44.

Result: pending approval navigation now has a tested service boundary while the workbench still owns UI selection, inspector visibility, and drawer state.

### T45: Active Document Context Service Extraction

Status: done.

Goal: move selected-document context projection out of `ShellViewModel` so active title/page draft state and document load decisions have a focused, tested boundary before further presenter and collection refresh extraction.

Acceptance criteria:

- Missing selected documents clear presenter state without overwriting existing text drafts.
- Selected documents project document-name drafts, current page numbers, page jump text, and current label drafts with the existing page-clamp behavior.
- Empty and text-like documents preserve the existing load decisions so the ViewModel can clear or load presenter content through existing paths.
- `ShellViewModel` remains responsible for assigning observable state, refreshing document collections, notifying dependent properties, and starting page/thumbnail loads.

Progress:

- Added `ReadOsActiveDocumentContextService` under `Services/Msp`.
- Rewired `ShellViewModel.OnSelectedDocumentChanged` to consume the context projection.
- Kept observable assignment, collection refresh, guided workflow notification, and async page/thumbnail loading in the ViewModel.
- Added service-level tests for missing documents, clamped PDF page projection, empty documents, and Markdown/text document load decisions.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 187 tests after extracting active document context projection.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T45.

Result: selected-document context refresh now has a tested service boundary while the workbench still owns observable UI state, collection refresh, and presenter loading.

### T46: Document Collection Refresh Service Extraction

Status: done.

Goal: move document collection refresh projection out of `ShellViewModel` so outline ordering and visible conversation selection are produced by a focused service before presenter-load behavior is extracted.

Acceptance criteria:

- Missing selected documents return an empty outline without changing conversation fallback behavior.
- Document outlines are projected in existing page-then-level order.
- Selected-document conversations are preferred over project standalone conversations and keep existing newest-first ordering.
- Project conversations are used when no document is selected.
- `ShellViewModel` remains responsible for clearing/filling observable collections, selecting the active conversation, refreshing messages, and raising dependent property notifications.

Progress:

- Added `ReadOsDocumentCollectionRefreshService` under `Services/Msp`.
- Rewired `ShellViewModel.RefreshDocumentCollections`, `RefreshOutline`, and `RefreshConversations` to consume projected refresh results while keeping observable mutation in the ViewModel.
- Reused `ReadOsConversationService` for conversation visibility and ordering rules.
- Added service-level tests for null document outlines, outline ordering, document conversation preference, and project conversation fallback.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 191 tests after extracting document collection refresh projection.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T46.

Result: document collection refresh now has a tested service boundary while the workbench still owns observable collection mutation, active conversation assignment, and timeline refresh.

### T47: Presenter Load Preparation Service Extraction

Status: done.

Goal: move presenter load preparation out of `ShellViewModel` so missing documents, text documents, empty documents, and renderable PDFs use a focused decision service while the ViewModel keeps file/PDF I/O and UI state mutation.

Acceptance criteria:

- Missing selected documents clear image/text presenter state and notify active context through the existing ViewModel path.
- Markdown and note documents clear page images, keep existing text until file load completes, load text through the existing file I/O path, and notify active context afterward.
- Empty PDFs or unsupported document kinds clear presenter state without attempting PDF render.
- Renderable PDFs clear text presenter state, preserve the existing render page clamp, update page labels after render, and refresh page signals.
- `ShellViewModel` remains responsible for artifact-preview state, `CurrentPageImage`, `PresenterTextContent`, file reads, PDF rendering, status messages, and property notifications.

Progress:

- Added `ReadOsPresenterLoadPreparationService` under `Services/Msp`.
- Rewired `ShellViewModel.LoadCurrentPageAsync` to apply presenter load preparation results before invoking existing text loading or PDF rendering.
- Kept `LoadPresenterTextAsync`, `pdfService.RenderPageAsync`, render failure status, and observable property writes in the ViewModel.
- Added service-level tests for missing documents, Markdown/note text load preparation, empty PDF clearing, and PDF render-page preparation.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 197 tests after extracting presenter load preparation.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T47.

Result: presenter load preparation now has a tested service boundary while the workbench still owns presenter state mutation, file reads, PDF rendering, and render error reporting.

### T48: Thumbnail Load Preparation Service Extraction

Status: done.

Goal: move thumbnail load preparation and rendered-thumbnail projection out of `ShellViewModel` so PDF thumbnail loading has a focused decision boundary before page navigation persistence is extracted.

Acceptance criteria:

- Missing and non-PDF selected documents clear thumbnail state without attempting PDF thumbnail rendering.
- PDF documents request thumbnails with the existing page count and 24-page limit.
- Rendered thumbnails receive existing page-label fallback behavior.
- The selected thumbnail is resolved from the current page when that page was rendered; otherwise selection is cleared.
- `ShellViewModel` remains responsible for invoking `RenderThumbnailsAsync`, clearing/filling `Thumbnails`, suppressing thumbnail selection events, assigning `SelectedThumbnail`, and reporting render failures.

Progress:

- Added `ReadOsThumbnailLoadPreparationService` under `Services/Msp`.
- Rewired `ShellViewModel.LoadThumbnailsAsync` to use thumbnail load preparation and projection results.
- Kept PDF thumbnail rendering, observable collection mutation, selection suppression, and error status messages in the ViewModel.
- Added service-level tests for missing documents, non-PDF documents, PDF render preparation, page label projection, and missing selected-page projection.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 202 tests after extracting thumbnail load preparation.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T48.

Result: thumbnail load preparation now has a tested service boundary while the workbench still owns thumbnail rendering I/O, observable thumbnail state, and selection suppression.

### T49: Page Navigation Persistence Service Extraction

Status: done.

Goal: move page navigation target resolution and persistence intent out of `ShellViewModel` so previous/next, thumbnail, outline, search-result, and jump-to-page paths share a focused service before page-label and outline editing boundaries are extracted.

Acceptance criteria:

- Navigation no-ops when workspace state, selected document, or document pages are unavailable.
- Requested page numbers clamp to the selected document page range.
- Successful navigation returns page jump text, current page-label draft, load-page intent, and save-workspace intent.
- Page text resolution keeps existing numeric clamp behavior.
- Page text resolution keeps existing case-insensitive page-label lookup behavior and returns zero for missing labels.
- `ShellViewModel` remains responsible for assigning `CurrentPageNumber`, mutating `SelectedDocument.CurrentPage`, loading the current page, and saving the workspace.

Progress:

- Added `ReadOsPageNavigationService` under `Services/Msp`.
- Rewired `ShellViewModel.GoToPageAsync` to consume navigation preparation before assigning observable state, loading the page, and saving workspace state.
- Rewired `ShellViewModel.ResolvePage` to delegate numeric and label lookup to the service so jump and range parsing share the same tested rules.
- Added service-level tests for missing prerequisites, empty documents, page clamping, page-label draft projection, numeric page resolution, case-insensitive labels, and unknown labels.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 214 tests after extracting page navigation persistence preparation.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T49.

Result: page navigation persistence now has a tested service boundary while the workbench still owns UI state assignment, page loading, and workspace persistence I/O.

### T50: Page Label Editing Service Extraction

Status: done.

Goal: move page-label editing rules out of `ShellViewModel` so manual label saves and automatic page-label mapping have a focused service boundary before outline editing is extracted.

Acceptance criteria:

- Manual label save no-ops when workspace state, selected document, or current page is unavailable.
- Manual label save creates a missing `PageLabelRule` for the current page.
- Manual label save updates an existing page label, trims label drafts, and falls back to the page number for blank drafts.
- Auto-map no-ops when workspace state, selected document, or document pages are unavailable.
- Auto-map replaces existing page labels with the current cover/i/ii/iii/page-minus-four mapping and returns the current page label draft.
- `ShellViewModel` remains responsible for assigning `CurrentPageLabelDraft`, saving workspace state, and applying status messages.

Progress:

- Added `ReadOsPageLabelEditingService` under `Services/Msp`.
- Rewired `ShellViewModel.SavePageLabelAsync` and `AutoMapPagesAsync` to consume page-label edit results.
- Kept workspace persistence and observable/status assignment in the ViewModel.
- Added service-level tests for missing prerequisites, create/update label behavior, draft trimming/fallback, auto-map no-op behavior, and default auto-map labels.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 220 tests after extracting page-label editing.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T50.

Result: page-label editing now has a tested service boundary while the workbench still owns workspace save I/O and user-facing status assignment.

### T51: Outline Editing Service Extraction

Status: done.

Goal: move outline editing rules out of `ShellViewModel` so generated outlines, fallback outline creation, manual add, and delete behavior use a focused service boundary before document search and remaining reader command boundaries are extracted.

Acceptance criteria:

- Outline generation no-ops when workspace state, selected document, or PDF state is unavailable.
- Extracted outline text preserves current page-marker tracking and supported heading detection for chapter/section, Chinese chapter headings, and numbered headings.
- Generated outlines replace existing document outline entries and keep the existing 80-item cap.
- Missing generated headings fall back to page-1, page-11, page-21 style outline entries.
- Manual add trims draft titles, falls back to the current-page title, uses minimum page 1, and asks the ViewModel to clear the draft.
- Delete no-ops when prerequisites are missing and removes the selected outline item otherwise.
- `ShellViewModel` remains responsible for PDF text extraction, busy state, outline collection refresh, workspace save I/O, and failure status messages.

Progress:

- Added `ReadOsOutlineEditingService` under `Services/Msp`.
- Rewired `ShellViewModel.GenerateOutlineAsync`, `AddOutlineItemAsync`, and `DeleteOutlineItemAsync` to consume outline edit results.
- Removed the old `ExtractOutlineFromText` helper from `ShellViewModel`.
- Kept `ExtractPageTextAsync`, `RefreshOutline`, `SaveWorkspaceAsync`, busy state, and exception status handling in the ViewModel.
- Added service-level tests for missing prerequisites, heading extraction, generated outline replacement/capping, fallback outlines, manual add behavior, and deletion behavior.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 228 tests after extracting outline editing.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T51.

Result: outline editing now has a tested service boundary while the workbench still owns PDF extraction, observable refresh, workspace persistence, and user-facing failure handling.

### T52: Document Search Routing Service Extraction

Status: done.

Goal: move document search preparation, search-result status projection, and search-hit routing out of `ShellViewModel` so document search has a focused service boundary before the remaining reader commands are extracted.

Acceptance criteria:

- Search clears existing results for every command invocation.
- Search no-ops when there is no selected document, the selected material is not a PDF, or the query is blank.
- Search preparation preserves the existing query text passed to `SearchAsync`.
- Search result projection returns the existing empty and hit-count status messages.
- Selecting a search hit routes to that hit's page; missing hits no-op.
- `ShellViewModel` remains responsible for invoking PDF search I/O, busy state, observable result collection mutation, page navigation, and exception status messages.

Progress:

- Added `ReadOsDocumentSearchService` under `Services/Msp`.
- Rewired `ShellViewModel.SearchInDocumentAsync` to consume search preparation and result projection.
- Rewired `ShellViewModel.OnSelectedSearchResultChanged` to consume search-hit route results before calling the existing page navigation path.
- Kept `pdfService.SearchAsync`, `DocumentSearchResults`, `IsBusy`, page navigation, and exception status handling in the ViewModel.
- Added service-level tests for unavailable search inputs, query preservation, empty/hit-count status projection, and hit route resolution.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 234 tests after extracting document search routing.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T52.

Result: document search routing now has a tested service boundary while the workbench still owns PDF search I/O, observable results, busy state, and navigation execution.

### T53: Reader Attachment Command Service Extraction

Status: done.

Goal: move reader attachment preparation out of `ShellViewModel` so current-page attachments, page-range attachments, text-file attachments, and region-selection attachments share a focused service boundary before remaining reader layout/toggle commands are reviewed.

Acceptance criteria:

- Current-page attachment no-ops without a selected document or current page.
- Text and note materials attach as full-file attachments with the existing title shape.
- PDF current-page attachment produces the existing single-page attachment.
- Page-range attachment keeps existing numeric and page-label parsing, reversed-range normalization, single-page/page-range kind selection, draft clearing, and invalid-range status message.
- Region attachment keeps existing context-page bounds, region coordinates, composer prompt, region-mode exit, and status message.
- `ShellViewModel` remains responsible for adding attachments to `PendingAttachments`, notifying attachment state, updating composer/page-range drafts, exiting region mode, and applying status messages.

Progress:

- Added `ReadOsReaderAttachmentService` under `Services/Msp`.
- Rewired `ShellViewModel.AttachCurrentPage`, `AttachRange`, and `AttachRegionSelection` to consume reader attachment results.
- Removed the old range parsing helper from `ShellViewModel`.
- Reused `ReadOsPageNavigationService` for numeric and page-label range parsing.
- Added service-level tests for no-op inputs, text attachments, PDF current-page attachment, invalid ranges, mixed label/range parsing, and region attachments.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 242 tests after extracting reader attachment commands.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T53.

Result: reader attachment commands now have a tested service boundary while the workbench still owns pending attachment mutation, UI notifications, and composer/region state updates.

### T54: Reader Layout Toggle Service Extraction

Status: done.

Goal: move remaining reader layout and toggle decisions out of `ShellViewModel` so route-to-reader, workspace layout presets, sidebar/tab string selections, outline visibility, and run-drawer pinning are resolved by a focused service before the hosting-split readiness pass.

Acceptance criteria:

- Reader navigation routes to the existing Home route, opens the inspector preview, and selects the Materials sidebar.
- Workspace layout preset parsing remains case-insensitive and ignores invalid input.
- Focus-chat layout closes the inspector; presenter-focused layouts open Preview; other layouts open the inspector without changing the tab.
- Sidebar mode parsing opens the library/sidebar only for valid modes.
- Inspector tab parsing opens the chat area only for valid tabs.
- Outline toggle keeps outline and chat visibility synchronized and routes the inspector tab to Evidence.
- Run drawer pinning opens the drawer when pinning and preserves the existing open state when unpinning.
- `ShellViewModel` remains responsible for assigning observable route, visibility, tab, sidebar, drawer, and layout properties.

Progress:

- Added `ReadOsReaderLayoutService` under `Services/Msp`.
- Rewired `ShellViewModel.NavigateReader`, `SetWorkspaceLayout`, `ApplyWorkspaceLayoutPreset`, `SelectSidebarMode`, `SelectInspectorTab`, `ToggleOutline`, and `PinRunDrawer` to consume layout decisions.
- Kept direct property assignment and property-change notifications in the ViewModel.
- Added service-level tests for reader navigation, layout presets, invalid layout modes, sidebar/tab parsing, outline toggling, and run-drawer pinning.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 256 tests after extracting reader layout/toggle decisions.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T54.

Result: reader layout and toggle commands now have a tested service boundary while the workbench still owns observable UI state updates.

### T55: Hosting Split Readiness Projection

Status: done.

Goal: make the future `ReadOS.Msp.Hosting` split concrete before creating the project by capturing the extracted host/session/policy/artifact boundaries in tested app-side code.

Acceptance criteria:

- The readiness report names the current app host and planned hosting project.
- Session/transcript, operator policy, artifact catalog/reuse, and active-command cancellation boundaries are marked ready for hosting extraction.
- Command host composition is marked as still needing an explicit host contract.
- Document/PDF/chat adapters and workbench UI projection remain app-owned.
- Parser and runtime dispatch remain owned by `ReadOS.Msp`.
- Next extraction steps are listed in the intended order.

Progress:

- Added `ReadOsMspHostingReadinessService` under `Services/Msp`.
- Added `ReadOsMspHostingBoundaryStatus`, boundary, and report projections for the hosting split.
- Added service-level tests that lock down ready boundaries, app-owned boundaries, runtime-owned boundaries, and next-step order.
- Kept the service read-only and disconnected from UI/runtime execution so this slice documents architecture without changing behavior.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 260 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after closing T55.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: the hosting split now has a tested readiness map that identifies which extracted services can move toward `ReadOS.Msp.Hosting` and which app/runtime responsibilities should stay outside that project.

### T56: First Hosting Project Boundary

Status: done.

Goal: create the first `ReadOS.Msp.Hosting` project boundary and move the safest host-neutral policy/cancellation logic behind it while keeping app adapters in `ReadOS.App`.

Acceptance criteria:

- `ReadOS.Msp.Hosting` and `ReadOS.Msp.Hosting.Tests` are registered in `ReadOS.sln`.
- Hosting exposes initial session, artifact, policy, cancellation, and command-host contracts.
- Active command cancellation token ownership moves into Hosting, with the app service reduced to a transcript-entry adapter.
- One-shot approval token storage and consumption moves into Hosting, with `ReadOsOperatorApprovalPolicy` retaining only app approval-mode decisions.
- `ReadOsMspHost` implements a host-neutral command execution interface without moving document/PDF/chat command adapters out of the app.
- `verify-msp.ps1` runs the Hosting test project by default.

Progress:

- Added `src/ReadOS.Msp.Hosting` with `IMspCommandHost`, `IMspSessionProjectionStore`, `IMspArtifactCatalog`, `IMspApprovalGrantStore`, and `IMspActiveCommandRegistry`.
- Added `MspActiveCommandRegistry` and `MspApprovalGrantStore` implementations under Hosting.
- Rewired `ReadOsActiveMspCommandService` to delegate cancellation registration/cancel checks to Hosting.
- Rewired `ReadOsOperatorApprovalPolicy` to delegate one-shot approval grants to Hosting.
- Added `tests/ReadOS.Msp.Hosting.Tests` with focused coverage for active command cancellation and approval grant semantics.
- Added the Hosting test project to `scripts/verify-msp.ps1`.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 9 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 260 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding Hosting tests to the default verification path.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: the first Hosting project boundary now exists, owns reusable approval-grant and active-command cancellation primitives, and is part of the normal solution and verification flow.

### T57: Hosting Session Projection Core

Status: done.

Goal: move durable MSP session summary projection rules into `ReadOS.Msp.Hosting` while keeping workspace persistence and observable collection mutation in the app.

Acceptance criteria:

- Hosting owns the pure rules that project session records from transcript records and artifact metadata.
- Blank transcript session IDs normalize to the configured default session.
- Running, pending-approval, approval, failure, transcript ID, and artifact path counts stay unchanged.
- Artifact-only sessions continue to be retained.
- App `ReadOsMspSessionStore` delegates projection to Hosting but still owns `WorkspaceState`, collection trimming/removal, and derived property notifications.
- Core transcript/session records carry the running and pending-approval fields needed for host-neutral projection.

Progress:

- Added `MspSessionProjectionService` under `ReadOS.Msp.Hosting/Sessions`.
- Added Hosting tests for default-session normalization, running/pending/failure/artifact projection, and artifact-only sessions.
- Added `IsRunning` to `MspCommandTranscriptRecord`, plus `RunningCount` and `PendingApprovalCount` to `MspSessionRecord`.
- Updated App record mappings so transcript/session running state survives conversion.
- Rewired `ReadOsMspSessionStore.RebuildAllSessions` and `RebuildSession` to apply Hosting projection records back into `WorkspaceState`.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 12 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 260 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after moving session projection into Hosting.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: session summary projection is now a tested host-neutral service, and the app layer is reduced to adapting workspace models and preserving observable state.

### T58: Hosting Artifact Catalog Core

Status: done.

Goal: move artifact catalog filtering, path lookup, content reading, and evidence metadata checks into `ReadOS.Msp.Hosting` while keeping export, clipboard, chat attachment, lineage UI, and workflow path helpers in the app.

Acceptance criteria:

- Hosting owns artifact list filtering and ordering by query, update time, and virtual path.
- Hosting owns case-insensitive virtual artifact path lookup.
- Hosting owns content-vs-preview read fallback and JSON evidence detection.
- App `ReadOsArtifactService` delegates catalog/read metadata to Hosting through a `WorkspaceArtifact` adapter.
- App-specific export metadata, artifact lineage rows, clipboard/chat attachment preparation, and workflow artifact path construction remain in `ReadOS.App`.
- Existing App artifact behavior and tests remain unchanged.

Progress:

- Added `MspArtifactCatalog`, `MspArtifactCatalogItem`, and expanded `IMspArtifactCatalog` under `ReadOS.Msp.Hosting/Artifacts`.
- Added Hosting tests for catalog filtering/order, path lookup, content fallback, and evidence detection.
- Rewired `ReadOsArtifactService.GetVisibleArtifacts`, `FindArtifact`, `GetArtifactContent`, and `IsEvidenceArtifact` to delegate host-neutral rules to Hosting.
- Kept lineage projection, export filename selection, and workflow path slugging in the app service.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 16 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 260 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after moving artifact catalog/read rules into Hosting.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: artifact catalog/read metadata is now covered by a host-neutral service, while app-only artifact actions remain scoped to `ReadOS.App`.

### T59: Workspace Session Projection Store Adapter

Status: done.

Goal: route transcript persistence entry points through the Hosting session store contract while keeping `WorkspaceState` and observable collection mutation in `ReadOS.App`.

Acceptance criteria:

- App exposes a thin `IMspSessionProjectionStore` adapter backed by `WorkspaceState`.
- The adapter returns host-neutral transcript/session records for Hosting-facing callers.
- Refresh, persist, remove, rebuild-one-session, and rebuild-all-sessions operations delegate to the existing app-backed session store.
- Existing null-workspace guards remain in `ReadOsMspTranscriptWorkspaceService`.
- `WorkspaceState`, `MspTranscriptEntry`, `MspSessionEntry`, and observable notifications remain app-owned.
- Tests cover record-level refresh, persist replacement, remove-by-id, and artifact-only session preservation.

Progress:

- Added `ReadOsWorkspaceMspSessionProjectionStore` under `Services/Msp`.
- Exposed default session ID and transcript cap from `ReadOsMspSessionStore` for the adapter.
- Rewired `ReadOsMspTranscriptWorkspaceService` to create an `IMspSessionProjectionStore` per workspace and call the host-facing contract.
- Added App tests for the adapter's record-level refresh, persist, remove, and missing-record behavior.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 264 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 16 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding the workspace-backed session projection adapter.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: transcript persistence now has a Hosting-facing adapter surface, while the app remains the owner of workspace persistence and observable session state.

### T60: Hosting Artifact Provenance Classifier

Status: done.

Goal: move artifact lineage source classification into `ReadOS.Msp.Hosting` while keeping glyphs, visible labels, and UI row projection in `ReadOS.App`.

Acceptance criteria:

- Hosting classifies source artifact paths, manifest sidecars, virtual document pages, virtual documents, sessions, transcripts, document IDs, page ranges, and fallback source paths.
- Duplicate source paths, source document IDs, and source page ranges are deduplicated case-insensitively.
- Existing artifacts are marked openable; missing `/artifacts/...` paths remain non-openable source artifact references.
- App `ReadOsArtifactService.BuildLineage` delegates classification to Hosting and maps host-neutral references back to `ArtifactLineageItem`.
- Existing Artifacts inspector lineage behavior remains unchanged.

Progress:

- Added `MspArtifactSourceClassifier`, `MspArtifactSourceReference`, and `MspArtifactSourceKind` under `ReadOS.Msp.Hosting/Artifacts`.
- Added Hosting tests for full source-reference projection, missing artifact path classification, and null artifact handling.
- Rewired `ReadOsArtifactService.BuildLineage` to call the Hosting classifier and keep only UI label/glyph mapping in the app.
- Removed duplicate document page path parsing and page-range detail formatting from the app artifact service.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 19 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 264 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after moving artifact provenance classification into Hosting.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: artifact provenance source classification is now host-neutral and tested, with the app retaining only presentation mapping for the Artifacts inspector.

### T61: Hosting Session Workspace Projection Paths

Status: done.

Goal: move `/sessions` and `/transcripts` virtual workspace path, listing, lookup, and size-estimation rules into `ReadOS.Msp.Hosting` while keeping actual workspace access and JSON serialization in the app.

Acceptance criteria:

- Hosting projects session records to `/sessions/{id}.json` workspace entries.
- Hosting projects transcript records to `/transcripts/{id}.json` workspace entries.
- Session and transcript record lookup is case-insensitive and rejects non-record paths.
- Size estimation for session/transcript JSON projections is owned by Hosting.
- `ReadOsVirtualWorkspace` delegates session/transcript list and read lookup to Hosting.
- App still owns `WorkspaceState`, `IWorkspaceStore`, PDF/document paths, artifact write/delete behavior, and JSON serialization.

Progress:

- Added `MspSessionWorkspaceProjectionService` under `ReadOS.Msp.Hosting/Sessions`.
- Added Hosting tests for session entry projection, transcript entry projection, record lookup, and invalid path rejection.
- Rewired `ReadOsVirtualWorkspace.ListAsync` and `TryReadTextAsync` for `/sessions` and `/transcripts` to use the Hosting projection service.
- Removed duplicate session/transcript size-estimation logic from `ReadOsVirtualWorkspace`.
- Included running and pending-approval counts in session JSON projection and running state in transcript JSON projection.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 23 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 264 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after moving session/transcript virtual workspace projection paths into Hosting.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: session and transcript virtual workspace projection metadata is now host-neutral and tested, with the app reduced to supplying workspace records and serializing them.

### T62: Hosting Approved Command Orchestration

Status: done.

Goal: move command request construction and approved-command execution orchestration into `ReadOS.Msp.Hosting` while keeping ReadOS command registration and document/PDF/chat adapters in `ReadOS.App`.

Acceptance criteria:

- Hosting owns default actor/session/working-directory request construction.
- Hosting owns approved command execution orchestration around `IMspCommandHost` and `IMspApprovalGrantStore`.
- Approval grants are injected into request environment and revoked after normal execution, host failures, and streaming enumeration.
- `ReadOsMspHost` delegates normal request construction and approved execution to Hosting.
- `ReadOsMspHost` still owns ReadOS command registration, workspace wiring, runtime construction, and app-domain command adapters.

Progress:

- Added `MspCommandRequestFactory` under `ReadOS.Msp.Hosting/Runtime`.
- Added `MspApprovedCommandExecutor` under `ReadOS.Msp.Hosting/Runtime`.
- Added Hosting tests for request defaults, request overrides, approved one-shot environment injection, revoke-on-execute, revoke-on-throw, and revoke-after-streaming.
- Rewired `ReadOsMspHost` to share one `MspApprovalGrantStore` between `ReadOsOperatorApprovalPolicy` and `MspApprovedCommandExecutor`.
- Rewired `ReadOsMspHost.ExecuteAsync`, `ExecuteStreamingAsync`, `ExecuteApprovedAsync`, and `ExecuteApprovedStreamingAsync` to use Hosting request/orchestration services.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 28 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 264 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after moving approved-command orchestration into Hosting.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: approved command execution is now a tested Hosting orchestration concern, and the App host is narrower around command registration/runtime wiring.

### T63: Hosting Command Composition Builder

Status: done.

Goal: move the core-registry plus host-command-pack composition rule into `ReadOS.Msp.Hosting` while keeping the concrete ReadOS document/PDF/chat command adapters in `ReadOS.App`.

Acceptance criteria:

- Hosting owns a composition builder that starts from the core MSP registry.
- Host command packs can be registered as `IMspCommand` instances without Hosting knowing app adapter types.
- Host commands can override core commands such as `workflow` when the app needs a domain-specific implementation.
- The composition exposes core command names, host command names, and final command names for tests and diagnostics.
- `ReadOsMspHost` delegates registry composition to Hosting but still constructs the ReadOS command instances.

Progress:

- Added `MspCommandHostComposition` under `ReadOS.Msp.Hosting/Runtime`.
- Added `MspCommandHostCompositionBuilder` under `ReadOS.Msp.Hosting/Runtime`.
- Added Hosting tests for core registry inclusion, host command registration, host override behavior, and custom core registry factories.
- Rewired `ReadOsMspHost` to pass its ReadOS app command instances into the Hosting composition builder.
- Kept workspace, PDF, chat, selected-document, attachment, and workflow adapter construction in `ReadOS.App`.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 31 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 264 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after moving registry composition into Hosting.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: command registry composition is now a tested Hosting concern, and `ReadOsMspHost` is narrower around app command construction plus runtime wiring.

### T64: App Command Pack Factory

Status: done.

Goal: define an app-side command-pack descriptor/factory boundary so `ReadOsMspHost` no longer owns the full ReadOS command construction list inline.

Acceptance criteria:

- App command construction is isolated behind a named factory/descriptor in `ReadOS.App`.
- The command pack exposes the expected app command names: `workspace`, `library`, `pdf`, `windows`, `page-label`, `outline`, `attach`, `chat`, and `workflow`.
- `ReadOsMspHost` delegates app command creation to the factory and passes the command pack into the Hosting composition builder.
- The app `workflow` command still overrides the core workflow command when the command pack is composed with the core registry.
- Document/PDF/chat adapters remain in `ReadOS.App`.

Progress:

- Added `ReadOsMspCommandPack` and `ReadOsMspCommandPackFactory` under `Services/Msp`.
- Moved the ReadOS app command construction list out of `ReadOsMspHost`.
- Rewired `ReadOsMspHost` to create the app command pack and delegate registry composition to `MspCommandHostCompositionBuilder`.
- Added App tests for command-pack names and workflow override behavior through the Hosting composition builder.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 266 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 31 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding the app command-pack factory.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: app command construction now has a tested app-owned boundary, leaving `ReadOsMspHost` focused on workspace/runtime/policy wiring and Hosting composition.

### T65: Hosting Runtime Host Factory

Status: done.

Goal: add runtime context composition helpers around workspace, registry, policy, and audit sink construction where the rules are host-neutral.

Acceptance criteria:

- Hosting owns a small factory that composes `IMspWorkspace`, `MspCommandRegistry`, `IMspPolicy`, and `IMspAuditSink` into `MspCommandContext` plus `MspRuntime`.
- The factory uses an in-memory audit sink by default but accepts an explicit audit sink and service provider.
- `ReadOsMspHost` delegates runtime/context construction to the Hosting factory.
- ReadOS document/PDF/chat command adapters, virtual workspace construction, and app policy decisions remain in `ReadOS.App`.
- Tests prove supplied host components flow into runtime execution and audit recording.

Progress:

- Added `MspRuntimeHost` and `MspRuntimeHostFactory` under `ReadOS.Msp.Hosting/Runtime`.
- Added Hosting tests for supplied workspace/registry/policy/audit/services composition, runtime execution with audit recording, and default audit sink factory behavior.
- Rewired `ReadOsMspHost` to use `MspRuntimeHostFactory` after building its app command pack and operator policy.
- Removed direct audit/context/runtime construction from the app host constructor.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 34 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 266 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding the runtime host factory.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: runtime host composition is now a tested Hosting concern, while app-specific workspace, policy, and command adapter inputs stay in `ReadOS.App`.

### T66: Hosting Command Pack Descriptor

Status: done.

Goal: identify and extract the remaining host-neutral command-pack metadata boundary without moving ReadOS-specific command implementations into Hosting.

Acceptance criteria:

- Hosting owns a generic command-pack descriptor that carries pack name, command instances, and command names.
- The composition builder accepts the descriptor and reports the host command-pack name.
- The composition reports core command overrides, including the app `workflow` override.
- App command construction remains in `ReadOS.App`; the app factory returns the Hosting descriptor around app-owned command instances.
- Existing `IEnumerable<IMspCommand>` composition callers still work through a default host-pack descriptor.

Progress:

- Added `MspCommandPack` under `ReadOS.Msp.Hosting/Runtime`.
- Extended `MspCommandHostComposition` with `HostCommandPackName` and `OverriddenCoreCommandNames`.
- Added a `MspCommandHostCompositionBuilder.Build(MspCommandPack)` overload while preserving the existing enumerable overload.
- Rewired `ReadOsMspCommandPackFactory` to return the host-neutral `MspCommandPack` instead of an app-specific descriptor type.
- Added Hosting tests for named command packs, default host-pack names, and core override reporting.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 35 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 266 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding the host-neutral command-pack descriptor.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: command-pack metadata is now a host-neutral Hosting contract, while ReadOS app command construction and document/PDF/chat adapters remain app-owned.

### T67: App-Owned MSP Host Dependencies

Status: done.

Goal: group `ReadOsMspHost` constructor dependencies behind a narrow app-owned dependency object without moving app adapters into Hosting.

Acceptance criteria:

- `ReadOsMspHost` has a primary constructor that accepts one app-owned dependency object.
- The dependency object carries workspace, PDF, chat, workspace/settings/document providers, attachment hooks, and chat result sinks.
- Missing app services are rejected early.
- `ShellViewModel` and tests create the host through the dependency object.
- ReadOS document/PDF/chat command adapters remain in `ReadOS.App`; Hosting remains limited to host-neutral composition/runtime contracts.

Progress:

- Added `ReadOsMspHostDependencies` under `ReadOS.App/Services/Msp`.
- Rewired `ReadOsMspHost` to consume `ReadOsMspHostDependencies` when creating the virtual workspace, command pack, policy, runtime host, and approved executor.
- Rewired `ShellViewModel` and App test helpers to construct the dependencies object explicitly.
- Added App tests for dependency-object host execution and missing service validation.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 268 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 35 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after grouping app host dependencies.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: the app host has a narrower constructor boundary, while all ReadOS-specific command adapters and UI state remain app-owned.

### T68: App MSP Host Runtime Factory

Status: done.

Goal: reduce `ReadOsMspHost` orchestration by moving app-specific runtime assembly into a separately tested app-owned factory.

Acceptance criteria:

- An app-owned factory composes the ReadOS virtual workspace, app command pack, Hosting command composition, request defaults, approval grants, app policy, and Hosting runtime host.
- `ReadOsMspHost` consumes the factory output instead of assembling workspace, command pack, policy, and runtime directly.
- The factory exposes enough metadata for tests to verify command-pack name, command names, core overrides, request defaults, and runtime execution.
- Document/PDF/chat commands and workbench state providers remain in `ReadOS.App`.
- Hosting remains limited to host-neutral command-pack, composition, approval, and runtime contracts.

Progress:

- Added `ReadOsMspHostRuntime` and `ReadOsMspHostRuntimeFactory` under `ReadOS.App/Services/Msp`.
- Moved virtual workspace, ReadOS command-pack, command composition, request factory, approval grant, operator policy, and runtime host assembly out of `ReadOsMspHost`.
- Rewired `ReadOsMspHost` to create an app runtime bundle and only attach the approved-command executor to itself.
- Updated App tests to compose the app command pack through the Hosting `MspCommandPack` descriptor and to verify runtime factory defaults, core workflow override metadata, runtime execution, and audit records.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 269 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 35 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding the app MSP host runtime factory.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: `ReadOsMspHost` is now mostly an execution facade over app-owned runtime assembly plus host-neutral Hosting services.

### T69: Hosting Runtime Command Host Adapter

Status: done.

Goal: move the remaining runtime-to-`IMspCommandHost` execution adapter out of the app facade and into `ReadOS.Msp.Hosting`.

Acceptance criteria:

- Hosting owns an `IMspCommandHost` adapter that delegates request execution and streaming execution to `MspRuntime`.
- `MspRuntimeHostFactory` returns a runtime host bundle with both the runtime and the command-host adapter.
- `ReadOsMspHostRuntimeFactory` passes the host-neutral command-host adapter to the app facade.
- `ReadOsMspHost` delegates request-level execution to the adapter and uses it for approved-command execution.
- Tests cover normal execution, streaming execution, audit recording, and the host bundle exposing the adapter.

Progress:

- Added `MspRuntimeCommandHost` under `ReadOS.Msp.Hosting/Runtime`.
- Extended `MspRuntimeHost` to expose `CommandHost`.
- Rewired `ReadOsMspHostRuntimeFactory` to return the command-host adapter instead of exposing raw `MspRuntime`.
- Rewired `ReadOsMspHost` request-level execution and approved-command orchestration to use the adapter.
- Added Hosting tests for runtime command-host execution and streaming behavior.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 37 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 269 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding the runtime command-host adapter.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: request-level command-host execution is now a host-neutral Hosting adapter, leaving `ReadOsMspHost` as a thin string-command facade for the workbench.

### T70: Hosting String Command Host Facade

Status: done.

Goal: move string-command defaults and approved string-command execution paths into a host-neutral Hosting facade.

Acceptance criteria:

- Hosting owns a facade that accepts command text plus optional actor and creates `MspCommandRequest` values through `MspCommandRequestFactory`.
- The facade delegates normal request execution and streaming execution to `IMspCommandHost`.
- The facade delegates approved normal and approved streaming execution through `MspApprovedCommandExecutor`.
- Default actor, default session, working directory, and one-shot approval environments remain covered by Hosting tests.
- `ReadOsMspHost` delegates all string-command overloads to the Hosting facade while keeping the request-level `IMspCommandHost` implementation.

Progress:

- Added `MspCommandHostFacade` under `ReadOS.Msp.Hosting/Runtime`.
- Added Hosting tests for string-command request defaults, actor override on streaming execution, approved environment injection, and approved streaming environment injection.
- Rewired `ReadOsMspHost` to use `MspCommandHostFacade` instead of directly owning `MspCommandRequestFactory` and `MspApprovedCommandExecutor`.
- Kept ReadOS app dependencies, command construction, virtual workspace, and workbench UI projection outside Hosting.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 41 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 269 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding the string-command host facade.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: string-command execution defaults are now host-neutral, and `ReadOsMspHost` is a thinner workbench-facing wrapper over Hosting command execution.

### T71: Hosting Command Host Diagnostics Projection

Status: done.

Goal: add a small host-neutral diagnostics projection for command-host observability without depending on ReadOS app models.

Acceptance criteria:

- Hosting owns a diagnostics model for request defaults, command-pack name, command counts, host command names, and core override names.
- Hosting owns a service that builds diagnostics from `MspCommandHostComposition` and `MspCommandRequestFactory`.
- Diagnostics report whether core command overrides are present.
- The App runtime factory carries the host-neutral diagnostics alongside the command host and composition metadata.
- Tests cover diagnostics for app-style workflow overrides and host-only command packs.

Progress:

- Added `MspCommandHostDiagnostics` and `MspCommandHostDiagnosticsService` under `ReadOS.Msp.Hosting/Runtime`.
- Added Hosting tests for request defaults, command counts, host command names, workflow override reporting, and no-override reporting.
- Rewired `ReadOsMspHostRuntimeFactory` to build and expose diagnostics on `ReadOsMspHostRuntime`.
- Extended App runtime factory coverage to assert the ReadOS command-host diagnostics projection.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 43 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 269 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 20 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding the command-host diagnostics projection.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: command-host observability now has a tested host-neutral projection that can be surfaced later without coupling Hosting to ReadOS UI models.

### T72: App Hosting Readiness Refresh

Status: done.

Goal: refresh the App-side Hosting readiness projection so it reflects the current extracted boundaries and records whether command-host diagnostics should be surfaced in the workbench.

Acceptance criteria:

- Readiness report marks already-extracted Hosting services as hosted instead of still ready-to-move.
- Readiness report keeps operator policy mode, ReadOS command construction, document/PDF/chat adapters, and workbench UI projection in App.
- Readiness report records a concrete command-host diagnostics visibility decision.
- Tests cover hosted boundaries, App-owned boundaries, and the diagnostics visibility decision.

Progress:

- Added a `Hosted` boundary status and a command-host diagnostics visibility decision model.
- Updated `ReadOsMspHostingReadinessService` to describe hosted session/projection, artifact/provenance, approval, cancellation, command-host composition, and diagnostics boundaries.
- Recorded that command-host diagnostics remain internal host metadata until startup/support/plugin-loading state becomes actionable in the workbench.
- Updated App tests for hosted boundaries, App-owned boundaries, and diagnostics visibility policy.
- Updated `DEVELOPMENT_PLAN.md` backlog and current-status notes to remove the open diagnostics visibility decision.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after refreshing the hosting readiness projection.

Result: the plan now has a tested App-owned projection of the current Hosting split and a clear decision to keep command-host diagnostics out of the operator UI for now.

### T73: Hosting Command Pack Validation

Status: done.

Goal: harden the host-neutral command-pack boundary so App-provided ReadOS commands enter Hosting through a validated, predictable command-pack contract.

Acceptance criteria:

- Command packs reject empty pack names.
- Command packs reject null command entries.
- Command packs reject empty command names.
- Command packs reject duplicate host command names case-insensitively.
- Command packs still allow a host command to intentionally override a core runtime command.
- Tests cover validation failures and the supported core override case.

Progress:

- Added command validation inside `MspCommandPack`.
- Added Hosting tests for trimmed pack names, order preservation, empty pack names, null commands, empty command names, duplicate command names, and core command overrides.
- Verified the existing ReadOS app command pack still passes through Hosting composition.
- Updated `DEVELOPMENT_PLAN.md` to track command-pack validation as a closed host-neutral boundary item.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 49 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding command-pack validation.

Result: Hosting now rejects malformed app command packs before registry composition while preserving the intentional ReadOS `workflow` core override path.

### T74: Hosting Command Name Trim Validation

Status: done.

Goal: tighten the host-neutral command-pack contract so host command names must already be normalized before registry composition.

Acceptance criteria:

- Command packs reject command names with leading or trailing whitespace.
- Command packs continue to preserve valid command order and names unchanged.
- Existing ReadOS app command packs still compose successfully under the stricter contract.
- Tests cover the new invalid command-name shape.

Progress:

- Added leading/trailing whitespace validation to `MspCommandPack`.
- Added a Hosting test for untrimmed command names.
- Verified App tests still pass, proving the ReadOS command pack uses normalized names.
- Updated `DEVELOPMENT_PLAN.md` to include untrimmed command-name validation in the command-pack boundary contract.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 50 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding command-name trim validation.

Result: malformed command names are now rejected at the Hosting boundary before they can become unreachable registry entries.

### T75: Hosting Command Name Whitespace Validation

Status: done.

Goal: reject command-pack command names that contain whitespace anywhere, keeping host commands aligned with MSP parser token semantics.

Acceptance criteria:

- Command packs reject command names with internal whitespace.
- Existing ReadOS app command packs still compose successfully under the stricter contract.
- Tests cover the new invalid command-name shape.
- Documentation records the tightened host-neutral command-pack contract.

Progress:

- Added full whitespace validation to `MspCommandPack` command-name checks.
- Added a Hosting test for an internal-whitespace command name.
- Verified App tests still pass, proving current ReadOS command names remain valid command tokens.
- Updated `DEVELOPMENT_PLAN.md` to include whitespace-containing command-name validation in the command-pack boundary contract.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 51 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding command-name whitespace validation.

Result: Hosting now accepts only command names that can be used as normal MSP command tokens without quoting or parser ambiguity.

### T76: Hosting Command Name Token Character Validation

Status: done.

Goal: restrict host command names to token-safe characters so command packs cannot register names that collide with MSP parser reserved syntax.

Acceptance criteria:

- Command packs allow ASCII letters, digits, hyphen, underscore, and dot in command names.
- Command packs reject command names that include reserved shell/parser characters such as `|`.
- Existing ReadOS app command packs still compose successfully under the stricter contract.
- Tests cover accepted token-safe names and rejected unsupported characters.

Progress:

- Added token-safe character validation to `MspCommandPack`.
- Added Hosting tests for hyphen, underscore, dot, and reserved-character command names.
- Verified App tests still pass with the current ReadOS command pack.
- Updated `DEVELOPMENT_PLAN.md` to include token-unsafe command-name validation in the command-pack boundary contract.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 53 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding command-name token character validation.

Result: Hosting command packs now only admit command names that are safe to invoke as normal MSP command tokens.

### T77: Hosting Command Composition Input Validation

Status: done.

Goal: make command-host composition fail with explicit host-neutral contract errors when builders receive invalid registry or command-pack inputs.

Acceptance criteria:

- `MspCommandHostCompositionBuilder` rejects a null core registry factory.
- `Build(MspCommandPack)` rejects a null command pack.
- Composition rejects a core registry factory that returns null.
- Existing ReadOS app runtime composition still works under the stricter checks.
- Tests cover each invalid input path.

Progress:

- Added constructor and build-time argument validation to `MspCommandHostCompositionBuilder`.
- Added an explicit invalid-operation failure for a null core registry returned by a custom factory.
- Added Hosting tests for null factory, null command pack, and null factory result.
- Verified App tests still pass, proving the ReadOS runtime factory composition remains valid.
- Updated `DEVELOPMENT_PLAN.md` to track composition input validation as a closed host-neutral boundary item.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 56 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding command composition input validation.

Result: command-host composition now reports invalid host inputs directly instead of surfacing indirect null-reference failures.

### T78: Hosting Core Registry Command Name Validation

Status: done.

Goal: apply the same token-safe command-name contract to custom core registries used by command-host composition.

Acceptance criteria:

- Command packs and custom core registries share one host-neutral command-name validation rule.
- Composition rejects core registry commands with empty names.
- Composition rejects core registry commands with unsupported token characters.
- Existing ReadOS app runtime composition still works under the stricter checks.
- Tests cover invalid custom core registry command names.

Progress:

- Added internal `MspCommandNameValidation` to centralize token-safe command-name checks.
- Rewired `MspCommandPack` to use the shared validation helper while preserving existing argument error behavior.
- Rewired `MspCommandHostCompositionBuilder` to validate custom core registry command names before override detection and host command registration.
- Added Hosting tests for empty and unsupported-character core registry command names.
- Verified App tests still pass, proving the default ReadOS runtime composition remains valid.
- Updated `DEVELOPMENT_PLAN.md` to track shared command-name validation across command packs and custom core registries.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 58 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding core registry command-name validation.

Result: Hosting composition now rejects invalid custom core registry command names before diagnostics, override detection, or host registration depend on them.

### T79: Hosting Runtime Host Audit Sink Validation

Status: done.

Goal: make runtime host assembly fail with a clear host-neutral contract error when the default audit sink factory is invalid.

Acceptance criteria:

- `MspRuntimeHostFactory` rejects a null audit sink factory.
- `Build` rejects a default audit sink factory that returns null.
- Existing ReadOS app runtime host assembly still works under the stricter checks.
- Tests cover constructor validation, default audit creation, and null factory output.

Progress:

- Added explicit null-result validation around `MspRuntimeHostFactory` default audit sink creation.
- Added Hosting tests for null audit sink factory input and null audit sink factory output.
- Verified App tests still pass, proving the ReadOS runtime factory keeps supplying a valid audit sink path.
- Updated `DEVELOPMENT_PLAN.md` to track audit sink factory validation as a closed host-neutral runtime host item.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 60 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding runtime host audit sink validation.

Result: runtime host assembly now reports invalid audit sink factory behavior directly before constructing an unusable command context.

### T80: Hosting Command Request Metadata Normalization

Status: done.

Goal: normalize host-level command request metadata before requests reach policy, audit, diagnostics, or runtime execution.

Acceptance criteria:

- `MspCommandRequestFactory` trims default session ID, actor, and working directory values.
- Empty default metadata still falls back to `default`, `agent`, and `/`.
- Per-request actor, session ID, and working directory overrides are trimmed.
- Empty per-request metadata overrides still fall back to defaults.
- Command text remains unchanged and continues to be parsed by the runtime.
- Tests cover default normalization, fallback behavior, and override normalization.

Progress:

- Updated `MspCommandRequestFactory` to trim default actor/session/working-directory metadata.
- Updated request creation to trim per-request actor/session/working-directory overrides while preserving existing fallback behavior.
- Added Hosting tests for trimmed defaults, empty defaults, trimmed overrides, and empty override fallback.
- Verified App tests still pass, proving ReadOS host execution remains compatible with normalized request metadata.
- Updated `DEVELOPMENT_PLAN.md` to track request metadata normalization as a closed host-neutral runtime item.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 64 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding command request metadata normalization.

Result: Hosting now prevents accidental whitespace in actor, session, and working-directory metadata from leaking into policy, audit, diagnostics, and runtime state.

### T81: Hosting Approved Command Actor Normalization

Status: done.

Goal: keep approved-command grants and runtime requests aligned after request metadata normalization.

Acceptance criteria:

- `MspApprovedCommandExecutor` trims actor overrides before approving the next command.
- The generated approved request uses the same normalized actor as the approval grant.
- Approved execution with a padded actor can still consume the one-shot approval token.
- `MspApprovedCommandExecutor` rejects null constructor dependencies.
- Existing ReadOS approved-command tests continue to pass.

Progress:

- Added dependency null checks to `MspApprovedCommandExecutor`.
- Added actor normalization before approval grant creation for normal and streaming approved execution.
- Added a Hosting test with a policy-consuming command host to prove padded actors still match one-shot approval grants after request creation.
- Added Hosting constructor guard coverage for executor dependencies.
- Verified App tests still pass, proving ReadOS approved command execution remains compatible.
- Updated `DEVELOPMENT_PLAN.md` to track approved-command actor normalization as a closed host-neutral runtime item.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 66 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding approved-command actor normalization.

Result: approved command grants now use the same normalized actor metadata as the requests that policy later evaluates.

### T82: Hosting Approval Grant Metadata Normalization

Status: done.

Goal: normalize approval-grant metadata at the grant store boundary so direct grant-store callers match the request metadata rules used by Hosting command execution.

Acceptance criteria:

- `MspApprovalGrantStore` trims configured approval environment keys.
- Approval grants normalize padded actor names before storing grant metadata.
- Approval consumption normalizes request actor metadata before matching.
- Empty actor metadata uses the same `agent` fallback as command request creation.
- Existing ReadOS operator policy and approved-command tests continue to pass.

Progress:

- Updated `MspApprovalGrantStore` to trim custom environment keys.
- Updated approval grant creation and consumption to normalize actor metadata.
- Added Hosting policy tests for trimmed environment keys, padded grant/request actor matching, and empty actor fallback.
- Verified App tests still pass, proving ReadOS operator policy remains compatible.
- Updated `DEVELOPMENT_PLAN.md` to track approval-grant metadata normalization as a closed host-neutral policy item.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 69 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding approval-grant metadata normalization.

Result: approval grants now use normalized actor and environment-key metadata whether called through the approved-command executor or directly through the grant store.

### T83: Hosting Approval Token Normalization

Status: done.

Goal: normalize approval token metadata at the grant store boundary so environment creation, policy consumption, and revocation behave consistently for direct grant-store callers.

Acceptance criteria:

- `MspApprovalGrantStore` trims approval tokens before adding them to generated environments.
- Approval consumption trims environment token values before lookup.
- Approval revocation trims token values before removing grants.
- Empty or whitespace-only environment tokens do not consume stored grants.
- Existing ReadOS operator policy and approved-command tests continue to pass.

Progress:

- Added token normalization inside `MspApprovalGrantStore`.
- Updated approval environment creation, revocation, and consumption to use normalized tokens.
- Added Hosting policy tests for generated environment token trimming, padded environment-token consumption, padded revocation, and empty-token non-consumption.
- Updated `DEVELOPMENT_PLAN.md` to track approval token normalization as a closed host-neutral policy item.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 73 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding approval token normalization.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: approval tokens now use normalized values for generated environments, direct environment consumption, and revocation without letting empty token values consume stored grants.

### T84: Hosting Command Host Facade Contract Coverage

Status: done.

Goal: lock the host-neutral string-command facade contract so app callers can rely on constructor guards and request metadata normalization without testing through ReadOS-specific host adapters.

Acceptance criteria:

- `MspCommandHostFacade` rejects null command-host, request-factory, and approval-grant dependencies.
- Normal facade execution trims padded actor overrides before request dispatch.
- Streaming facade execution falls back to the default actor for empty actor overrides.
- Approved facade execution uses normalized actor metadata for both request creation and one-shot approval consumption.
- Existing ReadOS App host tests continue to pass.

Progress:

- Added Hosting facade constructor guard tests.
- Added normal and streaming facade tests for actor override normalization and default actor fallback.
- Added approved facade coverage for padded actor overrides and approval grant consumption alignment.
- Updated `DEVELOPMENT_PLAN.md` to track command-host facade contract coverage as a closed host-neutral runtime item.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 77 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding command-host facade contract coverage.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: the host-neutral command-host facade contract is now covered for dependency guards, normal/streaming actor normalization, and approved execution actor alignment.

### T85: Hosting Runtime Command Host Request Guards

Status: done.

Goal: make the host-neutral runtime command-host adapter fail at the adapter boundary for invalid runtime/request inputs instead of leaking null failures from the MSP runtime internals.

Acceptance criteria:

- `MspRuntimeCommandHost` rejects a null runtime dependency.
- Normal execution rejects a null `MspCommandRequest` with `ArgumentNullException`.
- Streaming execution rejects a null `MspCommandRequest` before enumeration with `ArgumentNullException`.
- Existing normal and streaming delegation behavior stays unchanged.
- Existing ReadOS App host tests continue to pass.

Progress:

- Added explicit request null guards to `MspRuntimeCommandHost.ExecuteAsync`.
- Added explicit request null guards to `MspRuntimeCommandHost.ExecuteStreamingAsync`.
- Added Hosting adapter tests for null runtime, normal request, and streaming request guard behavior.
- Updated `DEVELOPMENT_PLAN.md` to track runtime command-host adapter request guards as a closed host-neutral runtime item.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 80 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding runtime command-host adapter request guards.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: the runtime command-host adapter now rejects invalid runtime/request inputs at the Hosting boundary while preserving normal and streaming runtime delegation.

### T86: Hosting Runtime Host Constructor Guards

Status: done.

Goal: make the host-neutral runtime host container reject invalid dependencies before they are stored or used to build command-host adapters.

Acceptance criteria:

- `MspRuntimeHost` rejects a null context when using the default command-host constructor.
- `MspRuntimeHost` rejects a null runtime when using the default command-host constructor.
- `MspRuntimeHost` rejects null context, runtime, and command-host dependencies when all dependencies are supplied directly.
- Valid constructors still preserve supplied context/runtime/command-host instances.
- Existing ReadOS App host tests continue to pass.

Progress:

- Added constructor guards to `MspRuntimeHost`.
- Added Hosting runtime-host tests for default command-host creation and supplied command-host preservation.
- Added Hosting runtime-host tests for null context, runtime, and command-host dependencies.
- Updated `DEVELOPMENT_PLAN.md` to track runtime host constructor guards as a closed host-neutral runtime item.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 85 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors after rerunning past a transient WinUI generated-file lock.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding runtime host constructor guards.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: the runtime host container now rejects invalid constructor dependencies while preserving valid context/runtime/command-host composition.

### T87: Hosting Runtime Host Factory Working Directory Normalization

Status: done.

Goal: normalize runtime host factory working-directory inputs before context creation so host callers get the same trimmed/default directory behavior as other Hosting request metadata boundaries.

Acceptance criteria:

- `MspRuntimeHostFactory.Build` trims padded working-directory input before passing it to `MspCommandContext`.
- Empty or whitespace-only working-directory input falls back to `/`.
- Context path normalization still resolves relative segments such as `..`.
- The factory rejects null workspace, registry, and policy dependencies with stable parameter names.
- Explicit audit sinks are used without invoking the default audit sink factory.
- Existing ReadOS App host tests continue to pass.

Progress:

- Added working-directory normalization in `MspRuntimeHostFactory.Build`.
- Added Hosting factory tests for padded working-directory normalization and empty working-directory fallback.
- Added Hosting factory tests for required dependency guards.
- Added Hosting factory coverage proving explicit audit sinks bypass the default audit sink factory.
- Updated `DEVELOPMENT_PLAN.md` to track runtime host factory working-directory normalization as a closed host-neutral runtime item.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 89 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding runtime host factory working-directory normalization.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: runtime host factory inputs now use trimmed/default working-directory metadata before context path normalization, with required dependency and audit selection contracts covered.

### T88: Core MSP Command Context Dependency Guards

Status: done.

Goal: make the runtime-neutral command context reject invalid required dependencies at construction time before commands, policy checks, or audit recording can fail deeper in execution.

Acceptance criteria:

- `MspCommandContext` rejects a null workspace with `ArgumentNullException`.
- `MspCommandContext` rejects a null command registry with `ArgumentNullException`.
- `MspCommandContext` rejects a null policy with `ArgumentNullException`.
- `MspCommandContext` rejects a null audit sink with `ArgumentNullException`.
- Valid context construction still preserves supplied workspace, registry, policy, audit, services, working directory, and default invocation.
- Existing Hosting and ReadOS App tests continue to pass.

Progress:

- Added constructor guards to `MspCommandContext` for workspace, registry, policy, and audit.
- Added core MSP runtime tests for valid context component preservation.
- Added core MSP runtime tests for required dependency guard parameter names.
- Updated `DEVELOPMENT_PLAN.md` to track command context dependency guards as a closed runtime-neutral item.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 22 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 89 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding command context dependency guards.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: command contexts now reject missing required runtime components at construction while preserving valid runtime-neutral composition.

### T89: Core MSP Command Registry Validation

Status: done.

Goal: make the runtime-neutral command registry reject invalid command registrations and empty lookups at the registry boundary before runtime composition or command execution.

Acceptance criteria:

- `MspCommandRegistry.Register` rejects null commands with `ArgumentNullException`.
- `MspCommandRegistry.Register` rejects empty, untrimmed, whitespace-containing, and unsupported-character command names.
- Valid command names using letters, digits, `.`, `_`, and `-` continue to register.
- Command lookup remains case-insensitive.
- Empty or whitespace-only lookup names return false without throwing.
- Hosting command composition tests continue to pass with core registry validation propagated from the lower layer.

Progress:

- Added command registration validation to `MspCommandRegistry`.
- Added empty lookup handling to `MspCommandRegistry.TryGet`.
- Added core MSP registry tests for valid registration, case-insensitive lookup, null commands, invalid command names, and empty lookup names.
- Updated Hosting composition builder tests to expect invalid core registry registration to fail at the core registry boundary.
- Updated `DEVELOPMENT_PLAN.md` to track command registry validation as a closed runtime-neutral item.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 31 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 89 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding command registry validation.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: command registry registration now validates commands and names at the runtime-neutral boundary while preserving valid lookup and composition behavior.

### T90: Core MSP Runtime Request Guards

Status: done.

Goal: make the runtime-neutral MSP runtime reject invalid context and request inputs at the runtime boundary before parsing, streaming, policy, or audit code can fail deeper in execution.

Acceptance criteria:

- `MspRuntime` rejects a null command context with `ArgumentNullException`.
- `MspRuntime.ExecuteAsync` rejects a null `MspCommandRequest` with `ArgumentNullException`.
- `MspRuntime.ExecuteStreamingAsync` rejects a null `MspCommandRequest` before enumeration.
- Existing normal execution behavior remains unchanged.
- Existing streaming execution behavior remains unchanged.
- Hosting and ReadOS App tests continue to pass.

Progress:

- Added a constructor guard to `MspRuntime`.
- Added a normal execution request guard to `MspRuntime.ExecuteAsync`.
- Split streaming execution into a guarded public method and private async iterator so null requests fail before enumeration.
- Added core MSP runtime tests for null context, null normal request, and null streaming request behavior.
- Updated `DEVELOPMENT_PLAN.md` to track runtime constructor and request guards as a closed runtime-neutral item.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 34 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 89 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding runtime constructor and request guards.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: the MSP runtime now rejects invalid context and request inputs at the runtime boundary while preserving normal and streaming execution behavior.

### T91: Core MSP Command Context Working Directory Normalization

Status: done.

Goal: normalize command-context working-directory metadata at the runtime-neutral context boundary so direct context construction and runtime request updates match Hosting request/factory normalization rules.

Acceptance criteria:

- `MspCommandContext` trims padded working-directory metadata before workspace path normalization.
- Empty or whitespace-only working-directory metadata falls back to `/`.
- Relative segments such as `..` still resolve through the workspace path normalizer.
- `WithWorkingDirectory` applies the same normalization rules as direct construction.
- Existing Hosting and ReadOS App tests continue to pass.

Progress:

- Added working-directory normalization inside `MspCommandContext`.
- Added core MSP context tests for padded working-directory normalization.
- Added core MSP context tests for empty working-directory fallback.
- Added core MSP context tests proving `WithWorkingDirectory` uses the same normalization.
- Updated `DEVELOPMENT_PLAN.md` to track command-context working-directory normalization as a closed runtime-neutral item.

Verification:

- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 37 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 89 tests.
- 2026-07-07: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 271 tests.
- 2026-07-07: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors.
- 2026-07-07: `.\scripts\verify-msp.ps1` passed after adding command-context working-directory normalization.
- 2026-07-07: `git diff --check` passed with only LF/CRLF warnings.

Result: command contexts now normalize working-directory metadata consistently for direct construction and runtime context updates.

## Current Workstream Details (T92-T95)

### T92: Trustworthy MSP Boundary

Status: done.

Goal: close the P0 trust gaps that could let MSP escape an app-owned namespace, lose terminal command evidence, report a false-green verification result, or persist provider secrets as ordinary workspace data.

Acceptance criteria:

- Artifact `list/show/write/rename/delete` resolves paths through a shared normalized namespace check and cannot address anything outside `/artifacts`.
- Workflow session/transcript/source-artifact reads validate record identifiers and namespace membership before constructing or following virtual paths.
- Regression coverage includes `..`, backslashes, rooted paths, Windows drive prefixes, namespace prefix siblings, manifest aliases, rename/delete, malicious session records, and malicious transcript IDs; normalization is centralized before namespace membership is checked.
- Success, parse failure, unknown command, policy confirmation/denial, policy exception, command exception, cancellation during policy/command execution, and cancellation before parsing all return a terminal result with persistent audit evidence.
- Pre-policy failures use an explicit `NotEvaluated` policy decision instead of implying authorization.
- Verification and packaging stop on the first failing external process; packaging restores from a clean state and runs all three managed test projects unless tests are explicitly skipped.
- A pinned .NET SDK and Windows CI execute the same full verifier used locally.
- Provider API keys are excluded from workspace JSON and workspace exports, protected per provider with Windows DPAPI `CurrentUser`, migrated safely from legacy plaintext state, and optional at startup without leaking into a provider request.

Implemented:

- Added `MspNamespacePathUtility` for normalized root/descendant resolution and token-safe record path construction.
- Applied the shared boundary to artifact content/manifest operations and workflow session, transcript, and upstream artifact-manifest lookups.
- Added artifact and workflow path-security suites covering escape attempts and namespace-confused persisted records.
- Completed terminal runtime audit construction for parse errors, unknown commands, confirmation/denial, policy failures, command failures, and cancellation while preserving stable diagnostics and exit codes.
- Added `MspPolicyDecision.NotEvaluated` for attempts that never reached an authorization decision.
- Made `verify-msp.ps1` and `package-windows.ps1` check every native process exit code and throw immediately on failure.
- Made packaging restore the solution and runtime-specific app assets, run core/Hosting/App tests, and propagate publish failures.
- Added `global.json` and `.github/workflows/windows-ci.yml`; CI invokes `verify-msp.ps1` on Windows with the pinned SDK and Rust toolchain.
- Added `IProviderCredentialStore` and `WindowsDpapiProviderCredentialStore`, removed API key serialization, migrated legacy plaintext only after protected persistence succeeds, and excluded the external credential directory from workspace exports.
- Added missing-credential chat behavior and credential persistence/migration/export tests without writing secrets to test output.

Verification:

- 2026-07-10: `.\scripts\verify-msp.ps1` passed end to end.
- 2026-07-10: Rust tests passed with 5 tests; native FFI smoke passed.
- 2026-07-10: `dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj` passed with 67 tests.
- 2026-07-10: `dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj` passed with 89 tests.
- 2026-07-10: `dotnet test .\tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj` passed with 286 tests.
- 2026-07-10: `dotnet build .\ReadOS.sln` passed with 0 warnings and 0 errors through the verifier.
- Static release-gate evidence: `.github/workflows/windows-ci.yml` calls the full verifier using `global.json`; `package-windows.ps1` restores, runs all three managed suites, and fails on nonzero restore/test/publish exit codes. This record does not claim a remote GitHub Actions run or a package smoke run that is not shown above.

Result: the current MSP boundary is namespace-confined, terminally auditable, locally verifiable without false-green process handling, CI-ready, and no longer treats provider API keys as workspace/export data.

### T93: Product-Level End-to-End Acceptance

Status: in progress.

Goal: prove one repeatable user-visible ReadOS workflow across real service boundaries rather than inferring product readiness from isolated command and service tests.

Flagship scenario:

```text
import PDF
-> select outline section
-> workflow run extract-evidence
-> approve artifact write
-> workflow run synthesize-evidence
-> persist session/transcript/artifact/manifest
-> restart and restore workspace
-> follow artifact lineage to the original document pages
```

Acceptance criteria:

- A deterministic fixture or test document drives the full scenario without hidden UI-only state.
- The generated evidence and synthesis artifacts retain document ID, page range, virtual page paths, source artifact, session, transcript, actor, command, and manifest provenance as applicable.
- Restart creates new app/service objects and reloads durable state; an in-memory refresh is not sufficient evidence.
- Lineage navigation reopens the correct source document and page/range after restart.
- The same harness covers cancellation, invalid pages, provider failure, denied approval, and a recoverable retry without partial writes or secret leakage.
- Service/integration automation is primary; a minimal WinUI smoke and an operator runbook cover the shell interactions that cannot be proven below UI level.
- The full verifier and a Windows package smoke pass after the scenario is added.

Progress and evidence:

- `ReadOsFlagshipWorkflowIntegrationTests.Flagship_workflow_survives_restart_and_traces_synthesis_back_to_imported_pdf_page` imports a deterministic PDF fixture through `WorkspaceStore` and selects a controlled outline section.
- The test persists an approval-required extraction request, constructs fresh store/host/service objects, restores the pending approval, approves extraction, and writes page-provenance evidence.
- It approval-gates and executes evidence synthesis, persists artifact/manifest/session/transcript state, denies a refinement without a partial artifact/model call, restarts again, and follows synthesis -> evidence -> original virtual PDF page -> page navigation.
- `Canceled_extraction_persists_one_terminal_record_without_partial_artifact` proves exit 130, one terminal transcript/session record, a stable `msp.canceled` diagnostic/audit record, restart persistence, and no partial artifact.
- `Invalid_outline_page_returns_stable_diagnostic_without_artifact` proves `reados.pdf.invalid_page`, terminal audit/session evidence, restart persistence, and no artifact for the invalid range.
- `Provider_failure_is_secret_safe_and_retry_after_restart_preserves_lineage_and_audit` proves provider failure does not expose the API key or sensitive request through stderr, diagnostics, audit, transcript, or workspace; restart restores the failed attempt, and a successful retry preserves lineage and both terminal records.
- Provider exception bodies are withheld from diagnostics, and cancellation projects an explicit terminal canceled message.
- The 2026-07-11 App suite passes with all four flagship integration scenarios plus the package-smoke isolation/cleanup, native-proxy, and ABI-runtime-info regressions; the verified baseline is Rust 66, Core 69, Hosting 259, App 300 (628 managed total).
- The earlier managed staged startup smoke still proves isolated WorkspaceStore persistence, DPAPI round trip/cleanup, Hosting composition, and `workspace info`, but it does not satisfy the newer native WorkspaceFS package gate.
- The corrected native package smoke uses a separate fixed-NTFS temporary root, runs packaged Rust `ls /` and binary `cat /workspace.json`, redacts the native path, and removes the native run directory in `finally`. The full no-skip package gate and direct smoke rerun pass; native residue is zero.
- Visible WinUI interaction is now covered: the operator runbook (`docs/OPERATOR_RUNBOOK.md`) documents the shell path import -> outline -> extract-evidence -> approve -> synthesize-evidence -> artifacts/lineage -> restart, and `ReadOS.App.Tests` adds a minimal WinUI smoke (`ShellViewModelTests.Flagship_evidence_and_synthesis_workflows_are_visible_through_shell`) that drives the flagship extract/synthesize-evidence path through `ShellViewModel` (prepare draft -> run -> approve from the shell -> visible transcript + artifact) with controlled PDF/chat doubles, no real provider.
- A real provider/network boundary and the full flagship workflow across a packaged-process restart remain open (CI/packaging); therefore T93 is not done.

### T94: Workbench Hardening

Status: in progress.

Goal: turn the implemented three-column shell, typed timeline, inspector, and Runtime Drawer into a resilient workbench across compact layouts and overlapping asynchronous document/runtime activity.

Current baseline:

- Typed timeline records and inspector routing are implemented.
- The Runtime Drawer is implemented with transcript rows, output previews, progress/status, approval/deny/cancel actions, open/close, and in-memory pin state.
- Responsive breakpoints collapse side panes, and `PresenterSurfaceView` is marked deprecated but still exists.

Acceptance criteria:

- Entering and leaving compact mode preserves the operator's sidebar and inspector widths instead of overwriting them with zero-width computed state.
- Compact mode provides overlay/flyout access to the sidebar and inspector while keeping the thread/composer usable.
- The Runtime Drawer supports constrained vertical resize, persists height and pin state, selects a command, and shows full stdout, stderr, diagnostics, recovery hints, effects, policy decision, timing, and artifacts for that command.
- Document, page, outline/search, presenter, and thumbnail loads use cancellation plus latest-request-wins so stale completions cannot replace newer selections.
- Deprecated `PresenterSurfaceView` and obsolete presenter-primary/layout state are removed after the inspector preview path is proven equivalent.
- `ShellViewModel` and large command services are split only along exercised feature boundaries with focused tests; line-count-only refactors do not satisfy the item.
- Managed tests, solution build, responsive manual checks, and screenshots or a short recording verify compact/medium/wide behavior and runtime supervision.

Progress and evidence:

- `LayoutService` now remembers bounded sidebar/inspector preferences independently of the zero-width compact projection, and `MainWindow` no longer writes computed compact widths back into the ViewModel.
- Compact thread-header controls open app-bound `WorkspaceSidebarView` and `InspectorView` flyouts and dismiss them when layout/visibility changes.
- `LayoutServiceTests` cover compact/medium/wide restoration, visibility toggles, and width clamping; the 2026-07-10 App suite and full verifier pass with these tests included.
- The InspectorView Run-tab selected-command card now shows full supervision detail for the selected transcript: scrollable read-only stdout/stderr, diagnostics (danger brush), recovery hint (warning brush), effects, policy preview, artifacts, exit code, and start→complete timing — all projected from `MspTranscriptEntry`. The model gained `HasEffects` (excludes the default `None`), `TimingLabel`, and `ExitCodeLabel` projections; `ReadOS.App` builds clean and `tests/ReadOS.App.Tests/Models/MspTranscriptEntryTests.cs` covers them.
- Removed the deprecated `PresenterSurfaceView` (`Views/PresenterSurfaceView.xaml` + `.xaml.cs`) and its `<Page>` reference in `ReadOS.App.csproj`; `ReadOS.App` and `ReadOS.App.Tests` compile clean. The bound `ShellViewModel` state (thumbnails, page/outline/search, presenter text/region, layout commands) is still consumed by the InspectorView Preview tab, so presenter-primary/layout VM state is retained pending a separate confirmation pass.
- Document, page, outline/search, presenter-text, and thumbnail loads in `ShellViewModel` now use per-category generation counters plus a `CancellationTokenSource` so a stale completion cannot overwrite a newer selection, and the in-flight request is canceled through the already token-aware `IPdfDocumentService`. `ReadOS.App` builds clean; automated coverage of the race is still pending CI/verify.
- Runtime Drawer resize/persistence/pin, presenter-primary/layout VM state cleanup, and final responsive/manual evidence remain open; therefore T94 is not done.

### T95: Upstream-Aligned Windows MSP Runtime-Neutral Core

Status: in progress.

Goal: use upstream MSP design and behavior evidence to build the general Windows-compatible runtime-neutral core in Rust under `native/msp-core`, then connect it to ReadOS through a stable .NET adapter while keeping ReadOS document, PDF, chat, workflow, credential, persistence, and WinUI behavior in C# app/domain layers.

Upstream reference boundary:

- The root `MSP/` directory is a local Apache-2.0 reference repository, not a ReadOS runtime dependency or package payload.
- Primary inputs are `MSP/Spec`, `MSP/Conformance`, and the Swift `MSPCore`, `MSPShell`, and `MSPPOSIXCore` implementations.
- `MSP/Implementations/Windows` currently contains only `.gitkeep`; it supplies no Windows implementation to ship or wrap.
- T95 must derive Windows behavior from the upstream contracts and evidence; it must not redefine MSP as a ReadOS-only minimal JSON protocol.
- Any copied or derived upstream source, fixture, or documentation must record its source revision/snapshot, modifications, Apache-2.0 license, NOTICE obligations, and downstream provenance.
- The raw `MSP/` tree must be excluded from ReadOS package output. Only required ReadOS-built native binaries, adapter assemblies, and applicable notice/provenance files may ship.

Acceptance criteria:

- A versioned upstream-input inventory maps selected Spec sections, profiles, conformance fixtures, and Swift reference components to Rust modules/tests and records `reference`, `adapt`, `defer`, or explicit `Windows deviation` decisions.
- `native/msp-core` implements the selected runtime-neutral WorkspaceFS, command/result/stream, shell parsing/execution, policy, audit, diagnostics, and command-profile semantics without exposing raw host paths or arbitrary system shell access.
- A per-feature and per-command compatibility matrix reports `conformant`, `partial`, `deferred`, or `not applicable`, with evidence links and Windows-specific rationale.
- Applicable upstream conformance fixtures run against the Rust core. ReadOS-specific fixtures may supplement them but cannot replace upstream behavior evidence.
- A stable .NET adapter owns request, streaming, cancellation, result, policy/audit, workspace, and artifact interop; ReadOS business services never depend directly on raw FFI details.
- Differential tests compare the existing managed runtime and Rust core for the adopted surface until the adapter becomes authoritative.
- C# remains the ReadOS business layer: PDF, documents, chat/provider, workflows, workspace persistence, credentials, and WinUI do not move into the generic Rust core.
- Secrets and app-private data remain excluded from adapter diagnostics, conformance inputs, audit examples, and native logs.
- Packaging and CI prove that `MSP/` is not distributed, required NOTICE/provenance accompanies copied or derived material, and Rust/adapter/conformance tests run with the existing release gates.

Phased compatibility matrix and acceptance gates:

| Stage | Scope | Gate to advance |
| --- | --- | --- |
| 0. Reference and license inventory | Spec/Profiles, Conformance fixtures, Swift MSPCore/Shell/POSIXCore, source revision, Apache-2.0/NOTICE, Windows placeholder | Reviewed mapping and provenance ledger exists; raw `MSP/` package exclusion is testable. |
| 1. Rust MSPCore parity | WorkspaceFS paths, command registry, results/streams, policy, audit, diagnostics | Selected MSPCore cases pass in Rust; host paths/secrets do not leak; deviations are recorded. |
| 2. Shell and POSIXCore compatibility | MSPShell parsing/execution and explicit MSPPOSIXCore subset adapted for Windows/virtual WorkspaceFS | Feature/command matrix is complete for the selected subset and applicable upstream fixtures pass in CI. |
| 3. Stable .NET adapter | Managed/native lifecycle, request, streaming, cancellation, workspace, policy/audit, results/artifacts | Adapter contract and C#-vs-Rust differential tests pass; no app service calls raw FFI. |
| 4. ReadOS adoption and release | Existing ReadOS command/workflow paths use the adapter | T92 safety and T93 workflow evidence pass through Rust; package contains required binaries/notices but no raw `MSP/` tree. |

Current evidence:

- The local reference repository exposes the required Spec, Conformance, and Swift reference surfaces and declares Apache-2.0 in `MSP/LICENSE` with project notices in `MSP/NOTICE`.
- `MSP/Implementations/Windows` is confirmed to be only `.gitkeep`, establishing that `native/msp-core` is the Windows implementation workstream rather than a wrapper around existing Windows code.
- `native/msp-core` now has versioned internal request/result/diagnostic/policy/audit contracts, byte-authoritative stdout/stderr with Base64 JSON transport, a serializable shell AST, selected built-ins behind validated deterministic `Command`/`Registry`/`CommandPack` composition, a backend-neutral `VirtualPath`, panic-contained legacy entry points, and a negotiated length-delimited ABI v2.
- The Windows read-only WorkspaceFS retains root/target handles, checks final-handle containment with case-insensitive component boundaries, rejects UNC/mapped/removable/non-NTFS/drive/ADS/device injection, hides `.msp` again after final-path resolution, and implements handle-based `stat`, stable listing, and binary range reads. Rust `ls` and binary-safe `cat` exercise this backend.
- Rust output sanitization covers DOS, verbatim, slash, JSON-escaped, file-URL, and percent-encoded workspace-root forms across chunk boundaries; output and directory resources are bounded. Native UTF-16LE process streams remain deferred until external processes exist.
- `ReadOS.Msp.Hosting.Native` implements the internal `reados-msp-native/1` adapter with safe DLL loading, strict UTF-8/JSON/Base64 decoding, deep-frozen contracts, operation-specific request/response limits, contract/audit/AST validation, host-path disclosure checks including UTF-16LE variants, and exception text that omits stack/build-machine paths.
- `native_pwd_echo_adoption_v1` is complete. The ReadOS product registry protects `pwd` and `echo` from host-pack overrides with ordinal-ignore-case validation and replaces their execution bodies with one shared lazy native adapter. Only exact canonical lowercase command names route to Rust; case variants fail closed without managed fallback. `ls`, `cat`, `help`, and all app-domain commands remain managed.
- Managed `MspRuntime` remains authoritative for parse, policy/approval, dry-run, streaming events, terminal results, cancellation projection, and exactly-once product audit. Native Parse must agree with the original command text, command name, explicit-empty-aware arguments, and a single simple AST before execute; pipelines, lists, redirections, assignments, negation, raw newlines, and unquoted ampersands fail closed. Native execution must return exactly one matching `Allow` audit and no state change; its audit is evidence only and is not appended to the managed result. The native request receives no approval environment, credentials, or workspace root.
- `native_abi_v2_handshake_v1` is complete. The fixed 32-byte ABI info reports major `2`, minor `0`, contract `0x324D534F44414552`, and capabilities `0x1F` (required `0xF`); v2 request/response ownership is explicit pointer/`ulong` length and embedded NUL is not truncated. Hosting permits full v1 fallback only when `msp_get_abi_info_v2`, `msp_invoke_v2`, and `msp_free_buffer_v2` are all absent. Partial availability, size/version/contract/capability drift, or malformed results fail closed. V1/v2 allocations and free functions never mix, every native-return path frees exactly once, and invoke/dispose share one lock. Runtime ABI information is exposed for staged-package evidence.
- Parse/Execute/Normalize v2 request caps are 128 KiB/1 MiB/1 MiB and response caps are 16 MiB/64 MiB/1 MiB. Rust's bounded writer refuses growth before reserve/copy, eliminating the measured 54.5x Parse response-amplification path while keeping contract-shaped bounded failures.
- Release builds use the static MSVC CRT. Native binary verification checks all seven required exports and rejects dynamic CRT markers; the current release DLL depends only on Windows system libraries.
- The committed Apache-2.0 conformance snapshot records upstream revision and fixture paths for `:`, `echo`, `false`, `pwd`, `true`, and `echo -e`; clean checkout tests no longer require the nested `MSP/` clone.
- [MSP_UPSTREAM_COMPATIBILITY_MATRIX.md](MSP_UPSTREAM_COMPATIBILITY_MATRIX.md) records the current adopted surface, Windows deviations, managed status, and next acceptance gate.
- Rust fmt/test/clippy/release build, native binary/FFI smoke, 69 Core tests, 318 Hosting tests (including real release-DLL operations without skip), 411 App tests, and the zero-warning/zero-error solution build pass; Rust has 132 tests and the managed total is 798. The registry candidate release DLL SHA256 is `2CFD14246FA963AC284B158903ADC910A782AFEDFEA4EC5F247692BF4613E49A`.
- Packaging builds and copies `msp_core.dll`, supplies license/NOTICE/provenance, rejects raw `MSP/`, `.git`, credentials/private state, PDBs, and dynamic CRT markers, and requires the reported runtime ABI information. The latest completed no-skip package is `0.1.0-native-command-registry-verified`: staged FFI and packaged `LengthDelimitedV2` 2.0 negotiation succeed, the ZIP contains 532 entries with `RawMSP`/PDB/`.git` counts of zero, all five `nativeCommands` exit 0, each of the three proxied commands has one managed audit, cleanup leaves zero native residue, and the ZIP is `artifacts/releases/ReadOS-0.1.0-native-command-registry-verified-win-x64.zip`.
- `native_command_core_registry_v1` is complete. Rust `Command`, `Invocation`, `Context`, `Registry`, and `CommandPack` contracts replace hard-coded command enumeration and dispatch; validation, duplicates, unknown lookup, deterministic names, pack composition, and registry-derived `help` are covered. The 12-case baseline/candidate ABI v1/v2 differential passes with no command-byte, exit-code, diagnostic, audit, state-change, fixture, ABI, or product-routing drift.
- `native_mixed_workspace_read_v1`, `native_stream_core_v1`, `mutable_workspace_write_v1`, and `recoverable_workspace_trash_v1` are complete; pipelines/redirection is the next dependency-ordered slice, followed by model-facing exec sessions, external processes/ConPTY, and wider shell/command conformance as later independent gates. The mixed bridge defines capability traits, longest-prefix mount routing/rebasing, opaque callback handles, disposal, cancellation, concurrency, and .NET delegate lifetime without weakening fixed-local-NTFS confinement, ABI v2, sanitization, or managed lifecycle/audit authority. Synchronous native invocation cannot interrupt an already-running call; current cancellation checks only bound the side-effect-free `pwd`/`echo` calls before and after invocation. Therefore T95 remains in progress.

## Decision Log

### 2026-07-11: Complete The Rust Command Core Registry

`native_command_core_registry_v1` replaces hard-coded Rust command enumeration and dispatch with validated deterministic registry/pack composition. Sixty-six Rust tests, the full managed baseline, the zero-warning/zero-error solution build, and a 12-case baseline/candidate ABI v1/v2 differential pass against DLL SHA256 `2CFD14246FA963AC284B158903ADC910A782AFEDFEA4EC5F247692BF4613E49A`. The no-skip `0.1.0-native-command-registry-verified` package gate also passes with the staged/package evidence recorded above; the next implementation slice is `native_mixed_workspace_read_v1`.

### 2026-07-11: Complete The Length-Delimited ABI v2 Boundary

`native_abi_v2_handshake_v1` closes the native binary negotiation, length-delimited ownership, allocator separation, operation-limit, amplification-resistance, real-DLL, and no-skip packaged-process evidence gaps without changing command behavior or product routing. The next implementation slice is the Rust command core registry.

### 2026-07-10: Move From Component Growth To Trust, Product Proof, And Upstream-Aligned Windows Core

T1-T91 established the vertical service architecture, Hosting boundary, workflows, typed timeline, and artifact lineage. T92 closes the immediate P0 trust gaps. T93 and T94 continue product proof and workbench hardening. T95 uses the local upstream MSP repository as Apache-2.0 design/conformance input for a general Windows-compatible Rust core plus stable .NET adapter; it is not a ReadOS-only JSON protocol exercise, a raw source-tree dependency, or a migration of ReadOS business logic into Rust.

### 2026-07-07: Track Development From the Plan

The immediate product gap is service-layer workflow orchestration, not another PDF reader feature. The first tracked implementation item is `workflow run summarize-current` because it exercises the core MSP loop: command parsing, workspace reads, policy, artifact output, provenance, transcript/session evidence, and tests.
