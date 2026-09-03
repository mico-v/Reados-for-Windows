[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string] $Configuration = "Release",

    [ValidateSet("win-x64")]
    [string] $Runtime = "win-x64",

    [string] $Version,

    [switch] $SkipTests,

    [switch] $NoZip,

    [switch] $SkipSmoke,

    [switch] $StopExisting,

    [Alias("IncludeMspFfi", "IncludePublicFfi")]
    [switch] $IncludePublicMspFfi
)

$ErrorActionPreference = "Stop"

function Invoke-NativeCommand {
    param(
        [Parameter(Mandatory)]
        [string] $FilePath,

        [string[]] $ArgumentList = @()
    )

    & $FilePath @ArgumentList
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "External command failed with exit code ${exitCode}: $FilePath $($ArgumentList -join ' ')"
    }
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$solutionPath = Join-Path $repoRoot "ReadOS.sln"
$projectPath = Join-Path $repoRoot "src\ReadOS.App\ReadOS.App.csproj"
$nativeRoot = Join-Path $repoRoot "native\msp-core"
$nativeDllPath = Join-Path $nativeRoot "target\release\msp_core.dll"
$runtimeFfiRoot = Join-Path $repoRoot "native\msp-command-runtime-ffi"
$runtimeFfiDllPath = Join-Path $repoRoot "target\release\msp_command_runtime_ffi.dll"
$runtimeFfiVerifierPath = Join-Path $repoRoot "scripts\verify-msp-command-runtime-ffi.ps1"
$runtimeFfiLicensePath = Join-Path $runtimeFfiRoot "LICENSE-APACHE-2.0"
$runtimeFfiNoticePath = Join-Path $runtimeFfiRoot "NOTICE"
$publicMspFfiRoot = Join-Path $repoRoot "native\msp-ffi"
$publicMspFfiManifestPath = Join-Path $publicMspFfiRoot "Cargo.toml"
$publicMspFfiDllPath = Join-Path $publicMspFfiRoot "target\release\msp_ffi.dll"
$publicMspFfiHeaderPath = Join-Path $publicMspFfiRoot "include\msp_ffi.h"
$publicMspFfiExportDefinitionPath = Join-Path $publicMspFfiRoot "exports\msp_ffi.def"
$publicMspFfiMetadataPath = Join-Path $publicMspFfiRoot "release-metadata.json"
$testProjectPaths = @(
    (Join-Path $repoRoot "tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj"),
    (Join-Path $repoRoot "tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj"),
    (Join-Path $repoRoot "tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj")
)
$artifactsRoot = Join-Path $repoRoot "artifacts"
$publishRoot = Join-Path $artifactsRoot "publish\ReadOS-windows-$Runtime"
$stagingRoot = Join-Path $artifactsRoot "staging"
$releaseRoot = Join-Path $artifactsRoot "releases"

$dotnetCandidates = @(
    (Join-Path $env:ProgramFiles "dotnet\dotnet.exe"),
    ((Get-Command dotnet -ErrorAction SilentlyContinue).Source)
) | Where-Object { $_ -and (Test-Path $_ -PathType Leaf) }

$dotnet = $dotnetCandidates | Select-Object -First 1
if (-not $dotnet) {
    throw "dotnet was not found. Install the .NET SDK or add dotnet.exe to PATH."
}

$cargoCandidates = @(
    ((Get-Command cargo -ErrorAction SilentlyContinue).Source),
    (Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe")
) | Where-Object { $_ -and (Test-Path $_ -PathType Leaf) }
$cargo = $cargoCandidates | Select-Object -First 1
if (-not $cargo) {
    throw "cargo was not found. Install Rust or add cargo.exe to PATH."
}

function Assert-UnderDirectory {
    param(
        [Parameter(Mandatory)]
        [string] $Path,

        [Parameter(Mandatory)]
        [string] $Directory
    )

    $fullPath = [System.IO.Path]::GetFullPath($Path)
    $fullDirectory = [System.IO.Path]::GetFullPath($Directory)
    if (-not $fullDirectory.EndsWith([System.IO.Path]::DirectorySeparatorChar)) {
        $fullDirectory += [System.IO.Path]::DirectorySeparatorChar
    }

    if (-not $fullPath.StartsWith($fullDirectory, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to operate outside artifact directory: $fullPath"
    }
}

function Remove-DirectorySafe {
    param(
        [Parameter(Mandatory)]
        [string] $Path
    )

    Assert-UnderDirectory -Path $Path -Directory $artifactsRoot
    if (Test-Path $Path) {
        Remove-Item -LiteralPath $Path -Recurse -Force
    }
}

if ([string]::IsNullOrWhiteSpace($Version)) {
    $stamp = Get-Date -Format "yyyyMMdd.HHmm"
    $Version = "0.1.0-local.$stamp"
}

$safeVersion = $Version -replace '[^A-Za-z0-9._-]', '-'
$assemblyVersion = "0.1.0.0"
if ($Version -match '^(?<major>\d+)\.(?<minor>\d+)\.(?<patch>\d+)(\.(?<revision>\d+))?') {
    $revision = if ($Matches.revision) { $Matches.revision } else { "0" }
    $assemblyVersion = "$($Matches.major).$($Matches.minor).$($Matches.patch).$revision"
}

$packageName = "ReadOS-$safeVersion-$Runtime"
$packageRoot = Join-Path $stagingRoot $packageName
$zipPath = Join-Path $releaseRoot "$packageName.zip"

$runningProcesses = @(Get-Process -Name "ReadOS.App" -ErrorAction SilentlyContinue)
if ($runningProcesses.Count -gt 0 -and $StopExisting) {
    Write-Host "Stopping running ReadOS.App instance..."
    $runningProcesses | ForEach-Object {
        $_.Kill($true)
        [void]$_.WaitForExit(5000)
    }
}
elseif ($runningProcesses.Count -gt 0) {
    throw "ReadOS.App is already running and may lock publish output. Re-run with -StopExisting or close the app window."
}

New-Item -ItemType Directory -Force -Path $artifactsRoot, $stagingRoot, $releaseRoot | Out-Null
Remove-DirectorySafe -Path $publishRoot
Remove-DirectorySafe -Path $packageRoot
if (Test-Path $zipPath) {
    Assert-UnderDirectory -Path $zipPath -Directory $releaseRoot
    Remove-Item -LiteralPath $zipPath -Force
}

$previousNativeDll = $env:READOS_MSP_NATIVE_DLL
Push-Location $repoRoot
try {
    Push-Location $nativeRoot
    try {
        $env:CARGO_INCREMENTAL = "0"
        if (-not $SkipTests) {
            Write-Host "Verifying the Rust MSP core..."
            Invoke-NativeCommand -FilePath $cargo -ArgumentList @("fmt", "--check")
            Invoke-NativeCommand -FilePath $cargo -ArgumentList @("test")
            Invoke-NativeCommand -FilePath $cargo -ArgumentList @("clippy", "--all-targets", "--", "-D", "warnings")
        }

        Write-Host "Building the Rust MSP core release DLL..."
        Invoke-NativeCommand -FilePath $cargo -ArgumentList @("build", "--release")
    }
    finally {
        Pop-Location
    }

    if (-not (Test-Path -LiteralPath $nativeDllPath -PathType Leaf)) {
        throw "Native MSP release DLL was not produced: $nativeDllPath"
    }
    & (Join-Path $repoRoot "scripts\verify-msp-native-binary.ps1") `
        -DllPath $nativeDllPath

    $env:CARGO_INCREMENTAL = "0"
    Write-Host "Verifying the command runtime FFI crate..."
    & $runtimeFfiVerifierPath -DllPath $runtimeFfiDllPath

    if (-not (Test-Path -LiteralPath $runtimeFfiDllPath -PathType Leaf)) {
        throw "Command runtime FFI release DLL was not produced: $runtimeFfiDllPath"
    }
    foreach ($runtimeFfiFile in @($runtimeFfiLicensePath, $runtimeFfiNoticePath)) {
        if (-not (Test-Path -LiteralPath $runtimeFfiFile -PathType Leaf)) {
            throw "Command runtime FFI package notice/license was not found: $runtimeFfiFile"
        }
    }

    if ($IncludePublicMspFfi) {
        $previousRustFlags = $env:RUSTFLAGS
        try {
            $env:CARGO_INCREMENTAL = "0"
            $staticMspFfiFlags = "-C target-feature=+crt-static -C link-arg=/Brepro"
            if (-not [string]::IsNullOrWhiteSpace($previousRustFlags)) {
                $staticMspFfiFlags = "$previousRustFlags $staticMspFfiFlags"
            }
            $env:RUSTFLAGS = $staticMspFfiFlags

            Write-Host "Verifying the public MSP FFI crate..."
            if (-not $SkipTests) {
                Invoke-NativeCommand -FilePath $cargo -ArgumentList @(
                    "test",
                    "--manifest-path", $publicMspFfiManifestPath,
                    "--locked"
                )
            }
            Write-Host "Building the public MSP FFI release DLL..."
            Invoke-NativeCommand -FilePath $cargo -ArgumentList @(
                "build",
                "--release",
                "--manifest-path", $publicMspFfiManifestPath,
                "--locked"
            )
        }
        finally {
            if ($null -eq $previousRustFlags) {
                Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue
            }
            else {
                $env:RUSTFLAGS = $previousRustFlags
            }
        }

        if (-not (Test-Path -LiteralPath $publicMspFfiDllPath -PathType Leaf)) {
            throw "Public MSP FFI release DLL was not produced: $publicMspFfiDllPath"
        }
        & (Join-Path $repoRoot "scripts\verify-msp-ffi-release.ps1") `
            -DllPath $publicMspFfiDllPath `
            -ManifestPath $publicMspFfiManifestPath `
            -HeaderPath $publicMspFfiHeaderPath `
            -ExportDefinitionPath $publicMspFfiExportDefinitionPath `
            -MetadataPath $publicMspFfiMetadataPath
    }

    $env:READOS_MSP_NATIVE_DLL = $nativeDllPath

    if (-not $SkipTests) {
        Write-Host "Restoring the ReadOS solution..."
        Invoke-NativeCommand -FilePath $dotnet -ArgumentList @("restore", $solutionPath)

        foreach ($testProjectPath in $testProjectPaths) {
            Write-Host "Running tests: $testProjectPath"
            Invoke-NativeCommand -FilePath $dotnet -ArgumentList @("test", $testProjectPath, "-c", $Configuration, "--no-restore")
        }
    }

    Write-Host "Restoring ReadOS.App for $Runtime..."
    Invoke-NativeCommand -FilePath $dotnet -ArgumentList @(
        "restore",
        $projectPath,
        "-r", $Runtime,
        "/p:WindowsAppSDKSelfContained=true"
    )

    Write-Host "Publishing ReadOS.App ($Configuration, $Runtime, version $Version)..."
    Invoke-NativeCommand -FilePath $dotnet -ArgumentList @(
        "publish",
        $projectPath,
        "-c", $Configuration,
        "-r", $Runtime,
        "--self-contained", "true",
        "--no-restore",
        "-o", $publishRoot,
        "/p:Version=$Version",
        "/p:InformationalVersion=$Version",
        "/p:AssemblyVersion=$assemblyVersion",
        "/p:FileVersion=$assemblyVersion",
        "/p:WindowsAppSDKSelfContained=true",
        "/p:DebugType=None",
        "/p:DebugSymbols=false",
        "/p:PathMap=$repoRoot=/_/reados",
        "/p:PublishSingleFile=false"
    )
}
finally {
    Pop-Location
    if ($null -eq $previousNativeDll) {
        Remove-Item Env:READOS_MSP_NATIVE_DLL -ErrorAction SilentlyContinue
    }
    else {
        $env:READOS_MSP_NATIVE_DLL = $previousNativeDll
    }
}

New-Item -ItemType Directory -Force -Path $publishRoot | Out-Null
Copy-Item -LiteralPath $runtimeFfiDllPath `
    -Destination (Join-Path $publishRoot "msp_command_runtime_ffi.dll") `
    -Force
$publishRuntimeFfiLicenseRoot = Join-Path $publishRoot "licenses\msp-command-runtime-ffi"
New-Item -ItemType Directory -Force -Path $publishRuntimeFfiLicenseRoot | Out-Null
Copy-Item -LiteralPath $runtimeFfiLicensePath `
    -Destination (Join-Path $publishRuntimeFfiLicenseRoot "LICENSE-APACHE-2.0") `
    -Force
Copy-Item -LiteralPath $runtimeFfiNoticePath `
    -Destination (Join-Path $publishRuntimeFfiLicenseRoot "NOTICE") `
    -Force

New-Item -ItemType Directory -Force -Path $packageRoot | Out-Null
Copy-Item -Path (Join-Path $publishRoot "*") -Destination $packageRoot -Recurse -Force
Copy-Item -LiteralPath $nativeDllPath -Destination (Join-Path $packageRoot "msp_core.dll") -Force
if ($IncludePublicMspFfi) {
    Copy-Item -LiteralPath $publicMspFfiDllPath -Destination (Join-Path $packageRoot "msp_ffi.dll") -Force
    $publicMspFfiIncludeRoot = Join-Path $packageRoot "include"
    New-Item -ItemType Directory -Force -Path $publicMspFfiIncludeRoot | Out-Null
    Copy-Item -LiteralPath $publicMspFfiHeaderPath `
        -Destination (Join-Path $publicMspFfiIncludeRoot "msp_ffi.h") `
        -Force
}

$nativeLicenseRoot = Join-Path $packageRoot "licenses\msp-upstream"
New-Item -ItemType Directory -Force -Path $nativeLicenseRoot | Out-Null
Copy-Item -LiteralPath (Join-Path $repoRoot "conformance\msp-upstream\APACHE-2.0.txt") `
    -Destination (Join-Path $nativeLicenseRoot "APACHE-2.0.txt") `
    -Force
Copy-Item -LiteralPath (Join-Path $repoRoot "conformance\msp-upstream\NOTICE") `
    -Destination (Join-Path $nativeLicenseRoot "NOTICE") `
    -Force
Copy-Item -LiteralPath (Join-Path $nativeRoot "UPSTREAM_MSP.md") `
    -Destination (Join-Path $nativeLicenseRoot "SOURCE-PROVENANCE.md") `
    -Force

$publicMspFfiReleaseNote = ""
if ($IncludePublicMspFfi) {
    $publicMspFfiMetadata = Get-Content -LiteralPath $publicMspFfiMetadataPath -Raw | ConvertFrom-Json
    $publicMspFfiReleaseNote = @"
Public MSP FFI: msp_ffi.dll (29 exports, header ABI $($publicMspFfiMetadata.abi_version)/$($publicMspFfiMetadata.header_version), SHA-256 $($publicMspFfiMetadata.hashes.dll), static MSVC CRT; opt-in via -IncludePublicMspFfi)
Public MSP FFI header: include\msp_ffi.h
"@
}

$releaseNotes = @"
ReadOS Windows Release
Version: $Version
Runtime: $Runtime
Configuration: $Configuration
Built: $(Get-Date -Format "yyyy-MM-dd HH:mm:ss zzz")
Native MSP: msp_core.dll (ABI 2.0, JSON reados-msp-native/1, static MSVC CRT)
Command runtime FFI: msp_command_runtime_ffi.dll (ABI 1, header 0.1.0, exact 13 exports, x64, static MSVC CRT)
Command runtime FFI notices: licenses\msp-command-runtime-ffi\LICENSE-APACHE-2.0 and NOTICE
$publicMspFfiReleaseNote
Start:
  Run ReadOS.App.exe

Local data:
  %LOCALAPPDATA%\ReadOS

Distribution:
  This is an unpackaged, self-contained WinUI 3 release folder.
  Keep all files together next to ReadOS.App.exe.
"@

Set-Content -LiteralPath (Join-Path $packageRoot "RELEASE.txt") -Value $releaseNotes -Encoding UTF8
Copy-Item -LiteralPath (Join-Path $repoRoot "README.md") -Destination (Join-Path $packageRoot "README.md") -Force

& (Join-Path $repoRoot "scripts\verify-windows-package.ps1") `
    -PackageRoot $packageRoot `
    -ArtifactsRoot $artifactsRoot `
    -RequireNativeMsp `
    -RequireCommandRuntimeFfi `
    -RequirePublicMspFfi:$IncludePublicMspFfi

if (-not $SkipSmoke) {
    Write-Host "Running the staged native MSP ABI smoke..."
    & (Join-Path $repoRoot "scripts\smoke-msp-native.ps1") `
        -DllPath (Join-Path $packageRoot "msp_core.dll")

    $smokeRoot = Join-Path $artifactsRoot "smoke\$packageName"
    & (Join-Path $repoRoot "scripts\smoke-windows-package.ps1") `
        -PackageRoot $packageRoot `
        -ArtifactsRoot $artifactsRoot `
        -SmokeRoot $smokeRoot
}

if (-not $NoZip) {
    Write-Host "Creating zip package..."
    Compress-Archive -Path $packageRoot -DestinationPath $zipPath -Force
}

Write-Host ""
Write-Host "Windows package created:"
Write-Host "  Publish directory: $publishRoot"
Write-Host "  Staged package:    $packageRoot"
if (-not $NoZip) {
    Write-Host "  Zip package:       $zipPath"
}
