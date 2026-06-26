# ReadOS

ReadOS is a WinUI 3 Windows desktop application for focused PDF reading and AI-assisted study. The current app can persist a local workspace, import real PDFs, render pages, manage page labels and outlines, attach pages to chat, and call an OpenAI-compatible chat endpoint when configured.

The long-term product goal is maintained in [PRODUCT_GOAL.md](PRODUCT_GOAL.md).

## Current Status

- Native WinUI 3 desktop app under `src/ReadOS.App`.
- Local workspace stored in `%LOCALAPPDATA%\ReadOS`.
- Real file import for PDF, Markdown, and text files.
- Real PDF page rendering through `Windows.Data.Pdf`.
- PDF page count, text extraction, search, and existing bookmark import through `PdfPig`.
- Finder-like project/library rail with import, rename, delete-to-ReadOS-trash, search, and open-location commands.
- Reader workspace with page navigation, jump by PDF page or mapped page label, zoom width, thumbnails, outline, and document search.
- Editable page labels and outline entries, plus automatic baseline page mapping and outline generation.
- Region explanation workflow: click "框选讲解", drag a red box on the page, and ReadOS creates a region attachment with context pages.
- Per-document chat warehouse with attachments, persisted conversations, and configurable prompts.
- AI provider settings for OpenAI-compatible `/chat/completions` endpoints. Offline reading mode remains available when no API key is configured.
- Workspace export/import as a zip archive.

Verified locally:

```powershell
& 'C:\Program Files\dotnet\dotnet.exe' build .\ReadOS.sln
```

The app also starts successfully from the Debug build output.

## Stack

- UI framework: WinUI 3 with Windows App SDK.
- Language: C# / .NET 10 Windows target.
- Architecture: single app project with MVVM (`CommunityToolkit.Mvvm`) and service interfaces for workspace storage, PDF, file picking, and AI chat.
- Local data: JSON workspace metadata plus copied user files under `%LOCALAPPDATA%\ReadOS\Library`.
- PDF layer: `Windows.Data.Pdf` for rendering, `PdfPig` for metadata/text/search.
- AI layer: OpenAI-compatible HTTP service with an offline local fallback.

## Run

Build only:

```powershell
.\scripts\run.ps1 -BuildOnly
```

Build and run:

```powershell
.\scripts\run.ps1
```

If the app is already running and locking build output:

```powershell
.\scripts\run.ps1 -StopExisting
```

## Development Documents

- [PRODUCT_GOAL.md](PRODUCT_GOAL.md): Product vision and core feature map.
- [DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md): Current implementation plan and milestones.
- [docs/ENVIRONMENT_SETUP.md](docs/ENVIRONMENT_SETUP.md): Windows, Visual Studio, WinUI, and CLI setup guide.
- [docs/INITIAL_MVP.md](docs/INITIAL_MVP.md): Historical initial shell scope.
