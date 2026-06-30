# Desktop Conversation UI/UX Design

ReadOS should feel like a complete desktop conversation workbench for MSP, not a PDF reader with a chat panel attached. The main surface should support long-running agent conversations, visible context, approval gates, evidence review, and generated artifacts.

## References

- OpenAI Codex app: command center for parallel threads, project sidebar, active thread, review pane, worktrees, automations, Git actions, terminal/actions, sidebar, and artifacts.
  - https://developers.openai.com/codex/app
  - https://openai.com/index/introducing-the-codex-app/
- OpenAI Codex approval modes: chat versus agent behavior and approval boundaries.
  - https://developers.openai.com/codex/ide/features
- OpenAI Codex mobile: active threads, approvals, terminal output, diffs, test results, and real-time supervision.
  - https://openai.com/index/work-with-codex-from-anywhere/
- OpenAI Apps SDK UX: conversational leverage, native fit, and composability.
  - https://developers.openai.com/apps-sdk/concepts/ux-principles
- Microsoft Windows navigation: consistency, simplicity, clarity, standard controls, and clear navigation paths.
  - https://learn.microsoft.com/en-us/windows/apps/design/basics/navigation-basics

## Product Frame

The default layout is a three-pane desktop conversation app:

```text
Workspace sidebar | Active conversation thread | Review / evidence inspector
```

The document preview remains available as a secondary pane, but the primary job is conversation orchestration around MSP commands, domain context, and artifacts.

## Information Architecture

### Workspace Sidebar

- Project and thread discovery.
- Search across conversations and materials.
- Primary actions: new thread, import material, create project.
- Local service status: active model, workspace summary, current status.

### Active Thread

- Header shows task context, model/service state, attachment count, and progress.
- Messages are full-width transcript records, not decorative chat bubbles.
- Attachments are shown inline as evidence chips/cards.
- Composer keeps task entry, page-range attachment, evidence actions, and send action in one stable area.

### Review Inspector

- Context: selected document/project, workspace export/import, rename/delete/open location.
- Run: task mode, current status, approval boundary, layout commands.
- Evidence: model status, source material status, document search.
- Attachments: queued pages, ranges, region captures, and clear/add actions.

### Preview Pane

- PDF/text/evidence preview.
- Navigation, outline, search, page labels, and region capture.
- Used when a task needs visual evidence or document inspection.

## Interaction Rules

- Make the first screen useful without onboarding copy.
- Keep one stable composer at the bottom of the active thread.
- Put risky or mutating operations in the review inspector, not scattered through the transcript.
- Keep navigation shallow: workspace, thread, review, preview, settings.
- Show progress and status persistently in the title/status areas.
- Prefer icon buttons with tooltips for compact window chrome.
- Preserve keyboard-friendly WinUI controls and resizeable panes.

## Visual Direction

- Neutral desktop palette with one restrained accent for active/primary actions.
- Dense but calm spacing: 8-12px internal rhythm, 48-56px headers, compact rows.
- Borders over heavy shadows.
- Cards only for repeated records such as messages, attachments, and status modules.
- Avoid oversized hero areas; the app opens directly into work.
