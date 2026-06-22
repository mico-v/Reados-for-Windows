# Environment Setup

This project targets WinUI 3 with the Windows App SDK. The development environment should be installed before scaffolding the actual app project.

## Recommended Setup

Microsoft's WinUI quick start currently recommends setting up the environment with WinGet Configuration:

```powershell
winget configure -f https://aka.ms/winui-config
```

That configuration is intended to install Visual Studio with the required WinUI and Windows App SDK workloads and enable Developer Mode.

Use this route if you are comfortable letting WinGet modify the Visual Studio installation for you.

## Manual Setup

Use this route if you prefer to control the Visual Studio installation yourself.

1. Install the latest Visual Studio.
2. Open Visual Studio Installer.
3. Select `Modify` for the installed Visual Studio instance.
4. Enable the `WinUI application development` workload.
5. Enable Developer Mode in Windows Settings.
   - Open Settings.
   - Go to `System`.
   - Open the advanced/developer settings area.
   - Turn on `Developer Mode`.
6. Restart Visual Studio.

After this, Visual Studio should show the `WinUI Blank App (Packaged)` C# project template.

## Optional Template Studio

Template Studio can generate a fuller starting shell with navigation, MVVM Toolkit, settings pages, and tests. Use it only if we decide we want generated structure instead of a minimal Blank App.

Install the Visual Studio extension:

- Template Studio for WinUI (C#)

Recommended Template Studio choices for ReadOS, if used:

- Project type: `Navigation Pane` or `Blank`
- Pattern: `MVVM Toolkit`
- Pages: shell/home, settings, library, reader placeholder, chat placeholder
- Tests: unit test project if available

## Verify The Environment

Run these checks in a new terminal after installation:

```powershell
git --version
gh auth status
dotnet --info
winget --version
```

In Visual Studio, also verify:

- `Create a new project` can find `WinUI Blank App (Packaged)`.
- A fresh WinUI app builds and launches with `F5`.
- Developer Mode is enabled if deployment prompts appear.

## Repository-Specific Notes

In the Codex runner, GitHub CLI was installed at:

```text
C:\Program Files\GitHub CLI\gh.exe
```

but that folder was not present in the runner PATH. If your own terminal already resolves `gh`, no action is needed. If not, add this folder to your user or system PATH.

The Codex runner did not find `dotnet.exe` in PATH or in `C:\Program Files\dotnet`. Installing the WinUI workload through Visual Studio should resolve that for normal development terminals.

## Scaffold The App After Setup

After the environment is ready:

1. Open this repository in Visual Studio.
2. Create a new project in the repository.
3. Select `WinUI Blank App (Packaged)` with C#.
4. Name the project `ReadOS.App`.
5. Place it under `src/ReadOS.App`.
6. Name the solution `ReadOS`.
7. Commit the generated solution and app project on the draft branch.

