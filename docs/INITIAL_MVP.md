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

## First App Shell Acceptance Criteria

- App launches as a packaged WinUI desktop app.
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

