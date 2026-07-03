[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string] $Configuration = "Release",

    [ValidateSet("win-x64")]
    [string] $Runtime = "win-x64",

    [string] $Version,

    [switch] $SkipTests,

    [switch] $NoZip,

    [switch] $StopExisting
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$projectPath = Join-Path $repoRoot "src\ReadOS.App\ReadOS.App.csproj"
$testProjectPath = Join-Path $repoRoot "tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj"
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
    Write-Warning "ReadOS.App is already running and may lock publish output. Re-run with -StopExisting or close the app window."
    exit 1
}

New-Item -ItemType Directory -Force -Path $artifactsRoot, $stagingRoot, $releaseRoot | Out-Null
Remove-DirectorySafe -Path $publishRoot
Remove-DirectorySafe -Path $packageRoot
if (Test-Path $zipPath) {
    Assert-UnderDirectory -Path $zipPath -Directory $releaseRoot
    Remove-Item -LiteralPath $zipPath -Force
}

if (-not $SkipTests) {
    Write-Host "Running MSP tests..."
    & $dotnet test $testProjectPath -c $Configuration --no-restore
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

Write-Host "Publishing ReadOS.App ($Configuration, $Runtime, version $Version)..."
& $dotnet publish $projectPath `
    -c $Configuration `
    -r $Runtime `
    --self-contained true `
    -o $publishRoot `
    /p:Version=$Version `
    /p:InformationalVersion=$Version `
    /p:AssemblyVersion=$assemblyVersion `
    /p:FileVersion=$assemblyVersion `
    /p:WindowsAppSDKSelfContained=true `
    /p:PublishSingleFile=false
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

New-Item -ItemType Directory -Force -Path $packageRoot | Out-Null
Copy-Item -Path (Join-Path $publishRoot "*") -Destination $packageRoot -Recurse -Force

$releaseNotes = @"
ReadOS Windows Release
Version: $Version
Runtime: $Runtime
Configuration: $Configuration
Built: $(Get-Date -Format "yyyy-MM-dd HH:mm:ss zzz")

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
