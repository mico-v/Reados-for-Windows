[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $DllPath,

    [string] $ManifestPath,

    [string] $HeaderPath,

    [string] $ExportDefinitionPath,

    [string] $MetadataPath
)

$ErrorActionPreference = "Stop"

function Get-FullPath {
    param(
        [Parameter(Mandatory)]
        [string] $Path
    )

    return [System.IO.Path]::GetFullPath($Path)
}

function Assert-File {
    param(
        [Parameter(Mandatory)]
        [string] $Path,

        [Parameter(Mandatory)]
        [string] $Description
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "$Description was not found: $Path"
    }

    if ((Get-Item -LiteralPath $Path).Length -le 0) {
        throw "$Description is empty: $Path"
    }
}

function Read-U16 {
    param(
        [Parameter(Mandatory)]
        [byte[]] $Bytes,

        [Parameter(Mandatory)]
        [int] $Offset
    )

    if ($Offset -lt 0 -or $Offset + 2 -gt $Bytes.Length) {
        throw "PE field is outside the DLL: offset $Offset"
    }

    return [System.BitConverter]::ToUInt16($Bytes, $Offset)
}

function Read-U32 {
    param(
        [Parameter(Mandatory)]
        [byte[]] $Bytes,

        [Parameter(Mandatory)]
        [int] $Offset
    )

    if ($Offset -lt 0 -or $Offset + 4 -gt $Bytes.Length) {
        throw "PE field is outside the DLL: offset $Offset"
    }

    return [System.BitConverter]::ToUInt32($Bytes, $Offset)
}

function Convert-RvaToFileOffset {
    param(
        [Parameter(Mandatory)]
        [uint32] $Rva,

        [Parameter(Mandatory)]
        [object[]] $Sections,

        [Parameter(Mandatory)]
        [byte[]] $Bytes
    )

    foreach ($section in $Sections) {
        $sectionStart = [uint64] $section.VirtualAddress
        $sectionEnd = $sectionStart + [uint64] $section.MappedSize
        if ([uint64] $Rva -ge $sectionStart -and [uint64] $Rva -lt $sectionEnd) {
            $delta = [uint64] $Rva - $sectionStart
            if ($delta -ge [uint64] $section.RawSize) {
                throw "PE RVA 0x$('{0:X8}' -f $Rva) points beyond raw section data."
            }

            $offset = [uint64] $section.RawPointer + $delta
            if ($offset -ge [uint64] $Bytes.Length) {
                throw "PE RVA 0x$('{0:X8}' -f $Rva) maps beyond the DLL."
            }

            return [int] $offset
        }
    }

    throw "PE RVA 0x$('{0:X8}' -f $Rva) does not map to a section."
}

function Read-AsciiStringAtRva {
    param(
        [Parameter(Mandatory)]
        [uint32] $Rva,

        [Parameter(Mandatory)]
        [object[]] $Sections,

        [Parameter(Mandatory)]
        [byte[]] $Bytes
    )

    $offset = Convert-RvaToFileOffset -Rva $Rva -Sections $Sections -Bytes $Bytes
    $end = $offset
    while ($end -lt $Bytes.Length -and $Bytes[$end] -ne 0) {
        $end++
    }

    if ($end -ge $Bytes.Length) {
        throw "PE string at RVA 0x$('{0:X8}' -f $Rva) is not NUL-terminated."
    }

    return [System.Text.Encoding]::ASCII.GetString($Bytes, $offset, $end - $offset)
}

function Assert-Hash {
    param(
        [Parameter(Mandatory)]
        [string] $Path,

        [Parameter(Mandatory)]
        [string] $Expected,

        [Parameter(Mandatory)]
        [string] $Description
    )

    if ($Expected -notmatch '^[0-9a-fA-F]{64}$') {
        throw "$Description hash in release metadata is not a SHA-256 value: $Expected"
    }

    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        $digest = $sha256.ComputeHash([System.IO.File]::ReadAllBytes($Path))
        $actual = ([System.BitConverter]::ToString($digest) -replace '-', '').ToLowerInvariant()
    }
    finally {
        $sha256.Dispose()
    }
    if ($actual -ne $Expected.ToLowerInvariant()) {
        throw "$Description SHA-256 mismatch. Expected $Expected, observed $actual."
    }

    return $actual
}

$repoRoot = Get-FullPath -Path (Join-Path $PSScriptRoot "..")
$ffiRoot = Join-Path $repoRoot "native\msp-ffi"
if ([string]::IsNullOrWhiteSpace($ManifestPath)) {
    $ManifestPath = Join-Path $ffiRoot "Cargo.toml"
}
if ([string]::IsNullOrWhiteSpace($HeaderPath)) {
    $HeaderPath = Join-Path $ffiRoot "include\msp_ffi.h"
}
if ([string]::IsNullOrWhiteSpace($ExportDefinitionPath)) {
    $ExportDefinitionPath = Join-Path $ffiRoot "exports\msp_ffi.def"
}
if ([string]::IsNullOrWhiteSpace($MetadataPath)) {
    $MetadataPath = Join-Path $ffiRoot "release-metadata.json"
}

$DllPath = Get-FullPath -Path $DllPath
$ManifestPath = Get-FullPath -Path $ManifestPath
$HeaderPath = Get-FullPath -Path $HeaderPath
$ExportDefinitionPath = Get-FullPath -Path $ExportDefinitionPath
$MetadataPath = Get-FullPath -Path $MetadataPath

Assert-File -Path $DllPath -Description "Public MSP FFI DLL"
Assert-File -Path $ManifestPath -Description "MSP FFI Cargo manifest"
Assert-File -Path $HeaderPath -Description "MSP FFI public header"
Assert-File -Path $ExportDefinitionPath -Description "MSP FFI export definition"
Assert-File -Path $MetadataPath -Description "MSP FFI release metadata"

try {
    $metadata = Get-Content -LiteralPath $MetadataPath -Raw | ConvertFrom-Json
}
catch {
    throw "MSP FFI release metadata is not valid JSON: $MetadataPath`n$($_.Exception.Message)"
}

if ([int] $metadata.schema_version -ne 1) {
    throw "Unsupported MSP FFI release metadata schema: $($metadata.schema_version)"
}

$requiredMetadataFields = @(
    "artifact_name",
    "manifest_path",
    "header_path",
    "export_definition_path",
    "package_path",
    "header_package_path",
    "package_option",
    "profile",
    "target",
    "machine",
    "abi_version",
    "header_version",
    "export_count",
    "expected_exports",
    "hashes",
    "crt",
    "build_flags",
    "provenance"
)
foreach ($field in $requiredMetadataFields) {
    $property = $metadata.PSObject.Properties[$field]
    if ($null -eq $property -or $null -eq $property.Value) {
        throw "MSP FFI release metadata is missing '$field'."
    }
}

if ([string] $metadata.artifact_name -cne "msp_ffi.dll") {
    throw "MSP FFI release metadata must name the public DLL msp_ffi.dll."
}
if ([string] $metadata.package_path -cne "msp_ffi.dll") {
    throw "MSP FFI release metadata must package the DLL at msp_ffi.dll."
}
if ([string] $metadata.header_package_path -cne "include/msp_ffi.h") {
    throw "MSP FFI release metadata must package the header at include/msp_ffi.h."
}
if ([string] $metadata.package_option -cne "IncludePublicMspFfi") {
    throw "MSP FFI release metadata must require the IncludePublicMspFfi package option."
}
if ([string] $metadata.hashes.algorithm -cne "SHA-256") {
    throw "MSP FFI release metadata must declare SHA-256 hashes."
}
$buildFlags = @($metadata.build_flags | ForEach-Object { [string] $_ })
foreach ($requiredBuildFlag in @(
        "-C target-feature=+crt-static",
        "-C link-arg=/Brepro")) {
    if ($buildFlags -notcontains $requiredBuildFlag) {
        throw "MSP FFI release metadata is missing required build flag: $requiredBuildFlag"
    }
}
if ($metadata.provenance.raw_msp_source_must_not_be_tracked_or_packaged -ne $true) {
    throw "MSP FFI release metadata must prohibit raw MSP source tracking and packaging."
}

if ([System.IO.Path]::GetFileName($DllPath) -cne [string] $metadata.artifact_name) {
    throw "The verified file must be named $($metadata.artifact_name): $DllPath"
}
if ([string] $metadata.profile -cne "release") {
    throw "MSP FFI release metadata must describe the release profile."
}
if ([string] $metadata.target -cne "x86_64-pc-windows-msvc") {
    throw "MSP FFI release metadata must target x86_64-pc-windows-msvc."
}
if ([string] $metadata.machine -cne "AMD64") {
    throw "MSP FFI release metadata has an unsupported machine value: $($metadata.machine)"
}
if ([string] $metadata.crt -cne "static-msvc") {
    throw "MSP FFI release metadata must require the static MSVC CRT."
}

$manifestText = [System.IO.File]::ReadAllText($ManifestPath)
if ($manifestText -notmatch '(?m)^\s*name\s*=\s*"msp-ffi"\s*$') {
    throw "The FFI build manifest does not identify the msp-ffi package: $ManifestPath"
}
if ($manifestText -notmatch '(?m)^\s*name\s*=\s*"msp_ffi"\s*$') {
    throw "The FFI build manifest does not identify the msp_ffi library: $ManifestPath"
}
if ([string] $metadata.manifest_path -ne "native/msp-ffi/Cargo.toml") {
    throw "Release metadata points at an unexpected FFI manifest: $($metadata.manifest_path)"
}
if ([string] $metadata.header_path -ne "native/msp-ffi/include/msp_ffi.h") {
    throw "Release metadata points at an unexpected FFI header: $($metadata.header_path)"
}
if ([string] $metadata.export_definition_path -ne "native/msp-ffi/exports/msp_ffi.def") {
    throw "Release metadata points at an unexpected FFI export definition: $($metadata.export_definition_path)"
}

$manifestHash = Assert-Hash -Path $ManifestPath -Expected ([string] $metadata.hashes.manifest) -Description "FFI manifest"
$headerHash = Assert-Hash -Path $HeaderPath -Expected ([string] $metadata.hashes.header) -Description "FFI header"
$definitionHash = Assert-Hash -Path $ExportDefinitionPath -Expected ([string] $metadata.hashes.export_definition) -Description "FFI export definition"
$dllHash = Assert-Hash -Path $DllPath -Expected ([string] $metadata.hashes.dll) -Description "FFI DLL"

$headerText = [System.IO.File]::ReadAllText($HeaderPath)
$abiMarker = "MSP_FFI_ABI_VERSION UINT32_C($([int] $metadata.abi_version))"
$versionMarker = 'MSP_FFI_HEADER_VERSION_STRING "' + [string] $metadata.header_version + '"'
if ($headerText.IndexOf($abiMarker, [System.StringComparison]::Ordinal) -lt 0) {
    throw "The FFI header does not declare ABI version $($metadata.abi_version)."
}
if ($headerText.IndexOf($versionMarker, [System.StringComparison]::Ordinal) -lt 0) {
    throw "The FFI header does not declare version $($metadata.header_version)."
}

$expectedExports = @($metadata.expected_exports | ForEach-Object { [string] $_ })
if ([int] $metadata.export_count -ne $expectedExports.Count -or $expectedExports.Count -ne 29) {
    throw "MSP FFI release metadata must define exactly 29 exports; found $($expectedExports.Count)."
}
if ($expectedExports.Count -ne (@($expectedExports | Sort-Object -Unique).Count)) {
    throw "MSP FFI release metadata contains duplicate export names."
}

$definitionLines = Get-Content -LiteralPath $ExportDefinitionPath
$readingExports = $false
$definitionExports = @()
foreach ($line in $definitionLines) {
    $trimmed = $line.Trim()
    if ($trimmed -eq "EXPORTS") {
        $readingExports = $true
        continue
    }
    if (-not $readingExports -or [string]::IsNullOrWhiteSpace($trimmed) -or $trimmed.StartsWith(";")) {
        continue
    }
    if ($trimmed -notmatch '^([A-Za-z_][A-Za-z0-9_]*)\s*(?:@[0-9]+)?$') {
        throw "Unsupported or malformed FFI export definition line: $line"
    }
    $definitionExports += $Matches[1]
}
if (-not $readingExports) {
    throw "The FFI export definition has no EXPORTS section."
}
if ($definitionExports.Count -ne $expectedExports.Count) {
    throw "The FFI export definition contains $($definitionExports.Count) exports; expected 29."
}
if ($definitionExports.Count -ne (@($definitionExports | Sort-Object -Unique).Count)) {
    throw "The FFI export definition contains duplicate exports."
}
foreach ($expectedExport in $expectedExports) {
    if ($definitionExports -notcontains $expectedExport) {
        throw "The FFI export definition is missing expected export: $expectedExport"
    }
}
foreach ($definitionExport in $definitionExports) {
    if ($expectedExports -notcontains $definitionExport) {
        throw "The FFI export definition contains an unexpected export: $definitionExport"
    }
}

$bytes = [System.IO.File]::ReadAllBytes($DllPath)
if ($bytes.Length -lt 512 -or $bytes[0] -ne 0x4D -or $bytes[1] -ne 0x5A) {
    throw "The FFI artifact is not a valid DOS/PE DLL: $DllPath"
}

$peOffset = Read-U32 -Bytes $bytes -Offset 0x3C
if ([uint64] $peOffset + 24 -gt [uint64] $bytes.Length) {
    throw "The PE header offset is outside the FFI DLL."
}
if ([System.Text.Encoding]::ASCII.GetString($bytes, [int] $peOffset, 4) -cne ("PE" + [char] 0 + [char] 0)) {
    throw "The FFI artifact does not contain a PE signature."
}

$fileHeaderOffset = [int] $peOffset + 4
$machine = Read-U16 -Bytes $bytes -Offset $fileHeaderOffset
$sectionCount = Read-U16 -Bytes $bytes -Offset ($fileHeaderOffset + 2)
$optionalHeaderSize = Read-U16 -Bytes $bytes -Offset ($fileHeaderOffset + 16)
$fileCharacteristics = Read-U16 -Bytes $bytes -Offset ($fileHeaderOffset + 18)
if ($machine -ne 0x8664) {
    throw "The FFI DLL is not AMD64 (machine 0x$('{0:X4}' -f $machine))."
}
if (($fileCharacteristics -band 0x2000) -eq 0) {
    throw "The FFI artifact is not marked as a DLL."
}
if ($sectionCount -le 0 -or $sectionCount -gt 96) {
    throw "The FFI PE section count is invalid: $sectionCount"
}

$optionalHeaderOffset = $fileHeaderOffset + 20
$optionalMagic = Read-U16 -Bytes $bytes -Offset $optionalHeaderOffset
if ($optionalMagic -ne 0x20B) {
    throw "The FFI DLL is not a PE32+ image."
}
if ($optionalHeaderSize -lt 240 -or [uint64] $optionalHeaderOffset + $optionalHeaderSize -gt [uint64] $bytes.Length) {
    throw "The FFI PE optional header is invalid."
}
$numberOfDirectories = Read-U32 -Bytes $bytes -Offset ($optionalHeaderOffset + 108)
if ($numberOfDirectories -lt 2) {
    throw "The FFI PE image has no export/import data directories."
}
$dataDirectoryOffset = $optionalHeaderOffset + 112
$sectionTableOffset = $optionalHeaderOffset + $optionalHeaderSize
$sections = @()
for ($index = 0; $index -lt $sectionCount; $index++) {
    $sectionOffset = $sectionTableOffset + ($index * 40)
    if ([uint64] $sectionOffset + 40 -gt [uint64] $bytes.Length) {
        throw "The FFI PE section table is truncated."
    }
    $virtualSize = Read-U32 -Bytes $bytes -Offset ($sectionOffset + 8)
    $virtualAddress = Read-U32 -Bytes $bytes -Offset ($sectionOffset + 12)
    $rawSize = Read-U32 -Bytes $bytes -Offset ($sectionOffset + 16)
    $rawPointer = Read-U32 -Bytes $bytes -Offset ($sectionOffset + 20)
    if ($rawSize -gt 0 -and ([uint64] $rawPointer + $rawSize -gt [uint64] $bytes.Length)) {
        throw "The FFI PE section points outside the file."
    }
    $mappedSize = [Math]::Max([uint32] $virtualSize, [uint32] $rawSize)
    if ($mappedSize -gt 0) {
        $sections += [pscustomobject] @{
            VirtualAddress = $virtualAddress
            MappedSize = $mappedSize
            RawSize = $rawSize
            RawPointer = $rawPointer
        }
    }
}

$exportDirectoryRva = Read-U32 -Bytes $bytes -Offset $dataDirectoryOffset
$exportDirectorySize = Read-U32 -Bytes $bytes -Offset ($dataDirectoryOffset + 4)
if ($exportDirectoryRva -eq 0 -or $exportDirectorySize -eq 0) {
    throw "The FFI DLL has no PE export directory."
}
$exportDirectoryOffset = Convert-RvaToFileOffset -Rva $exportDirectoryRva -Sections $sections -Bytes $bytes
if ([uint64] $exportDirectoryOffset + 40 -gt [uint64] $bytes.Length) {
    throw "The FFI PE export directory is truncated."
}
$exportBase = Read-U32 -Bytes $bytes -Offset ($exportDirectoryOffset + 16)
$functionCount = Read-U32 -Bytes $bytes -Offset ($exportDirectoryOffset + 20)
$nameCount = Read-U32 -Bytes $bytes -Offset ($exportDirectoryOffset + 24)
$functionsRva = Read-U32 -Bytes $bytes -Offset ($exportDirectoryOffset + 28)
$namesRva = Read-U32 -Bytes $bytes -Offset ($exportDirectoryOffset + 32)
$ordinalsRva = Read-U32 -Bytes $bytes -Offset ($exportDirectoryOffset + 36)
if ($functionCount -ne 29 -or $nameCount -ne 29) {
    throw "The FFI DLL PE export directory contains $nameCount named/$functionCount total exports; expected 29/29."
}

$actualExports = @()
for ($index = 0; $index -lt $nameCount; $index++) {
    $nameRva = Read-U32 -Bytes $bytes -Offset ((Convert-RvaToFileOffset -Rva $namesRva -Sections $sections -Bytes $bytes) + ($index * 4))
    $actualExports += Read-AsciiStringAtRva -Rva $nameRva -Sections $sections -Bytes $bytes
}
if ($actualExports.Count -ne (@($actualExports | Sort-Object -Unique).Count)) {
    throw "The FFI DLL PE export directory contains duplicate names."
}
foreach ($expectedExport in $expectedExports) {
    if ($actualExports -notcontains $expectedExport) {
        throw "The FFI DLL is missing expected PE export: $expectedExport"
    }
}
foreach ($actualExport in $actualExports) {
    if ($expectedExports -notcontains $actualExport) {
        throw "The FFI DLL contains an unexpected PE export: $actualExport"
    }
}

$functionTableOffset = Convert-RvaToFileOffset -Rva $functionsRva -Sections $sections -Bytes $bytes
$exportRangeEnd = [uint64] $exportDirectoryRva + [uint64] $exportDirectorySize
for ($index = 0; $index -lt $functionCount; $index++) {
    $functionRva = Read-U32 -Bytes $bytes -Offset ($functionTableOffset + ($index * 4))
    if ($functionRva -eq 0) {
        throw "The FFI DLL has an export with no function address."
    }
    if ([uint64] $functionRva -ge [uint64] $exportDirectoryRva -and [uint64] $functionRva -lt $exportRangeEnd) {
        throw "The FFI DLL contains a forwarded export; only native functions are allowed."
    }
}

$importDirectoryRva = Read-U32 -Bytes $bytes -Offset ($dataDirectoryOffset + 8)
$importDirectorySize = Read-U32 -Bytes $bytes -Offset ($dataDirectoryOffset + 12)
if ($importDirectoryRva -eq 0 -or $importDirectorySize -eq 0) {
    throw "The FFI DLL has no PE import directory; static-CRT verification cannot complete."
}
$importDirectoryOffset = Convert-RvaToFileOffset -Rva $importDirectoryRva -Sections $sections -Bytes $bytes
$importNames = @()
$terminatedImportTable = $false
for ($index = 0; $index -lt 4096; $index++) {
    $descriptorOffset = $importDirectoryOffset + ($index * 20)
    if ([uint64] $descriptorOffset + 20 -gt [uint64] $bytes.Length) {
        throw "The FFI PE import directory is truncated."
    }
    $originalFirstThunk = Read-U32 -Bytes $bytes -Offset $descriptorOffset
    $timeDateStamp = Read-U32 -Bytes $bytes -Offset ($descriptorOffset + 4)
    $forwarderChain = Read-U32 -Bytes $bytes -Offset ($descriptorOffset + 8)
    $nameRva = Read-U32 -Bytes $bytes -Offset ($descriptorOffset + 12)
    $firstThunk = Read-U32 -Bytes $bytes -Offset ($descriptorOffset + 16)
    if ($originalFirstThunk -eq 0 -and $timeDateStamp -eq 0 -and $forwarderChain -eq 0 -and $nameRva -eq 0 -and $firstThunk -eq 0) {
        $terminatedImportTable = $true
        break
    }
    if ($nameRva -eq 0) {
        throw "The FFI PE import directory contains an unnamed dependency."
    }
    $importNames += Read-AsciiStringAtRva -Rva $nameRva -Sections $sections -Bytes $bytes
}
if (-not $terminatedImportTable) {
    throw "The FFI PE import directory has no terminator."
}
$dynamicCrtImport = $importNames | Where-Object {
    $_ -match '^(?i:VCRUNTIME|MSVCP|CONCRT|ucrtbase\.dll$|api-ms-win-crt-)'
} | Select-Object -First 1
if ($dynamicCrtImport) {
    throw "The FFI DLL imports the dynamic CRT: $dynamicCrtImport"
}

$binaryText = [System.Text.Encoding]::ASCII.GetString($bytes)
$dynamicCrtMarker = @(
    "VCRUNTIME",
    "MSVCP140",
    "CONCRT140",
    "api-ms-win-crt-",
    "ucrtbase.dll"
) | Where-Object {
    $binaryText.IndexOf($_, [System.StringComparison]::OrdinalIgnoreCase) -ge 0
} | Select-Object -First 1
if ($dynamicCrtMarker) {
    throw "The FFI DLL contains a dynamic CRT marker: $dynamicCrtMarker"
}

Write-Host "Public MSP FFI release verification passed."
Write-Host "  DLL: $DllPath"
Write-Host "  Manifest: $ManifestPath (SHA-256 $manifestHash)"
Write-Host "  Header: $HeaderPath (SHA-256 $headerHash)"
Write-Host "  Exports: 29"
Write-Host "  DLL SHA-256: $dllHash"
Write-Host "  Imports: $($importNames -join ', ')"
Write-Host "  CRT: static MSVC"
