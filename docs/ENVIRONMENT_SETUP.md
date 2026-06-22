# Compile And Debug Environment

This project targets WinUI 3 with the Windows App SDK. You only need the environment required to pull, build, run, and debug the app. Codex will handle project scaffolding and source changes.

## Recommended Install

Microsoft's WinUI quick start currently recommends setting up the environment with WinGet Configuration:

```powershell
winget configure -f https://aka.ms/winui-config
```

That configuration is intended to install Visual Studio with the required WinUI and Windows App SDK workloads and enable Developer Mode.

Use this route if you are comfortable letting WinGet modify the Visual Studio installation for you.

## Manual Install

Use this route if you prefer to control the Visual Studio installation yourself.

1. Install the latest Visual Studio version that supports WinUI 3 and the Windows App SDK.
2. Open Visual Studio Installer.
3. Select `Modify` for the installed Visual Studio instance.
4. Enable the `WinUI application development` workload.
5. Install any recommended Windows SDK and .NET components selected by that workload.
6. Enable Developer Mode in Windows Settings.
   - Open Settings.
   - Go to `System`.
   - Open the advanced/developer settings area.
   - Turn on `Developer Mode`.
7. Restart Visual Studio.

After this, Visual Studio should show the `WinUI Blank App (Packaged)` C# project template.

## Optional Tools

These are useful but not required just to build and debug:

- Template Studio for WinUI, if we later decide to generate reference shell code.
- GitHub CLI, if you want to create PRs or inspect GitHub from the terminal.
- Windows Terminal, if you prefer it over the default PowerShell host.

## Verify The Environment

Run these checks in a new terminal after installation:

```powershell
git --version
gh auth status
dotnet --info
winget --version
```

If `dotnet` is still not found after installing Visual Studio, install the .NET SDK directly:

```powershell
winget install --id Microsoft.DotNet.SDK.10 -e --source winget
```

The current verified SDK is `.NET SDK 10.0.301`.

In Visual Studio, also verify:

- `Create a new project` can find `WinUI Blank App (Packaged)`.
- A fresh WinUI app builds and launches with `F5`.
- Developer Mode is enabled if deployment prompts appear.

You do not need to create the ReadOS project yourself. The template check only proves your machine can compile and debug the WinUI app once Codex adds it to the repository.

## Daily Compile And Debug Workflow

After Codex adds the app source, your normal workflow should be:

```powershell
git switch draft/initial-mvp
git pull
```

Then open `ReadOS.sln` in Visual Studio and press `F5`.

If you want to build from the terminal after the solution exists:

```powershell
dotnet build .\ReadOS.sln
```

If your shell has not refreshed PATH yet, use the full path:

```powershell
& 'C:\Program Files\dotnet\dotnet.exe' build .\ReadOS.sln
```

The first MVP project is intentionally unpackaged to make the initial compile/debug loop faster. Visual Studio should still have the WinUI workload installed because the project uses WinUI and the Windows App SDK.

## Debugging Access

Codex cannot currently attach to your Visual Studio instance through an exposed Visual Studio MCP server in this thread. The reliable workflow is:

1. Codex runs CLI builds with `dotnet` or Visual Studio MSBuild.
2. You run `F5` in Visual Studio when UI debugging is needed.
3. If Visual Studio-only errors appear, copy the error list text or build output. Screenshots are optional; text is better.

Do not edit hidden Visual Studio Copilot configuration files to replace GitHub Copilot's base URL or API key. GitHub documents MCP and bring-your-own-key flows for Copilot, but the supported BYOK surfaces are Copilot Chat, Copilot CLI, and VS Code through organization or local configuration. It is not the supported route for wiring this Codex desktop thread into Visual Studio debugging.

## Repository-Specific Notes

In the Codex runner, GitHub CLI was installed at:

```text
C:\Program Files\GitHub CLI\gh.exe
```

but that folder was not present in the runner PATH. If your own terminal already resolves `gh`, no action is needed. If not, add this folder to your user or system PATH.

The Codex runner did not find `dotnet.exe` in PATH or in `C:\Program Files\dotnet`. Installing the WinUI workload through Visual Studio should resolve that for normal development terminals.
