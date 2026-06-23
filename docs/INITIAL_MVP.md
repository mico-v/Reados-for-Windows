# Initial MVP

The first MVP should prove the ReadOS interaction model without trying to solve PDF intelligence or AI quality immediately.

## Branch

Current draft branch:

```text
draft/initial-mvp
```

## MVP Goal

Build a WinUI shell that makes the future product visible:

- left library panel
- central reader workspace
- right chat panel
- top command area
- tab strip placeholder
- settings entry
- local mock state for library items, open tabs, chat messages, and attachments

No real PDF rendering or model calls are required in the first UI MVP. The app should communicate the workflow and make the next integration points obvious.

## Current Scaffold

The first source pass has been added by hand because the Codex runner does not currently have the WinUI template environment installed.

- Solution: `ReadOS.sln`
- App project: `src/ReadOS.App`
- Mode: unpackaged WinUI app for the first compile/debug loop
- Data: in-memory mock workspace
- Status: builds successfully with `.NET SDK 10.0.301`

After the first successful Visual Studio `F5` run, we can decide whether to keep the lean unpackaged setup for fast development or switch to the packaged Visual Studio template shape.

## Current Interactive Surface

The mock shell now supports these MVP interactions:

- Search filters the library list by document name, folder name, detail, or progress text.
- Import creates a new mock PDF, opens it, and gives it a starter conversation.
- Tabs can be opened by selecting documents and closed with `Close Tab`.
- Page slider and outline selection update the current page label.
- Page mapping and outline buttons mutate the mock chips/outline instead of only changing status text.
- Current page, page range, and region explanation add mock attachments.
- Attachments can be cleared.
- Conversation dropdown is scoped to the selected PDF.
- `Send Mock Prompt` stores a user message and a mock ReadOS answer in the selected PDF conversation.

## First App Shell Acceptance Criteria

- App launches as a WinUI desktop app.
- Main window uses a three-region reading layout.
- Library panel shows folders and PDFs from mock data.
- Reader area shows a selected document placeholder with page navigation affordances.
- Chat panel shows a per-document conversation placeholder.
- Toolbar contains commands for import, page attach, region explain, outline, page mapping, settings, and panel toggles.
- Panels can be collapsed or resized if feasible in the first pass.
- ViewModels own UI state; code-behind stays minimal.
- No secrets, API keys, or local user data are committed.

## Suggested Project Structure

```text
src/
  ReadOS.App/
    Assets/
    Models/
    Services/
    ViewModels/
    Views/
tests/
  ReadOS.App.Tests/
docs/
```

Start with one app project if that is fastest. Extract `ReadOS.Core` after the shell stabilizes and the library/PDF/AI services need testable boundaries.

The current source pass follows this single-project starting point.

## First ViewModels

- `ShellViewModel`
  - current mode
  - panel visibility
  - selected document
  - open tabs
- `LibraryViewModel`
  - folder/document tree
  - search query
  - selected library item
- `ReaderViewModel`
  - current document
  - current page label
  - page count
  - thumbnail placeholder state
- `ChatViewModel`
  - conversations
  - current conversation
  - draft prompt
  - attachments
- `SettingsViewModel`
  - placeholder provider list
  - shortcut and prompt placeholders

## Mock Data

Use simple in-memory data for the first shell:

- folders: `Textbooks`, `Papers`, `Manuals`
- documents: `Computer Networking.pdf`, `Abstract Algebra.pdf`, `Transformer Notes.pdf`
- page labels: cover, i, ii, 1, 2, 3
- outline items: chapter, section, subsection
- chat messages: one user question and one AI answer per sample document

## Follow-Up Integration Order

1. Persistent local library model.
2. Real file import.
3. PDF rendering spike.
4. Per-PDF chat warehouse persistence.
5. Provider settings and mockable AI service.
6. Page attachment workflow.
7. Book page mapping and outline generation.
