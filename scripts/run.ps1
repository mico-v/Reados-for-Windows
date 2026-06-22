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
$exePath = Join-Path $repoRoot "src\ReadOS.App\bin\$Configuration\net10.0-windows10.0.19041.0\win-x64\ReadOS.App.exe"
$exeDirectory = Split-Path $exePath -Parent

$dotnetCandidates = @(
    (Join-Path $env:ProgramFiles "dotnet\dotnet.exe"),
    ((Get-Command dotnet -ErrorAction SilentlyContinue).Source)
) | Where-Object { $_ -and (Test-Path $_ -PathType Leaf) }

$dotnet = $dotnetCandidates | Select-Object -First 1
if (-not $dotnet) {
    throw "dotnet was not found. Install the .NET SDK or add dotnet.exe to PATH."
}

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
    Write-Host "Building ReadOS.App ($Configuration)..."
    & $dotnet build $projectPath -c $Configuration
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

if ($BuildOnly) {
    Write-Host "Build complete."
    return
}

if (-not (Test-Path $exePath -PathType Leaf)) {
    throw "Build output was not found: $exePath"
}

Write-Host "Starting ReadOS.App..."
if ($Wait) {
    & $exePath
}
else {
    Start-Process -FilePath $exePath -WorkingDirectory $exeDirectory
}
