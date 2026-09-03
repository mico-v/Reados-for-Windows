[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $PackageRoot,

    [Parameter(Mandatory)]
    [string] $ArtifactsRoot,

    [switch] $RequireNativeMsp,

    [switch] $RequireCommandRuntimeFfi,

    [switch] $RequirePublicMspFfi
)

$ErrorActionPreference = "Stop"

function Get-FullPath {
    param(
        [Parameter(Mandatory)]
        [string] $Path
    )

    return [System.IO.Path]::GetFullPath($Path)
}

function Get-RelativePath {
    param(
        [Parameter(Mandatory)]
        [string] $BasePath,

        [Parameter(Mandatory)]
        [string] $Path
    )

    $fullBasePath = (Get-FullPath -Path $BasePath).TrimEnd(
        [System.IO.Path]::DirectorySeparatorChar,
        [System.IO.Path]::AltDirectorySeparatorChar)
    $fullPath = Get-FullPath -Path $Path
    $prefix = $fullBasePath + [System.IO.Path]::DirectorySeparatorChar
    if ($fullPath.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $fullPath.Substring($prefix.Length)
    }

    if ([string]::Equals(
        $fullPath,
        $fullBasePath,
        [System.StringComparison]::OrdinalIgnoreCase)) {
        return [string]::Empty
    }

    return $fullPath
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

$forbiddenDirectoryNames = @(
    ".git",
    ".cargo",
    ".msp",
    "MSP",
    "credentials",
    "private",
    "state",
    "source",
    "sources",
    "tests",
    "test",
    "target",
    "obj",
    "bin",
    "build",
    "incremental",
    "src"
)
$forbiddenFileNames = @(
    ".env",
    ".gitignore",
    ".gitattributes",
    ".editorconfig",
    "secrets.json",
    "credentials.json",
    "private.json",
    "state.json",
    "workspace.json",
    "reados-package-smoke.success.json",
    "reados-package-smoke.log.json",
    "Cargo.toml",
    "Cargo.lock",
    "build.rs",
    "msp_ffi.def"
)
$forbiddenSourceExtensions = @(
    ".bat",
    ".bash",
    ".c",
    ".cc",
    ".cmd",
    ".cpp",
    ".cxx",
    ".cs",
    ".csproj",
    ".def",
    ".d",
    ".fs",
    ".fsproj",
    ".h",
    ".hh",
    ".hpp",
    ".ilk",
    ".lock",
    ".m",
    ".mm",
    ".obj",
    ".ps1",
    ".psd1",
    ".psm1",
    ".py",
    ".pyc",
    ".rlib",
    ".rmeta",
    ".rs",
    ".sh",
    ".sln",
    ".swift",
    ".toml",
    ".ts",
    ".tsx",
    ".vb",
    ".vbproj",
    ".xaml"
)
$forbiddenGeneratedExtensions = @(
    ".exp",
    ".lib",
    ".pdb"
)
$allowedPublicFfiHeader = "include\msp_ffi.h"
$ffiVerifierPath = Join-Path $PSScriptRoot "verify-msp-ffi-release.ps1"
$ffiRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\native\msp-ffi"))
$ffiManifestPath = Join-Path $ffiRoot "Cargo.toml"
$ffiExportDefinitionPath = Join-Path $ffiRoot "exports\msp_ffi.def"
$ffiMetadataPath = Join-Path $ffiRoot "release-metadata.json"
$runtimeFfiVerifierPath = Join-Path $PSScriptRoot "verify-msp-command-runtime-ffi.ps1"
$runtimeFfiDllPath = Join-Path $PackageRoot "msp_command_runtime_ffi.dll"

$forbiddenEntries = Get-ChildItem -LiteralPath $PackageRoot -Recurse -Force | Where-Object {
    if ($_.PSIsContainer) {
        return $forbiddenDirectoryNames -contains $_.Name
    }

    $relativePath = Get-RelativePath -BasePath $PackageRoot -Path $_.FullName
    $normalizedRelativePath = $relativePath.Replace('/', '\\')
    if ($RequirePublicMspFfi -and
        [string]::Equals($normalizedRelativePath, $allowedPublicFfiHeader, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $false
    }

    return ($forbiddenFileNames -contains $_.Name) -or
        ($forbiddenSourceExtensions -contains $_.Extension.ToLowerInvariant()) -or
        ($forbiddenGeneratedExtensions -contains $_.Extension.ToLowerInvariant()) -or
        $_.Name -match '(?i)^(?:\.env(?:\..*)?|(?:credentials?|secrets?|private|state|workspace)(?:[._-].*)?)$' -or
        $_.Name -match '(?i)\.(?:generated|gen|temporary|tmp)(?:[._-]|$)'
}

if ($forbiddenEntries) {
    $relativePaths = $forbiddenEntries | ForEach-Object {
        Get-RelativePath -BasePath $PackageRoot -Path $_.FullName
    }
    throw "Windows package contains private, source-only, or generated entries: $($relativePaths -join ', ')"
}

$publicMspFfiDllPath = Join-Path $PackageRoot "msp_ffi.dll"
$publicMspFfiHeaderPath = Join-Path $PackageRoot $allowedPublicFfiHeader
$publicMspFfiEntries = @(Get-ChildItem -LiteralPath $PackageRoot -Recurse -Force | Where-Object {
    if ($_.PSIsContainer) {
        return $false
    }

    return [string]::Equals($_.Name, "msp_ffi.dll", [System.StringComparison]::OrdinalIgnoreCase) -or
        [string]::Equals($_.Name, "msp_ffi.h", [System.StringComparison]::OrdinalIgnoreCase)
})
$unexpectedPublicMspFfiEntries = @($publicMspFfiEntries | Where-Object {
    $relativePath = (Get-RelativePath -BasePath $PackageRoot -Path $_.FullName).Replace('/', '\\')
    -not ([string]::Equals($relativePath, "msp_ffi.dll", [System.StringComparison]::OrdinalIgnoreCase) -or
        [string]::Equals($relativePath, $allowedPublicFfiHeader, [System.StringComparison]::OrdinalIgnoreCase))
})
if ($unexpectedPublicMspFfiEntries) {
    $relativePaths = $unexpectedPublicMspFfiEntries | ForEach-Object {
        Get-RelativePath -BasePath $PackageRoot -Path $_.FullName
    }
    throw "Windows package contains a public MSP FFI file at an unapproved path: $($relativePaths -join ', ')"
}

$runtimeFfiEntries = @(Get-ChildItem -LiteralPath $PackageRoot -Recurse -Force | Where-Object {
    if ($_.PSIsContainer) {
        return $false
    }

    return [string]::Equals($_.Name, "msp_command_runtime_ffi.dll", [System.StringComparison]::OrdinalIgnoreCase)
})
$unexpectedRuntimeFfiEntries = @($runtimeFfiEntries | Where-Object {
    $relativePath = (Get-RelativePath -BasePath $PackageRoot -Path $_.FullName).Replace('/', '\\')
    -not [string]::Equals($relativePath, "msp_command_runtime_ffi.dll", [System.StringComparison]::OrdinalIgnoreCase)
})
if ($unexpectedRuntimeFfiEntries) {
    $relativePaths = $unexpectedRuntimeFfiEntries | ForEach-Object {
        Get-RelativePath -BasePath $PackageRoot -Path $_.FullName
    }
    throw "Windows package contains the command runtime FFI DLL at an unapproved path: $($relativePaths -join ', ')"
}

if ($RequireCommandRuntimeFfi) {
    Assert-RequiredFile -RelativePath "msp_command_runtime_ffi.dll"
    Assert-RequiredFile -RelativePath "licenses\msp-command-runtime-ffi\LICENSE-APACHE-2.0"
    Assert-RequiredFile -RelativePath "licenses\msp-command-runtime-ffi\NOTICE"
    & $runtimeFfiVerifierPath `
        -DllPath $runtimeFfiDllPath `
        -SkipBuild `
        -SkipContractSmoke
}
elseif ($runtimeFfiEntries.Count -gt 0) {
    throw "Windows package contains the optional command runtime FFI without -RequireCommandRuntimeFfi."
}
if ($RequirePublicMspFfi) {
    Assert-RequiredFile -RelativePath "msp_ffi.dll"
    Assert-RequiredFile -RelativePath $allowedPublicFfiHeader
    & $ffiVerifierPath `
        -DllPath $publicMspFfiDllPath `
        -ManifestPath $ffiManifestPath `
        -HeaderPath $publicMspFfiHeaderPath `
        -ExportDefinitionPath $ffiExportDefinitionPath `
        -MetadataPath $ffiMetadataPath
}
elseif ($publicMspFfiEntries.Count -gt 0) {
    throw "Windows package contains the optional public MSP FFI without -RequirePublicMspFfi."
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
Write-Host "  Public MSP FFI required: $RequirePublicMspFfi"
