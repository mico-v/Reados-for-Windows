# ReadOS

ReadOS is a Windows desktop application planned around WinUI and the Windows App SDK. Its product goal is to become a serious AI PDF reading workspace: a native-feeling reader, a durable PDF library, and an AI study workflow that understands pages, outlines, selected regions, chapters, attachments, prompts, and long-term review.

The detailed product goal is maintained in [PRODUCT_GOAL.md](PRODUCT_GOAL.md).

## Current Status

This repository is on the `draft/initial-mvp` branch with an initial hand-scaffolded WinUI shell.

- Product goal: drafted.
- Development plan: drafted.
- WinUI template research: completed at planning level.
- Application source: initial mock shell added under `src/ReadOS.App`.
- Build verification: `dotnet build .\ReadOS.sln` passes with 0 warnings and 0 errors using .NET SDK 10.0.301.

## Planned Stack

- UI framework: WinUI 3 with Windows App SDK.
- Language: C#.
- App architecture: MVVM, likely with CommunityToolkit.Mvvm.
- Packaging: the first hand-scaffolded MVP uses an unpackaged WinUI app for faster F5 debugging; MSIX packaging will be added after the first compile loop.
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

## Build Environment

You only need to install the environment required to build and debug. Codex will handle project scaffolding and source changes.

1. Follow [docs/ENVIRONMENT_SETUP.md](docs/ENVIRONMENT_SETUP.md).
2. Open Visual Studio.
3. Install or confirm the WinUI application development workload.
4. Verify that `WinUI Blank App (Packaged)` is available and that a fresh WinUI app can run with `F5`.
5. Pull the branch, open `ReadOS.sln`, and press `F5`.

Current verified CLI build command:

```powershell
& 'C:\Program Files\dotnet\dotnet.exe' build .\ReadOS.sln
```

More convenient build-and-run command:

```powershell
.\scripts\run.ps1
```

If the app is already open and locking the build output:

```powershell
.\scripts\run.ps1 -StopExisting
```

Build only:

```powershell
.\scripts\run.ps1 -BuildOnly
```

Current MVP shell interactions include library search, mock import, document tabs, page/outline navigation, page-range attachments, region-explain attachments, per-PDF conversation filtering, and mock prompt sending.
