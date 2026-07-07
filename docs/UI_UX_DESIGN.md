# ReadOS Desktop UI/UX Redesign Plan

ReadOS should feel like a desktop MSP conversation workbench: a place to supervise long-running agent work, inspect source material, approve risky operations, and review generated evidence. It should not feel like a PDF reader with a chat panel attached.

This plan is based on a local Codex Desktop research extraction plus a review of the current WinUI implementation.

## Research Snapshot

The Codex Desktop extractor was run successfully:

- Script: `scripts/extract-codex-ui.ps1`
- Codex version: `26.602.71036`
- Output: `artifacts/codex-research/app-source`
- Notes: `7z` reported ignored macOS symlink warnings while unpacking the `.app`, but `app.asar` was found and extracted successfully.

Useful Codex Desktop assets for layout research:

- `webview/assets/app-shell-DJDX7Pvr.css`
- `webview/assets/app-main-D4WiCdvh.css`
- `webview/assets/thread-app-shell-chrome-CRsi27Q_.js`
- `webview/assets/thread-side-panel-tabs-LLCJoVFx.js`
- `webview/assets/local-conversation-thread-BKdbtMNX.js`
- `webview/assets/composer-BVAB1vz9.js`
- `webview/assets/pending-request-item-panel-CiJyrwmf.js`
- `webview/assets/review-mode-content-DSRDiPn5.js`

Key Codex Desktop patterns observed from the extracted build:

- Thread is the primary workspace object, not the file or preview.
- Right side panel is a tabbed utility dock for review, timeline, browser/app tabs, files, and artifacts.
- Bottom panel is reserved for runtime output such as terminal/process state.
- Composer is a control center, not only a text box: it exposes model, reasoning, approval mode, context, queue, stop/steer/send, side-chat, and goal controls.
- Approval is a first-class surface with explicit risk wording, scope, and durable choices.
- Tool activity is rendered as timeline records: reading, searching, editing, running commands, approved/denied requests, shell output, artifacts, and failures.
- Responsive layout uses shell-level tokens: `46px` toolbar, `36px` small toolbar, `40px` pane toolbar, `clamp(240px, 300px, min(520px, calc(100vw - 320px)))` sidebar sizing, and edge-scroll behavior for wide thread layouts.

Patterns ReadOS should not copy directly:

- Git PR, worktree, branch, and diff workflows should map only to MSP artifacts and audit records unless ReadOS gains source-control features.
- Browser/device annotation features should stay secondary; ReadOS evidence capture is PDF/text/material-first.
- Codex home/marketing surfaces are not appropriate for the ReadOS first screen.
- Dense cloud/account/usage controls should not crowd the local reading workflow.

## Current ReadOS Baseline

The current implementation already has the right outer shell:

```text
Title bar
Workspace sidebar | Active chat thread | Unified inspector
Status bar
```

Implemented files:

- `src/ReadOS.App/MainWindow.xaml`: 3-column shell, title toolbar, status bar, splitters.
- `src/ReadOS.App/Views/WorkspaceSidebarView.xaml`: conversations/materials sidebar.
- `src/ReadOS.App/Views/ChatSurfaceView.xaml`: thread header, transcript, composer.
- `src/ReadOS.App/Views/InspectorView.xaml`: context, run, evidence, attachments, preview tabs.
- `src/ReadOS.App/Services/LayoutService.cs`: responsive breakpoints and pane width calculation.
- `src/ReadOS.App/ViewModels/ShellViewModel.cs`: MSP command execution, streaming status, approvals, audit transcript, artifacts/session rebuilding.
- `src/ReadOS.App/Models/WorkbenchUiModels.cs`: timeline records project explicit message, MSP, approval, artifact, evidence, and error types for the thread surface.

Main gaps:

- The transcript has moved beyond generic chat cards with typed record labels, MSP state titles, distinct body sections for approvals, running commands, failed commands, artifacts, evidence, and ordinary messages, plus actions that route artifacts/evidence/diagnostics into the inspector. Run and Policy inspector tabs now show selected transcript detail cards and visible selected-row state.
- The inspector has too many peer tabs, so "run supervision" and "evidence review" feel buried.
- MSP command output is trapped inside the `Run` tab instead of being available as a persistent runtime surface.
- Approval exists in `MspTranscriptEntry`, but the UI does not make pending approval impossible to miss.
- Composer now exposes queued evidence as removable chips, approval-mode segmented controls, and runtime chips; its primary action switches between send and stop based on runtime state, pending approval chips open the Policy inspector, running-state drafts can be queued and restored, and selected artifacts can be steered into refinement workflow drafts from the current composer text.
- MSP session sidebar rows now show active state, pending approvals, failures, and artifact counts. Thread row status and richer material/artifact row counters still need refinement.
- Preview, outline, document search, and evidence attachments are functional but split across competing surfaces.

## Codex-Native Architecture Decision

The previous ReadOS shell grew from the existing PDF-reader/chat layout. The next iteration should instead start from Codex Desktop's product model and then place ReadOS capabilities into that model. The rule is:

> Thread first, tools around it, runtime below it.

ReadOS should not expose every subsystem as a peer panel. It should expose one work narrative, one context chooser, one review dock, and one runtime drawer.

### Shell Model

Use this native structure:

```text
Global toolbar
Left rail + context list | Thread workspace | Review dock
Runtime drawer
```

This replaces the old mental model:

```text
Title bar
Workspace sidebar | Chat thread | Inspector
Status bar
```

The `Status bar` is removed as an independent surface. Status is promoted into the global toolbar and runtime drawer, because Codex Desktop treats runtime/process state as work context, not as passive footer text.

### Keep, Move, Or Cut

Keep as primary:

- Thread timeline, because it is the work narrative.
- Composer, because it is the control center for prompts, evidence, approval mode, and send/queue.
- MSP transcript, approvals, diagnostics, and artifacts, because they are the ReadOS equivalent of Codex tool activity.
- Preview and evidence capture, because ReadOS is material-first.

Move or downgrade:

- Materials, conversations, MSP sessions, and artifacts move into a left context list behind a slim rail, not four large peer tabs.
- PDF preview, outline, and document search move into the right review dock. They are tools for the selected thread, not the app's center.
- Workspace location/import/new-project controls move to rail/footer actions and toolbar actions.
- Status text moves to the global toolbar and runtime drawer.

Cut from the main shell:

- Separate status bar row.
- Repeated action footer blocks that duplicate toolbar/composer actions.
- Any standalone presenter surface as a top-level shell region. Preview is a review dock tab.
- Generic "settings/content" surfaces from the workspace shell; settings remains a separate route.

### Existing Code Mapping

The first implementation can preserve the existing ViewModel/service contracts while changing the shell organization:

- `ShellViewModel.TimelineItems` feeds the thread workspace.
- `SidebarViewModel` feeds the left rail's contextual list.
- `InspectorViewModel` becomes the review dock model.
- `MspTranscript`, `MspSessions`, and `Artifacts` feed both timeline records and runtime/review surfaces.
- PDF services remain behind Preview/Evidence instead of driving the main layout.

This keeps behavioral risk contained while replacing the UI hierarchy.

## Target Information Architecture

Build from the Codex-native shell above, not from the previous 3-column PDF/chat composition.

```text
Global command bar
Navigation rail + context list | Thread timeline | Review dock
Bottom runtime drawer
```

### Title / Command Bar

Purpose: global state and fast app commands.

Content:

- Brand mark and compact command buttons: import, new thread, theme, settings.
- Active workspace/thread title.
- Runtime chips: local/offline mode, model, approval mode, busy/idle.
- Optional pending approval indicator that opens the run drawer or inspector.

Design:

- Keep `48px` current title row.
- Avoid duplicating long labels already visible in the thread header.
- Use icon buttons with tooltips for global commands.

### Left Rail + Context List

Purpose: choose work, not operate work.

The left side is split into:

- A slim icon rail for primary context types.
- A contextual list for the selected type.

Rail destinations:

- Threads
- Materials
- MSP Sessions
- Artifacts

Thread row should show:

- Title
- Last activity
- Status chip: idle, running, blocked, approval, failed, done
- Small counters: attachments, artifacts, approvals

Material row should show:

- Type icon or prefix
- Title
- Detail line: page count, imported date, current page, search hits when filtering

The previous large sidebar footer should be cut down. Import/new-thread/settings live in the global toolbar. The left pane may keep only small contextual actions that are not already exposed in the composer or toolbar.

- Model/runtime
- MSP host state
- Workspace root action

### Thread Timeline

Purpose: main work narrative.

Replace generic chat cards with typed timeline records:

- User prompt
- Assistant answer
- MSP request
- MSP approval required
- MSP command running
- MSP command result
- Evidence attached
- Artifact created/updated
- Error/recovery hint

Timeline record anatomy:

- Left rail icon or status mark.
- Header: actor/action, timestamp, state.
- Body: message, command, output preview, or evidence summary.
- Actions: approve/deny, copy, open artifact, open evidence, retry, collapse output.

Thread header should show:

- Thread title
- Active material/project scope
- Current page/range context
- Attachment summary
- Running command or approval state

Composer should become the control center:

- Top context row: queued evidence chips, page range, approval mode, model/runtime. Queued evidence chips are implemented with per-item removal, and approval mode is a segmented control wired to MSP policy.
- Text input: stable, multi-line, no layout jump.
- Bottom action row: attach page, attach range, region capture, command palette, stop/steer/send. Steer is implemented for selected artifacts by preparing a `workflow run refine-artifact` draft from the composer instruction.
- Send button changes state: Send, Queue, Stop, Resume depending on runtime state.

### Review Dock

Purpose: inspect selected context and decide next action.

Reduce the top-level tabs to the surfaces a user naturally needs during work:

- `Evidence`: selected material, queued evidence, search results, outline shortcuts.
- `Preview`: PDF/text preview, page navigation, region capture.
- `Run`: current MSP session, command transcript, approvals, policy preview.
- `Artifacts`: generated summaries, exports, search TSVs, answers, audit outputs.
- `Policy`: approval mode, effects preview, safety diagnostics, recovery hints, and failure-review workflow presets.

The inspector should react to selection:

- Selecting an evidence card opens `Evidence`.
- Selecting a page/material opens `Preview`.
- Selecting a running command or approval opens `Run`.
- Selecting an artifact opens `Artifacts`.
- Selecting a policy warning opens `Policy`.

### Bottom Runtime Drawer

Purpose: persistent runtime output without stealing the inspector.

Use a collapsible bottom drawer for:

- Live MSP stdout/stderr.
- Command progress.
- Approval queue.
- Recent failures.
- Background process-like tasks, if MSP gains them later.

Behavior:

- Closed by default.
- Auto-peek when a command starts, fails, or needs approval.
- User can pin open.
- Does not replace the right inspector; it complements it.

No independent status bar is needed. When the drawer is closed, compact runtime state remains visible in the global toolbar and composer chips.

## Core Workflows

### Import And Ask

1. User imports a PDF/material.
2. Sidebar selects the material and thread scope updates.
3. Preview opens in the inspector.
4. User attaches page/range/region from composer or preview toolbar.
5. User sends a prompt.
6. MSP requests and command results appear inline in the thread timeline.
7. Generated artifacts appear both inline and in the inspector `Artifacts` tab.

### Approval Gate

1. Agent requests a risky MSP command.
2. Thread timeline shows an approval-required record.
3. Title bar and sidebar row show pending approval.
4. Bottom run drawer opens to the approval item.
5. Inspector `Policy` tab shows effects, policy preview, diagnostics, and recovery hint.
6. User approves once, denies, or changes approval mode when allowed.

### Evidence Review

1. User clicks an evidence chip, attachment, or search hit.
2. Inspector switches to `Evidence` or `Preview`.
3. Preview highlights the page/region.
4. User can attach more evidence, replace queued evidence, or ask a follow-up.

### Artifact Review

1. MSP writes an artifact.
2. Timeline shows an artifact-created record.
3. Inspector `Artifacts` lists by session/thread.
4. User opens, copies, exports, or attaches the artifact back into the conversation.
5. Rename/delete remain audited MSP commands routed through Run/Policy approval rather than direct inspector icon actions.

### Workflow Draft Review

1. User prepares a workflow from a selected outline item, artifact, or failed MSP diagnostic.
2. Inspector switches to `Run` and fills the command input without executing it.
3. Run inspector keeps the latest prepared commands as a compact history.
4. User restores any prepared draft, edits it if needed, then explicitly runs the MSP command.

## Visual Direction

Use a quiet Windows desktop style:

- Neutral canvas and sidebar colors.
- One restrained accent for primary action and active state.
- Borders and hairlines over heavy shadows.
- Cards only for repeated records: timeline items, evidence chips, artifact rows, approval records.
- Stable dimensions for icon buttons, toolbars, splitters, rows, and composer controls.
- Compact rhythm: `4/6/8/12/16/20/24` spacing tokens already exist.
- Keep corner radius at `4-8px`.
- Prefer Segoe Fluent icons and tooltips for compact actions.

Recommended layout tokens:

- Title bar: `48px`
- Thread header: `46px`
- Pane toolbar: `40px`
- Sidebar row: `52-64px`
- Sidebar width: `240-360px`, default `280px`
- Inspector width: `360-520px`, default `420px`
- Bottom run drawer: `180-320px`
- Splitter hit target: keep current `16px` thumb over `6px` visual column

## Implementation Plan

### Phase 1: Information Architecture Cleanup

Scope: XAML and light ViewModel projection only.

- Rename inspector tabs around user jobs: `Evidence`, `Preview`, `Run`, `Artifacts`, `Policy`.
- Merge duplicate document search/outline controls into `Evidence` and `Preview`.
- Move context rename/delete/export/import out of the first-priority path; keep it under a context details section.
- Add status chips to title bar, thread header, sidebar rows, and inspector header.
- Make pending approval visible outside the `Run` tab.

Primary files:

- `src/ReadOS.App/Views/InspectorView.xaml`
- `src/ReadOS.App/Views/WorkspaceSidebarView.xaml`
- `src/ReadOS.App/Views/ChatSurfaceView.xaml`
- `src/ReadOS.App/ViewModels/InspectorViewModel.cs`
- `src/ReadOS.App/ViewModels/SidebarViewModel.cs`

### Phase 2: Typed Thread Timeline

Scope: add UI projection for existing data.

- Introduce timeline item view models that wrap chat messages, MSP transcript entries, attachments, and artifacts.
- Render different templates for message, MSP command, approval, artifact, evidence, and error items.
- Keep command output collapsed by default with copy/open actions.
- Show policy decision, effects, diagnostics, and recovery hints in a consistent record layout.

Primary files:

- `src/ReadOS.App/ViewModels/ThreadViewModel.cs`
- `src/ReadOS.App/Views/ChatSurfaceView.xaml`
- `src/ReadOS.App/Models/WorkspaceModels.cs`

### Phase 3: Composer Control Center

Scope: improve task entry ergonomics.

- Add model/runtime chip.
- Add approval mode segmented control. Current status: composer and Policy inspector expose policy, confirm-all, and allow-workspace modes.
- Add queued evidence chips with remove actions.
- Add stop/steer/send state handling. Current status: send/stop state is wired, and selected-artifact steer prepares a reviewable refinement workflow draft.
- Add compact command palette entry for MSP commands and evidence operations.
- Keep page range and region capture controls stable and keyboard-friendly.

Primary files:

- `src/ReadOS.App/Views/ChatSurfaceView.xaml`
- `src/ReadOS.App/ViewModels/ThreadViewModel.cs`
- `src/ReadOS.App/ViewModels/ShellViewModel.cs`

### Phase 4: Bottom Run Drawer

Scope: new shell area for runtime supervision.

- Add a collapsible bottom drawer between main workspace and status bar.
- Show live MSP transcript, stdout/stderr, progress, and approval queue.
- Auto-open on command start, failure, or approval required.
- Add pin/collapse controls and remember drawer height.

Primary files:

- `src/ReadOS.App/MainWindow.xaml`
- `src/ReadOS.App/MainWindow.xaml.cs`
- `src/ReadOS.App/Services/LayoutService.cs`
- `src/ReadOS.App/ViewModels/ShellViewModel.cs`

### Phase 5: Artifacts And Evidence Cohesion

Scope: make generated output inspectable and reusable.

- Add an artifact list grouped by MSP session/thread.
- Support artifact preview for markdown/text/TSV first.
- Add open/copy/export/attach actions. Current status: selected artifacts can be previewed, copied, exported, or attached directly from the Artifacts inspector.
- Link artifact rows back to the command that created them.
- Show selected-artifact lineage for source artifacts, manifests, virtual pages, source documents, and page ranges.
- Allow openable source artifacts in lineage to become the selected artifact.
- Add selected outline and artifact actions that prepare workflow command drafts and switch to the Run inspector.
- Keep destructive artifact rename/delete behind explicit MSP command review and approval.
- Link evidence chips back to page/range/region preview.

Primary files:

- `src/ReadOS.App/Views/InspectorView.xaml`
- `src/ReadOS.App/ViewModels/InspectorViewModel.cs`
- `src/ReadOS.App/Models/WorkspaceModels.cs`

### Phase 6: Polish And Responsiveness

Scope: make the redesign feel native and stable.

- Add visual states for compact, medium, wide, and extra-wide windows.
- Animate panel open/close with short reduced-motion-aware transitions.
- Tune light/dark theme tokens after checking screenshots.
- Verify no text overlap in sidebar rows, composer controls, approval records, and inspector tabs.
- Remove old presenter-primary layout concepts once the inspector preview path fully replaces them.

Primary files:

- `src/ReadOS.App/App.xaml`
- `src/ReadOS.App/Services/LayoutService.cs`
- `src/ReadOS.App/MainWindow.xaml`
- `src/ReadOS.App/Views/*.xaml`

## Acceptance Criteria

- First screen is immediately useful: sidebar, active thread, inspector, and composer are all visible when space allows.
- A running MSP command is visible in at least two places: thread timeline and run drawer/status.
- A pending approval is impossible to miss and includes command, effects, policy preview, and approve/deny actions.
- Evidence can be attached from composer or preview and later reopened from the timeline.
- Artifacts can be found from the timeline and inspector without searching the filesystem.
- Narrow windows collapse side surfaces predictably while keeping the thread and composer usable.
- The UI uses existing WinUI controls, keyboard focus, tooltips, and theme resources.
- `dotnet build .\ReadOS.sln` succeeds after each implementation phase.

## Near-Term Recommendation

Do not rebuild the whole UI at once. The current 3-column shell is a good base. Start by changing information hierarchy:

1. Make pending MSP activity and approval state visible in the title/header/sidebar.
2. Convert chat cards into typed timeline records.
3. Add the bottom run drawer.
4. Then refine inspector tabs and artifact/evidence flows.

This sequence gives ReadOS the Codex-like supervision model without losing the document-centered strengths that make ReadOS different.
