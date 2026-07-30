[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string] $Configuration = "Debug",

    [switch] $NoBuild,

    [switch] $BuildOnly,

    [switch] $StopExisting,

    [switch] $Wait
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$projectPath = Join-Path $repoRoot "src\ReadOS.App\ReadOS.App.csproj"
$nativeRoot = Join-Path $repoRoot "native\msp-core"
$nativeDllPath = Join-Path $nativeRoot "target\release\msp_core.dll"
$nativeVerificationScript = Join-Path $repoRoot "scripts\verify-msp-native-binary.ps1"
$exePath = Join-Path $repoRoot "src\ReadOS.App\bin\$Configuration\net10.0-windows10.0.19041.0\win-x64\ReadOS.App.exe"
$exeDirectory = Split-Path $exePath -Parent
$deployedNativeDllPath = Join-Path $exeDirectory "msp_core.dll"

$runningProcesses = @(Get-Process -Name "ReadOS.App" -ErrorAction SilentlyContinue)
if ($runningProcesses.Count -gt 0 -and $StopExisting) {
    Write-Host "Stopping running ReadOS.App instance..."
    $runningProcesses | Stop-Process -Force
}
elseif ($runningProcesses.Count -gt 0 -and -not $NoBuild) {
    Write-Warning "ReadOS.App is already running and may lock the build output. Re-run with -StopExisting or close the app window."
    exit 1
}

if (-not $NoBuild) {
    $cargoCandidates = @(
        ((Get-Command cargo -ErrorAction SilentlyContinue).Source),
        (Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe")
    ) | Where-Object { $_ -and (Test-Path $_ -PathType Leaf) }
    $cargo = $cargoCandidates | Select-Object -First 1
    if (-not $cargo) {
        throw "cargo was not found. Install Rust or add cargo.exe to PATH."
    }

    $dotnetCandidates = @(
        (Join-Path $env:ProgramFiles "dotnet\dotnet.exe"),
        ((Get-Command dotnet -ErrorAction SilentlyContinue).Source)
    ) | Where-Object { $_ -and (Test-Path $_ -PathType Leaf) }
    $dotnet = $dotnetCandidates | Select-Object -First 1
    if (-not $dotnet) {
        throw "dotnet was not found. Install the .NET SDK or add dotnet.exe to PATH."
    }

    if (-not (Test-Path -LiteralPath $nativeVerificationScript -PathType Leaf)) {
        throw "Native MSP binary verification script was not found: $nativeVerificationScript"
    }

    Write-Host "Building the Rust MSP core (Release)..."
    Push-Location $nativeRoot
    try {
        & $cargo build --release
        $cargoExitCode = $LASTEXITCODE
        if ($cargoExitCode -ne 0) {
            throw "Rust MSP release build failed with exit code $cargoExitCode."
        }
    }
    finally {
        Pop-Location
    }

    if (-not (Test-Path -LiteralPath $nativeDllPath -PathType Leaf)) {
        throw "Native MSP release DLL was not produced: $nativeDllPath"
    }
    & $nativeVerificationScript -DllPath $nativeDllPath

    Write-Host "Building ReadOS.App ($Configuration)..."
    & $dotnet build $projectPath -c $Configuration
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }

    if (-not (Test-Path -LiteralPath $exePath -PathType Leaf)) {
        throw "Build output was not found: $exePath"
    }

    Copy-Item `
        -LiteralPath $nativeDllPath `
        -Destination $deployedNativeDllPath `
        -Force
    if (-not (Test-Path -LiteralPath $deployedNativeDllPath -PathType Leaf)) {
        throw "Native MSP DLL was not deployed next to ReadOS.App.exe: $deployedNativeDllPath"
    }

    Write-Host "Native MSP DLL deployed next to ReadOS.App.exe."
}

if (-not (Test-Path -LiteralPath $exePath -PathType Leaf)) {
    throw "Build output was not found: $exePath"
}
if (-not (Test-Path -LiteralPath $deployedNativeDllPath -PathType Leaf)) {
    throw "Native MSP DLL was not found next to ReadOS.App.exe: $deployedNativeDllPath"
}

if ($BuildOnly) {
    Write-Host "Build complete."
    Write-Host "  App:        $exePath"
    Write-Host "  Native MSP: $deployedNativeDllPath"
    return
}

Write-Host "Starting ReadOS.App..."
if ($Wait) {
    & $exePath
}
else {
    Start-Process -FilePath $exePath -WorkingDirectory $exeDirectory
}
