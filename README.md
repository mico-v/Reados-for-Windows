# ReadOS

ReadOS is a Windows desktop application planned around WinUI and the Windows App SDK. Its product goal is to become a serious AI PDF reading workspace: a native-feeling reader, a durable PDF library, and an AI study workflow that understands pages, outlines, selected regions, chapters, attachments, prompts, and long-term review.

The detailed product goal is maintained in [PRODUCT_GOAL.md](PRODUCT_GOAL.md).

## Current Status

This repository is at project initialization stage.

- Product goal: drafted.
- Development plan: drafted.
- WinUI template research: completed at planning level.
- Application source: not scaffolded yet.

## Planned Stack

- UI framework: WinUI 3 with Windows App SDK.
- Language: C#.
- App architecture: MVVM, likely with CommunityToolkit.Mvvm.
- Packaging: start with packaged WinUI app unless development constraints force an unpackaged build.
- Local data: SQLite or an embedded document database after the first app shell lands.
- PDF layer: to be selected after a focused spike on rendering, page labels, outlines, annotations, and export metadata.
- AI layer: provider-agnostic model configuration with OpenAI-compatible endpoints as the first abstraction target.

## Template Direction

Recommended starting point:

1. Use Visual Studio's WinUI Blank App (Packaged) C# template for the initial app shell.
2. Use Template Studio for WinUI only if we want the wizard to generate MVVM navigation, settings, and test scaffolding up front.
3. Use `microsoft/microsoft-ui-xaml`, WinUI Gallery, and Windows App SDK Samples as references, not as the repository base.

The local machine currently has Git available. GitHub CLI is installed at `C:\Program Files\GitHub CLI\gh.exe`, but its folder was not visible in the Codex runner PATH during initialization checks, so repository automation may call it by full path. `dotnet` was not found in PATH or in the common `C:\Program Files\dotnet` location. WinUI development should be set up through Visual Studio with the WinUI application development workload.

## Product Shape

ReadOS should not be a generic PDF editor or a simple PDF-plus-chat app. The first-class workflows are:

- Finder-like document library.
- Immersive PDF reader with tabs and hideable panels.
- AI book page mapping for scanned and mismatched PDFs.
- AI outline generation and editable bookmarks.
- Fast PDF-to-chat attachment workflows.
- Region selection explanation with context pages.
- Per-PDF chat warehouse for review.
- MinorU-style parsing cache for precise section extraction.
- Workspace export/import and future cloud sync.

## Development Documents

- [PRODUCT_GOAL.md](PRODUCT_GOAL.md): Product vision and core feature map.
- [DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md): WinUI-oriented implementation plan, milestones, and template research.
- [docs/ENVIRONMENT_SETUP.md](docs/ENVIRONMENT_SETUP.md): Windows, Visual Studio, WinUI, and CLI setup guide.
- [docs/INITIAL_MVP.md](docs/INITIAL_MVP.md): Draft branch MVP scope and first shell acceptance criteria.

## Getting Started

The app source has not been scaffolded yet. After the WinUI environment is installed:

1. Follow [docs/ENVIRONMENT_SETUP.md](docs/ENVIRONMENT_SETUP.md).
2. Open Visual Studio.
3. Install or confirm the WinUI application development workload.
4. Create a WinUI Blank App (Packaged) C# project inside `src/ReadOS.App`.
5. Keep project structure aligned with [DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md) and [docs/INITIAL_MVP.md](docs/INITIAL_MVP.md).
