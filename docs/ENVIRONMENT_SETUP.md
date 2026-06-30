# Compile And Debug Environment

ReadOS currently targets WinUI 3 with the Windows App SDK, .NET 10, and an optional Rust native MSP core. Use this environment to build the workbench, run MSP tests, and verify the native boundary.

## Required Tools

- Git
- Visual Studio with the WinUI application development workload
- .NET SDK 10
- Windows Developer Mode
- Rust toolchain with `cargo`, required for `native/msp-core` and `.\scripts\verify-msp.ps1`

## Recommended WinUI Install

Microsoft's WinUI quick start supports installing the required Visual Studio workload with WinGet Configuration:

```powershell
winget configure -f https://aka.ms/winui-config
```

Use this route if you are comfortable letting WinGet modify the Visual Studio installation for you.

## Manual WinUI Install

1. Install the latest Visual Studio version that supports WinUI 3 and the Windows App SDK.
2. Open Visual Studio Installer.
3. Select `Modify` for the installed Visual Studio instance.
4. Enable the `WinUI application development` workload.
5. Install the recommended Windows SDK and .NET components selected by that workload.
6. Enable Developer Mode in Windows Settings.
7. Restart Visual Studio.

## Optional Tools

- Template Studio for WinUI, only for reference patterns.
- GitHub CLI, if you want to create PRs or inspect GitHub from the terminal.
- Windows Terminal, if you prefer it over the default PowerShell host.

## Verify The Environment

Run:

```powershell
git --version
dotnet --info
cargo --version
winget --version
```

If `dotnet` is not found, install the SDK directly:

```powershell
winget install --id Microsoft.DotNet.SDK.10 -e --source winget
```

If `cargo` is not found, install Rust from `rustup` or ensure `%USERPROFILE%\.cargo\bin` is on PATH.

## Daily Development Workflow

Build and run the WinUI workbench:

```powershell
.\scripts\run.ps1
```

Build without launching:

```powershell
.\scripts\run.ps1 -BuildOnly
```

Stop an already-running app before rebuilding:

```powershell
.\scripts\run.ps1 -StopExisting
```

Run managed MSP tests:

```powershell
dotnet test .\tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj
```

Run the full MSP verification path:

```powershell
.\scripts\verify-msp.ps1
```

The full verification script runs Rust format checks, Rust tests, clippy, a release build, a native FFI smoke test, .NET MSP tests, and a solution build.

## Visual Studio Debugging

Open `ReadOS.sln` in Visual Studio and press `F5` for UI debugging. For command-line verification, prefer the scripts above so build, test, and packaging behavior stays reproducible.

If Visual Studio-only errors appear, copy the error list text or build output. Text is more useful than screenshots.

## Local Data And Secrets

ReadOS stores local user data under:

```text
%LOCALAPPDATA%\ReadOS
```

Do not commit imported documents, workspace data, generated artifacts, API keys, or provider secrets.
