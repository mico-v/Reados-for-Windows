# ReadOS Development Plan

## Purpose

This document turns the product goal into a practical WinUI development plan. The application should be built as a native Windows desktop reader first, with AI workflows integrated into the reading surface rather than treated as a side panel novelty.

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
- .NET CLI (`dotnet`) was not found in PATH or in the common `C:\Program Files\dotnet` location.
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

### Milestone 1: WinUI Shell

- Scaffold WinUI 3 app.
- Establish MVVM Toolkit.
- Add main layout: library panel, PDF workspace placeholder, chat panel placeholder.
- Add navigation and basic command structure.

### Milestone 2: Library MVP

- Persist folders and documents.
- Implement import, rename, delete, trash, restore, and manual ordering.
- Add search.
- Add keyboard shortcuts matching the product goal.

### Milestone 3: Reader MVP

- Select PDF library after spike.
- Render PDFs with page navigation and thumbnails.
- Add tabs and panel hide/show.
- Persist reading progress.

### Milestone 4: AI Chat MVP

- Add provider/model settings.
- Implement per-PDF chat warehouse.
- Attach current page and page ranges.
- Add prompt defaults and conversation persistence.

### Milestone 5: Smart PDF Metadata

- Implement book page mapping workflow.
- Implement manual page mapping editor.
- Implement outline generation workflow.
- Export PDFs with page labels and outlines.

### Milestone 6: Precision Workflows

- Region selection explanation.
- Attachment page deletion/splitting/cropping.
- MinorU-style parse cache integration.
- Outline section explanation.

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
