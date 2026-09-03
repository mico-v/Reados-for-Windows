[CmdletBinding()]
param(
    [switch] $SkipDotNet,
    [switch] $SkipNative
)

$ErrorActionPreference = "Stop"

if ($SkipDotNet -and $SkipNative) {
    throw "At least one verification target must be enabled."
}

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
$legacyBoundaryVerifier = Join-Path $repoRoot "scripts\verify-msp-legacy-boundary.ps1"
& $legacyBoundaryVerifier -RepositoryRoot $repoRoot
if ($LASTEXITCODE -and $LASTEXITCODE -ne 0) {
    throw "MSP legacy boundary verification failed."
}

$runtimeFfiAbiVerifier = Join-Path $repoRoot "scripts\verify-msp-command-runtime-ffi-abi.ps1"
& $runtimeFfiAbiVerifier -RepositoryRoot $repoRoot
if ($LASTEXITCODE -and $LASTEXITCODE -ne 0) {
    throw "MSP command runtime FFI ABI verification failed."
}

$commandProfileVerifier = Join-Path $repoRoot "scripts\verify-msp-command-profile.ps1"
& $commandProfileVerifier -RepositoryRoot $repoRoot
if ($LASTEXITCODE -and $LASTEXITCODE -ne 0) {
    throw "MSP portable command profile verification failed."
}

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
$nativeDllPath = $null

if (-not $SkipNative) {
    if (-not $cargo) {
        throw "cargo was not found. Install Rust or add cargo.exe to PATH."
    }

    Push-Location (Join-Path $repoRoot "native\msp-core")
    try {
        $env:CARGO_INCREMENTAL = "0"
        Invoke-NativeCommand -FilePath $cargo -ArgumentList @("fmt", "--check")
        Invoke-NativeCommand -FilePath $cargo -ArgumentList @("test", "--locked")
        Invoke-NativeCommand -FilePath $cargo -ArgumentList @("clippy", "--all-targets", "--locked", "--", "-D", "warnings")
        Invoke-NativeCommand -FilePath $cargo -ArgumentList @("build", "--release", "--locked")
        $nativeDllPath = Join-Path (Get-Location) "target\release\msp_core.dll"
        if (-not (Test-Path -LiteralPath $nativeDllPath -PathType Leaf)) {
            throw "Native MSP release DLL was not produced: $nativeDllPath"
        }
        & (Join-Path $repoRoot "scripts\verify-msp-native-binary.ps1") `
            -DllPath $nativeDllPath
    }
    finally {
        Pop-Location
    }

    $runtimeFfiDllPath = Join-Path $repoRoot "target\release\msp_command_runtime_ffi.dll"
    & (Join-Path $repoRoot "scripts\verify-msp-command-runtime-ffi.ps1") `
        -DllPath $runtimeFfiDllPath

    & (Join-Path $repoRoot "scripts\smoke-msp-native.ps1") -Configuration release
    if (-not $?) {
        throw "Native MSP FFI smoke failed."
    }
}

if (-not $SkipDotNet) {
    if (-not $dotnet) {
        throw "dotnet was not found. Install the .NET SDK or add dotnet.exe to PATH."
    }

    $previousNativeDll = $env:READOS_MSP_NATIVE_DLL
    if ($nativeDllPath) {
        $env:READOS_MSP_NATIVE_DLL = $nativeDllPath
    }

    Push-Location $repoRoot
    try {
        Invoke-NativeCommand -FilePath $dotnet -ArgumentList @("restore", (Join-Path $repoRoot "ReadOS.sln"))
        Invoke-NativeCommand -FilePath $dotnet -ArgumentList @("test", (Join-Path $repoRoot "tests\ReadOS.Msp.Tests\ReadOS.Msp.Tests.csproj"), "--no-restore")
        Invoke-NativeCommand -FilePath $dotnet -ArgumentList @("test", (Join-Path $repoRoot "tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj"), "--no-restore")
        Invoke-NativeCommand -FilePath $dotnet -ArgumentList @("test", (Join-Path $repoRoot "tests\ReadOS.App.Tests\ReadOS.App.Tests.csproj"), "--no-restore")
        Invoke-NativeCommand -FilePath $dotnet -ArgumentList @("build", (Join-Path $repoRoot "ReadOS.sln"), "--no-restore")
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
}

if ($SkipDotNet) {
    Write-Host "Native MSP verification passed."
}
elseif ($SkipNative) {
    Write-Host "Managed MSP verification passed."
}
else {
    Write-Host "MSP verification passed."
}
