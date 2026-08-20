[CmdletBinding()]
param(
    [switch] $SkipBuild
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

function Assert-Fails {
    param(
        [Parameter(Mandatory)]
        [scriptblock] $Action,

        [Parameter(Mandatory)]
        [string] $Description
    )

    $failed = $false
    try {
        & $Action
    }
    catch {
        $failed = $true
    }

    if (-not $failed) {
        throw "Expected focused package test to fail: $Description"
    }
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$artifactsRoot = Join-Path $repoRoot "artifacts"
$fixtureRoot = Join-Path $artifactsRoot "package-verification-tests-$PID"
$ffiRoot = Join-Path $repoRoot "native\msp-ffi"
$ffiManifestPath = Join-Path $ffiRoot "Cargo.toml"
$ffiDllPath = Join-Path $ffiRoot "target\release\msp_ffi.dll"
$ffiHeaderPath = Join-Path $ffiRoot "include\msp_ffi.h"
$ffiVerifierPath = Join-Path $repoRoot "scripts\verify-msp-ffi-release.ps1"
$packageVerifierPath = Join-Path $repoRoot "scripts\verify-windows-package.ps1"

$cargoCandidates = @(
    ((Get-Command cargo -ErrorAction SilentlyContinue).Source),
    (Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe")
) | Where-Object { $_ -and (Test-Path $_ -PathType Leaf) }
$cargo = $cargoCandidates | Select-Object -First 1
if (-not $SkipBuild -and -not $cargo) {
    throw "cargo was not found. Use -SkipBuild with an existing FFI release DLL or install Rust."
}

try {
    New-Item -ItemType Directory -Force -Path $fixtureRoot | Out-Null

    if (-not $SkipBuild) {
        $previousRustFlags = $env:RUSTFLAGS
        $previousCargoIncremental = $env:CARGO_INCREMENTAL
        try {
            $env:CARGO_INCREMENTAL = "0"
            $env:RUSTFLAGS = "-C target-feature=+crt-static -C link-arg=/Brepro"
            Invoke-NativeCommand -FilePath $cargo -ArgumentList @(
                "build",
                "--release",
                "--manifest-path", $ffiManifestPath,
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
            if ($null -eq $previousCargoIncremental) {
                Remove-Item Env:CARGO_INCREMENTAL -ErrorAction SilentlyContinue
            }
            else {
                $env:CARGO_INCREMENTAL = $previousCargoIncremental
            }
        }
    }

    if (-not (Test-Path -LiteralPath $ffiDllPath -PathType Leaf)) {
        throw "Focused package tests require the FFI release DLL: $ffiDllPath"
    }

    & $ffiVerifierPath -DllPath $ffiDllPath

    $defaultPackage = Join-Path $fixtureRoot "default"
    $publicPackage = Join-Path $fixtureRoot "public"
    foreach ($package in @($defaultPackage, $publicPackage)) {
        New-Item -ItemType Directory -Force -Path $package | Out-Null
        Set-Content -LiteralPath (Join-Path $package "ReadOS.App.exe") -Value "fixture" -NoNewline
        Set-Content -LiteralPath (Join-Path $package "ReadOS.App.dll") -Value "fixture" -NoNewline
        Set-Content -LiteralPath (Join-Path $package "RELEASE.txt") -Value "fixture" -NoNewline
        Set-Content -LiteralPath (Join-Path $package "README.md") -Value "fixture" -NoNewline
    }

    New-Item -ItemType Directory -Force -Path (Join-Path $publicPackage "include") | Out-Null
    Copy-Item -LiteralPath $ffiDllPath -Destination (Join-Path $publicPackage "msp_ffi.dll") -Force
    Copy-Item -LiteralPath $ffiHeaderPath -Destination (Join-Path $publicPackage "include\msp_ffi.h") -Force

    & $packageVerifierPath -PackageRoot $defaultPackage -ArtifactsRoot $artifactsRoot
    & $packageVerifierPath -PackageRoot $publicPackage -ArtifactsRoot $artifactsRoot -RequirePublicMspFfi

    Add-Content -LiteralPath (Join-Path $publicPackage "include\msp_ffi.h") -Value "// tampered" -NoNewline
    Assert-Fails {
        & $packageVerifierPath -PackageRoot $publicPackage -ArtifactsRoot $artifactsRoot -RequirePublicMspFfi
    } "tampered staged FFI header hash"
    Copy-Item -LiteralPath $ffiHeaderPath -Destination (Join-Path $publicPackage "include\msp_ffi.h") -Force

    Copy-Item -LiteralPath $ffiDllPath -Destination (Join-Path $defaultPackage "msp_ffi.dll") -Force
    Assert-Fails {
        & $packageVerifierPath -PackageRoot $defaultPackage -ArtifactsRoot $artifactsRoot
    } "default package with unrequested msp_ffi.dll"
    Remove-Item -LiteralPath (Join-Path $defaultPackage "msp_ffi.dll") -Force

    New-Item -ItemType Directory -Force -Path (Join-Path $defaultPackage "native") | Out-Null
    Copy-Item -LiteralPath $ffiDllPath -Destination (Join-Path $defaultPackage "native\msp_ffi.dll") -Force
    Assert-Fails {
        & $packageVerifierPath -PackageRoot $defaultPackage -ArtifactsRoot $artifactsRoot
    } "public FFI DLL at an unapproved path"
    Remove-Item -LiteralPath (Join-Path $defaultPackage "native") -Recurse -Force

    Set-Content -LiteralPath (Join-Path $defaultPackage "native.rs") -Value "source" -NoNewline
    Assert-Fails {
        & $packageVerifierPath -PackageRoot $defaultPackage -ArtifactsRoot $artifactsRoot
    } "Rust source in package"
    Remove-Item -LiteralPath (Join-Path $defaultPackage "native.rs") -Force

    Set-Content -LiteralPath (Join-Path $defaultPackage "native.swift") -Value "source" -NoNewline
    Assert-Fails {
        & $packageVerifierPath -PackageRoot $defaultPackage -ArtifactsRoot $artifactsRoot
    } "Swift source in package"
    Remove-Item -LiteralPath (Join-Path $defaultPackage "native.swift") -Force

    Set-Content -LiteralPath (Join-Path $defaultPackage "ReadOS.App.pdb") -Value "generated" -NoNewline
    Assert-Fails {
        & $packageVerifierPath -PackageRoot $defaultPackage -ArtifactsRoot $artifactsRoot
    } "PDB in package"
    Remove-Item -LiteralPath (Join-Path $defaultPackage "ReadOS.App.pdb") -Force

    Set-Content -LiteralPath (Join-Path $defaultPackage "native.obj") -Value "generated" -NoNewline
    Assert-Fails {
        & $packageVerifierPath -PackageRoot $defaultPackage -ArtifactsRoot $artifactsRoot
    } "object file in package"
    Remove-Item -LiteralPath (Join-Path $defaultPackage "native.obj") -Force

    Set-Content -LiteralPath (Join-Path $defaultPackage "private-state.json") -Value "private" -NoNewline
    Assert-Fails {
        & $packageVerifierPath -PackageRoot $defaultPackage -ArtifactsRoot $artifactsRoot
    } "private state in package"
    Remove-Item -LiteralPath (Join-Path $defaultPackage "private-state.json") -Force

    New-Item -ItemType Directory -Force -Path (Join-Path $defaultPackage "MSP") | Out-Null
    Set-Content -LiteralPath (Join-Path $defaultPackage "MSP\README.md") -Value "raw" -NoNewline
    Assert-Fails {
        & $packageVerifierPath -PackageRoot $defaultPackage -ArtifactsRoot $artifactsRoot
    } "raw MSP source in package"
    Remove-Item -LiteralPath (Join-Path $defaultPackage "MSP") -Recurse -Force

    New-Item -ItemType Directory -Force -Path (Join-Path $defaultPackage ".msp") | Out-Null
    Set-Content -LiteralPath (Join-Path $defaultPackage ".msp\state.json") -Value "private" -NoNewline
    Assert-Fails {
        & $packageVerifierPath -PackageRoot $defaultPackage -ArtifactsRoot $artifactsRoot
    } "private MSP state in package"
    Remove-Item -LiteralPath (Join-Path $defaultPackage ".msp") -Recurse -Force

    $tamperedMetadataPath = Join-Path $fixtureRoot "msp_ffi-invalid-algorithm.json"
    $tamperedMetadata = Get-Content -LiteralPath (Join-Path $ffiRoot "release-metadata.json") -Raw | ConvertFrom-Json
    $tamperedMetadata.hashes.algorithm = "MD5"
    $tamperedMetadata | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $tamperedMetadataPath -Encoding UTF8
    Assert-Fails {
        & $ffiVerifierPath -DllPath $ffiDllPath -MetadataPath $tamperedMetadataPath
    } "non-SHA-256 FFI metadata"

    $tamperedDllPath = Join-Path $fixtureRoot "msp_ffi-tampered.dll"
    $tamperedBytes = [System.IO.File]::ReadAllBytes($ffiDllPath)
    $tamperedBytes[$tamperedBytes.Length - 1] = $tamperedBytes[$tamperedBytes.Length - 1] -bxor 0x01
    [System.IO.File]::WriteAllBytes($tamperedDllPath, $tamperedBytes)
    Assert-Fails {
        & $ffiVerifierPath -DllPath $tamperedDllPath
    } "tampered FFI DLL hash"

    Write-Host "Focused package verification tests passed."
}
finally {
    if (Test-Path -LiteralPath $fixtureRoot) {
        Remove-Item -LiteralPath $fixtureRoot -Recurse -Force
    }
}
