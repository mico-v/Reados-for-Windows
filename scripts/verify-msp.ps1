[CmdletBinding()]
param(
    [switch] $SkipDotNet,
    [switch] $SkipNative
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$dotnetCandidates = @(
    (Join-Path $env:ProgramFiles "dotnet\dotnet.exe"),
    ((Get-Command dotnet -ErrorAction SilentlyContinue).Source)
) | Where-Object { $_ -and (Test-Path $_ -PathType Leaf) }
$dotnet = $dotnetCandidates | Select-Object -First 1

$cargoCandidates = @(
    ((Get-Command cargo -ErrorAction SilentlyContinue).Source),
    (Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe")
) | Where-Object { $_ -and (Test-Path $_ -PathType Leaf) }
$cargo = $cargoCandidates | Select-Object -First 1

if (-not $SkipNative) {
    if (-not $cargo) {
        throw "cargo was not found. Install Rust or add cargo.exe to PATH."
    }

    Push-Location (Join-Path $repoRoot "native\msp-core")
    try {
        $env:CARGO_INCREMENTAL = "0"
        & $cargo fmt --check
        & $cargo test
        & $cargo clippy --all-targets -- -D warnings
        & $cargo build --release
    }
    finally {
        Pop-Location
    }

    & (Join-Path $repoRoot "scripts\smoke-msp-native.ps1") -Configuration release
}

if (-not $SkipDotNet) {
    if (-not $dotnet) {
        throw "dotnet was not found. Install the .NET SDK or add dotnet.exe to PATH."
    }

    & $dotnet restore (Join-Path $repoRoot "ReadOS.sln")
    & $dotnet test (Join-Path $repoRoot "tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj") --no-restore
    & $dotnet test (Join-Path $repoRoot "tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj") --no-restore
    & $dotnet build (Join-Path $repoRoot "ReadOS.sln") --no-restore
}

Write-Host "MSP verification passed."
