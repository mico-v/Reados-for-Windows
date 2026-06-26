# ReadOS Development Plan

## Purpose

This document turns the product goal into a practical WinUI development plan. The application should be built as a native Windows desktop reader first, with AI workflows integrated into the reading surface rather than treated as a side panel novelty.

## Current Implementation Progress

ReadOS now has a usable local desktop MVP rather than a static shell.

Completed in the current app:

- WinUI 3 app frame with resizable library and chat panes.
- Local workspace persistence under `%LOCALAPPDATA%\ReadOS`.
- Project/library model with import, rename, delete-to-trash, search, and open-location commands.
- Real PDF import and page rendering using `Windows.Data.Pdf`.
- PDF page count, text extraction, document search, and bookmark import using `PdfPig`.
- Reader controls for previous/next, page jump, zoom width, thumbnails, page labels, and outline navigation.
- Editable page labels and outline items.
- Automatic baseline page mapping and heuristic outline generation.
- Region selection workflow that creates a red-box attachment with context pages.
- Per-document chat warehouse with persisted messages and attachments.
- OpenAI-compatible chat service with configurable provider, base URL, API key, model, and prompts.
- Offline reading response mode for use without an API key.
- Workspace export/import as zip archives.

Remaining product-level work:

- PDF metadata writing for exported page labels and outlines.
- Annotation editing beyond the region-explanation overlay.
- Robust drag/drop folder tree, undo/redo, manual ordering UI, and restore UI for trash.
- True scanned-book vision page mapping and table-of-contents extraction.
- Cropped image attachments for selected regions.
- MinorU parse/cache integration and precise chapter/section extraction.
- Packaged installer, migration system, and automated tests.

## WinUI Template Research

The current best starting options are:

1. Visual Studio WinUI Blank App (Packaged) C# template
   - Best default for this repository.
   - Keeps the app shell minimal and avoids inheriting a large sample codebase.
   - Microsoft Learn currently points WinUI developers to Visual Studio with the WinUI application development workload, then the WinUI Blank App (Packaged) C# template.

2. Microsoft Template Studio
   - Useful if we want generated MVVM navigation, settings pages, tests, and common app patterns.
   - It is a Visual Studio extension and supports WinUI 3 project types including Blank, Navigation Pane, and Menu Bar, with MVVM Toolkit support.
   - It should be used as a generator, not cloned wholesale as the product source.

3. Microsoft WinUI Gallery
   - Strong reference for WinUI controls, Fluent styling, adaptive UI, and control snippets.
   - Not a product template for ReadOS because it is intentionally a gallery/sample app.

4. Microsoft Windows App SDK Samples
   - Strong reference for platform features: app lifecycle, deployment, Mica, notifications, windowing, resource management, and Windows AI/OCR samples.
   - Not a product template; use specific samples as references during feature spikes.

5. Microsoft microsoft-ui-xaml
   - This is the upstream WinUI repository, containing framework source, controls, styles, specs, samples, docs, build infrastructure, and release history.
   - It is valuable as an authoritative reference when we need to inspect control behavior, Fluent styling, WinUI limitations, and release direction.
   - It is not a suitable application template for ReadOS because it is the UI framework/product repository itself, with a large native build system and many framework-level concerns unrelated to a desktop app.

Recommendation: start from Visual Studio's WinUI Blank App (Packaged) C# template. If we want navigation/settings/test scaffolding immediately, generate a Template Studio WinUI 3 app in a scratch directory and copy only the patterns we choose. Use `microsoft-ui-xaml`, WinUI Gallery, and Windows App SDK Samples as references, not as the repository base.

## Local Environment Findings

- Git is available.
- GitHub CLI is installed at `C:\Program Files\GitHub CLI\gh.exe`, but `C:\Program Files\GitHub CLI` was not visible in the Codex runner PATH.
- GitHub CLI is authenticated as `mico-v` with repository permissions.
- .NET SDK 10.0.301 is available at `C:\Program Files\dotnet\dotnet.exe` and has been used to verify solution builds.
- WinUI project creation is therefore expected to happen through Visual Studio until the local SDK/CLI path is installed or fixed.

## Proposed Solution Structure

The first scaffold should aim for this shape:

```text
ReadOS.sln
src/
  ReadOS.App/
    App.xaml
    MainWindow.xaml
    Views/
    ViewModels/
    Services/
    Models/
    Assets/
  ReadOS.Core/
    Library/
    Documents/
    Ai/
    Storage/
    Sync/
tests/
  ReadOS.Core.Tests/
docs/
```

If keeping a single project is faster for the first prototype, it is acceptable to start with only `ReadOS.App` and extract `ReadOS.Core` when the library, storage, and PDF logic become stable enough to test separately.

## Architecture Principles

- Keep UI state in ViewModels; keep file, PDF, AI, and persistence logic out of code-behind.
- Use provider interfaces for AI calls so OpenAI-compatible, Gemini-compatible, local, or proxy endpoints can coexist.
- Keep PDF metadata operations behind a document service abstraction because page labels, outlines, export, annotations, and rendering may require different libraries.
- Persist user data locally first; sync should be a later adapter around the same data model.
- Treat long-running AI and PDF parse jobs as cancellable background operations with visible progress.

## Major Modules

### App Shell

- Main window with document library, reader workspace, and chat panel.
- Hideable side panels and adaptive command bars.
- Tab model for multiple open PDFs.
- App-wide command routing and shortcut management.

### Library

- Folder tree and ordered children.
- Import, drag/move, rename, delete, restore, trash, search, and manual ordering.
- Durable local metadata independent of raw file paths.

### PDF Reader

- Rendering and navigation.
- Thumbnails.
- Search.
- Page jump using mapped labels.
- Annotation baseline.
- Export page subsets with metadata.

### PDF Intelligence

- Book page mapping model and editor.
- Outline generation model and editor.
- PDF metadata writer for page labels and outlines.
- OCR/vision job history and error recovery.

### AI Workspace

- Provider/model settings.
- Per-PDF system prompts.
- Per-PDF chat warehouse.
- Attachment manager for pages, ranges, images, PDFs, and Markdown.
- Conversation search and long-image export.

### Precision Study

- Region selection with context pages and red-box annotation.
- Attachment crop/split editor.
- MinorU-style parse cache.
- Outline item to precise section extraction.

### Data And Sync

- Local database and file storage layout.
- Workspace export/import.
- Future iCloud/PCloud-style sync adapter.

## Milestones

### Milestone 0: Repository And Tooling

- Initialize Git repository.
- Add README, product goal, development plan, and .gitignore.
- Create GitHub repository and push initial commit.
- Install or document required local WinUI tooling.

### Milestone 1: WinUI Shell - complete

- Scaffold WinUI 3 app.
- Establish MVVM Toolkit.
- Add main layout: library panel, PDF workspace, chat panel, settings drawer, and panel resizing.
- Add navigation and basic command structure.

### Milestone 2: Library MVP - mostly complete

- Persist folders and documents.
- Implement import, rename, delete, trash, restore, and manual ordering.
- Add search.
- Add keyboard shortcuts matching the product goal.

Current status: local persistence, import, rename, delete-to-trash, search, and open-location are implemented. Folder restore UI, drag/drop, undo/redo, manual ordering UI, and shortcut polish remain.

### Milestone 3: Reader MVP - mostly complete

- Select PDF library after spike.
- Render PDFs with page navigation and thumbnails.
- Add tabs and panel hide/show.
- Persist reading progress.

Current status: PDF rendering, page navigation, thumbnails, search, imported bookmarks, editable outline entries, editable page labels, and reading progress are implemented. Multiple tabs and annotation tools remain.

### Milestone 4: AI Chat MVP - mostly complete

- Add provider/model settings.
- Implement per-PDF chat warehouse.
- Attach current page and page ranges.
- Add prompt defaults and conversation persistence.

Current status: provider settings, OpenAI-compatible chat calls, offline mode, per-document persisted conversations, page/range/region attachments, and prompt defaults are implemented. Conversation export and richer provider-specific options remain.

### Milestone 5: Smart PDF Metadata - partial

- Implement book page mapping workflow.
- Implement manual page mapping editor.
- Implement outline generation workflow.
- Export PDFs with page labels and outlines.

Current status: manual page label editing, baseline page mapping, bookmark import, heuristic outline generation, and manual outline edits are implemented. Vision mapping and PDF metadata writing remain.

### Milestone 6: Precision Workflows - partial

- Region selection explanation.
- Attachment page deletion/splitting/cropping.
- MinorU-style parse cache integration.
- Outline section explanation.

Current status: red-box region selection creates context-page attachments and inserts the region prompt. Cropped image export, attachment crop/split editing, and MinorU integration remain.

## Key Technical Spikes

1. PDF engine spike
   - Requirements: render quality, page thumbnails, text search, annotations, page labels, outlines, export subset, metadata writing, commercial/license constraints.

2. AI vision workflow spike
   - Requirements: page image extraction, prompt schema, structured response validation, retry strategy, cost/progress visibility.

3. Local persistence spike
   - Requirements: library tree, ordering, document metadata, chat history, attachments, parse cache, export/import.

4. WinUI shell spike
   - Requirements: adaptive command bars, Mica/Acrylic where appropriate, custom title bar, panel resizing, keyboard shortcuts.

## Open Decisions

- PDF library choice.
- Packaged versus unpackaged app after the first scaffold.
- Database choice.
- Whether to use Template Studio directly or hand-build MVVM shell from Blank App.
- Exact AI provider configuration schema.
- Whether MinorU integration starts as API-only, local process, or both.
