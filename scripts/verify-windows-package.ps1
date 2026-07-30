[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $PackageRoot,

    [Parameter(Mandatory)]
    [string] $ArtifactsRoot,

    [switch] $RequireNativeMsp
)

$ErrorActionPreference = "Stop"

function Get-FullPath {
    param(
        [Parameter(Mandatory)]
        [string] $Path
    )

    return [System.IO.Path]::GetFullPath($Path)
}

function Assert-UnderDirectory {
    param(
        [Parameter(Mandatory)]
        [string] $Path,

        [Parameter(Mandatory)]
        [string] $Directory
    )

    $fullPath = Get-FullPath -Path $Path
    $fullDirectory = (Get-FullPath -Path $Directory).TrimEnd(
        [System.IO.Path]::DirectorySeparatorChar,
        [System.IO.Path]::AltDirectorySeparatorChar)
    $directoryPrefix = $fullDirectory + [System.IO.Path]::DirectorySeparatorChar
    if (-not $fullPath.StartsWith($directoryPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to inspect a package outside artifacts: $fullPath"
    }

    return $fullPath
}

function Assert-RequiredFile {
    param(
        [Parameter(Mandatory)]
        [string] $RelativePath
    )

    $path = Join-Path $PackageRoot $RelativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Windows package is missing required file: $RelativePath"
    }

    if ((Get-Item -LiteralPath $path).Length -le 0) {
        throw "Windows package contains an empty required file: $RelativePath"
    }
}

$ArtifactsRoot = Get-FullPath -Path $ArtifactsRoot
$PackageRoot = Assert-UnderDirectory -Path $PackageRoot -Directory $ArtifactsRoot
if (-not (Test-Path -LiteralPath $PackageRoot -PathType Container)) {
    throw "Windows package directory was not found: $PackageRoot"
}

Assert-RequiredFile -RelativePath "ReadOS.App.exe"
Assert-RequiredFile -RelativePath "ReadOS.App.dll"
Assert-RequiredFile -RelativePath "RELEASE.txt"
Assert-RequiredFile -RelativePath "README.md"

$forbiddenDirectoryNames = @(".git", "MSP", "credentials")
$forbiddenFileNames = @(
    ".env",
    "secrets.json",
    "workspace.json",
    "reados-package-smoke.success.json",
    "reados-package-smoke.log.json"
)

$forbiddenEntries = Get-ChildItem -LiteralPath $PackageRoot -Recurse -Force | Where-Object {
    if ($_.PSIsContainer) {
        return $forbiddenDirectoryNames -contains $_.Name
    }

    return ($forbiddenFileNames -contains $_.Name) -or
        [string]::Equals($_.Extension, ".pdb", [System.StringComparison]::OrdinalIgnoreCase)
}

if ($forbiddenEntries) {
    $relativePaths = $forbiddenEntries | ForEach-Object {
        [System.IO.Path]::GetRelativePath($PackageRoot, $_.FullName)
    }
    throw "Windows package contains private or source-only entries: $($relativePaths -join ', ')"
}

if ($RequireNativeMsp) {
    Assert-RequiredFile -RelativePath "msp_core.dll"
    Assert-RequiredFile -RelativePath "licenses\msp-upstream\APACHE-2.0.txt"
    Assert-RequiredFile -RelativePath "licenses\msp-upstream\NOTICE"
    Assert-RequiredFile -RelativePath "licenses\msp-upstream\SOURCE-PROVENANCE.md"

    & (Join-Path $PSScriptRoot "verify-msp-native-binary.ps1") `
        -DllPath (Join-Path $PackageRoot "msp_core.dll")
}

Write-Host "Windows package content verification passed."
Write-Host "  Package: $PackageRoot"
Write-Host "  Native MSP required: $RequireNativeMsp"
