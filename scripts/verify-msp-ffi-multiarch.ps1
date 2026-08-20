[CmdletBinding()]
param(
    [string] $EvidencePath,
    [switch] $Json,
    [switch] $RequireAvailable
)

$ErrorActionPreference = "Stop"

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrWhiteSpace($scriptRoot)) {
    $scriptRoot = (Get-Location).Path
}
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $scriptRoot ".."))
if ([string]::IsNullOrWhiteSpace($EvidencePath)) {
    $EvidencePath = Join-Path $repoRoot "conformance\msp-upstream\msp-ffi-multiarch-evidence.json"
}
$evidenceFullPath = [System.IO.Path]::GetFullPath($EvidencePath)

function Add-Unique {
    param(
        [Parameter(Mandatory)]
        [AllowEmptyCollection()]
        [System.Collections.Generic.List[string]] $List,

        [Parameter(Mandatory)]
        [string] $Value
    )

    if (-not [string]::IsNullOrWhiteSpace($Value) -and -not $List.Contains($Value)) {
        $List.Add($Value)
    }
}

function Assert-RelativePath {
    param(
        [Parameter(Mandatory)]
        [string] $RelativePath,

        [Parameter(Mandatory)]
        [string] $Field
    )

    if ([string]::IsNullOrWhiteSpace($RelativePath)) {
        throw "Evidence field is empty: $Field"
    }
    if ([System.IO.Path]::IsPathRooted($RelativePath) -or
        $RelativePath -match '(^|[\\/])\.\.([\\/]|$)') {
        throw "Evidence path is not repository-relative: $Field=$RelativePath"
    }
}

function Resolve-RepoPath {
    param(
        [Parameter(Mandatory)]
        [string] $RelativePath,

        [Parameter(Mandatory)]
        [string] $Field
    )

    Assert-RelativePath -RelativePath $RelativePath -Field $Field
    return [System.IO.Path]::GetFullPath((Join-Path $repoRoot ($RelativePath.Replace('/', [System.IO.Path]::DirectorySeparatorChar))))
}

function Get-ExecutablePath {
    param(
        [Parameter(Mandatory)]
        [string] $Name
    )

    $command = Get-Command -Name $Name -ErrorAction SilentlyContinue
    if ($null -eq $command) {
        return $null
    }
    if ($command.PSObject.Properties.Name -contains "Source" -and -not [string]::IsNullOrWhiteSpace([string] $command.Source)) {
        return [string] $command.Source
    }
    if ($command.PSObject.Properties.Name -contains "Path" -and -not [string]::IsNullOrWhiteSpace([string] $command.Path)) {
        return [string] $command.Path
    }
    return [string] $command.Name
}

function Read-U16 {
    param(
        [Parameter(Mandatory)]
        [byte[]] $Bytes,

        [Parameter(Mandatory)]
        [int] $Offset
    )

    if ($Offset -lt 0 -or [uint64] $Offset + 2 -gt [uint64] $Bytes.Length) {
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

    if ($Offset -lt 0 -or [uint64] $Offset + 4 -gt [uint64] $Bytes.Length) {
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

function Get-PeImage {
    param(
        [Parameter(Mandatory)]
        [string] $DllPath,

        [Parameter(Mandatory)]
        [int] $ExpectedExportCount,

        [Parameter(Mandatory)]
        [string[]] $ExpectedExports
    )

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
    if (($fileCharacteristics -band 0x2000) -eq 0) {
        throw "The FFI artifact is not marked as a DLL."
    }
    if ($sectionCount -le 0 -or $sectionCount -gt 96) {
        throw "The FFI PE section count is invalid: $sectionCount"
    }

    $optionalHeaderOffset = $fileHeaderOffset + 20
    $optionalMagic = Read-U16 -Bytes $bytes -Offset $optionalHeaderOffset
    if ($optionalMagic -eq 0x10B) {
        $numberOfDirectoriesOffset = 92
        $dataDirectoryRelativeOffset = 96
        $minimumOptionalHeaderSize = 224
    }
    elseif ($optionalMagic -eq 0x20B) {
        $numberOfDirectoriesOffset = 108
        $dataDirectoryRelativeOffset = 112
        $minimumOptionalHeaderSize = 240
    }
    else {
        throw "The FFI PE optional header has an unsupported magic value: 0x$('{0:X4}' -f $optionalMagic)"
    }
    if ($optionalHeaderSize -lt $minimumOptionalHeaderSize -or
        [uint64] $optionalHeaderOffset + $optionalHeaderSize -gt [uint64] $bytes.Length) {
        throw "The FFI PE optional header is invalid."
    }

    $numberOfDirectories = Read-U32 -Bytes $bytes -Offset ($optionalHeaderOffset + $numberOfDirectoriesOffset)
    if ($numberOfDirectories -lt 2) {
        throw "The FFI PE image has no export/import data directories."
    }
    $dataDirectoryOffset = $optionalHeaderOffset + $dataDirectoryRelativeOffset
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

    $functionCount = Read-U32 -Bytes $bytes -Offset ($exportDirectoryOffset + 20)
    $nameCount = Read-U32 -Bytes $bytes -Offset ($exportDirectoryOffset + 24)
    $functionsRva = Read-U32 -Bytes $bytes -Offset ($exportDirectoryOffset + 28)
    $namesRva = Read-U32 -Bytes $bytes -Offset ($exportDirectoryOffset + 32)
    if ($functionCount -ne $ExpectedExportCount -or $nameCount -ne $ExpectedExportCount) {
        throw "The FFI DLL PE export directory contains $nameCount named/$functionCount total exports; expected $ExpectedExportCount/$ExpectedExportCount."
    }

    $actualExports = @()
    $namesOffset = Convert-RvaToFileOffset -Rva $namesRva -Sections $sections -Bytes $bytes
    for ($index = 0; $index -lt $nameCount; $index++) {
        $nameRva = Read-U32 -Bytes $bytes -Offset ($namesOffset + ($index * 4))
        $actualExports += Read-AsciiStringAtRva -Rva $nameRva -Sections $sections -Bytes $bytes
    }
    if ($actualExports.Count -ne (@($actualExports | Sort-Object -Unique).Count)) {
        throw "The FFI DLL PE export directory contains duplicate names."
    }
    foreach ($expectedExport in $ExpectedExports) {
        if ($actualExports -notcontains $expectedExport) {
            throw "The FFI DLL is missing expected PE export: $expectedExport"
        }
    }
    foreach ($actualExport in $actualExports) {
        if ($ExpectedExports -notcontains $actualExport) {
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

    return [pscustomobject] @{
        machine_value = ("0x{0:X4}" -f $machine)
        optional_magic = ("0x{0:X}" -f $optionalMagic)
        export_count = [int] $functionCount
        exports = @($actualExports | Sort-Object)
        imports = @($importNames | Sort-Object)
        sha256 = (Get-FileHash -LiteralPath $DllPath -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

if (-not (Test-Path -LiteralPath $evidenceFullPath -PathType Leaf)) {
    throw "Public MSP FFI multiarch evidence record was not found: $evidenceFullPath"
}

$evidence = Get-Content -LiteralPath $evidenceFullPath -Raw | ConvertFrom-Json
if ([int] $evidence.schema_version -ne 1) {
    throw "Unsupported public MSP FFI multiarch evidence schema: $($evidence.schema_version)"
}
if ([string] $evidence.evidence_kind -ne "reados-public-msp-ffi-multiarch") {
    throw "Unexpected public MSP FFI multiarch evidence kind: $($evidence.evidence_kind)"
}

$missingPaths = New-Object 'System.Collections.Generic.List[string]'
$missingCommands = New-Object 'System.Collections.Generic.List[string]'
$notes = New-Object 'System.Collections.Generic.List[string]'

$targetDefinitions = @(
    [pscustomobject] @{
        runtime = "win-x64"
        rust_target = "x86_64-pc-windows-msvc"
        machine = "AMD64"
        machine_value = "0x8664"
        optional_magic = "0x20B"
        dll_path = "native/msp-ffi/target/release/msp_ffi.dll"
        build_command = "cargo build --release --locked --manifest-path native/msp-ffi/Cargo.toml --target x86_64-pc-windows-msvc"
    },
    [pscustomobject] @{
        runtime = "win-x86"
        rust_target = "i686-pc-windows-msvc"
        machine = "I386"
        machine_value = "0x014C"
        optional_magic = "0x10B"
        dll_path = "native/msp-ffi/target/i686-pc-windows-msvc/release/msp_ffi.dll"
        build_command = "cargo build --release --locked --manifest-path native/msp-ffi/Cargo.toml --target i686-pc-windows-msvc"
    },
    [pscustomobject] @{
        runtime = "win-arm64"
        rust_target = "aarch64-pc-windows-msvc"
        machine = "ARM64"
        machine_value = "0xAA64"
        optional_magic = "0x20B"
        dll_path = "native/msp-ffi/target/aarch64-pc-windows-msvc/release/msp_ffi.dll"
        build_command = "cargo build --release --locked --manifest-path native/msp-ffi/Cargo.toml --target aarch64-pc-windows-msvc"
    }
)

$source = $evidence.source
$sourceFields = @(
    @{ name = "manifest_path"; key = "manifest" },
    @{ name = "header_path"; key = "header" },
    @{ name = "export_definition_path"; key = "export_definition" },
    @{ name = "metadata_path"; key = $null }
)
foreach ($field in $sourceFields) {
    $value = [string] $source.($field.name)
    Assert-RelativePath -RelativePath $value -Field "source.$($field.name)"
}

$sourceMetadataPath = Resolve-RepoPath -RelativePath ([string] $source.metadata_path) -Field "source.metadata_path"
$sourceMetadata = $null
if (-not (Test-Path -LiteralPath $sourceMetadataPath -PathType Leaf)) {
    Add-Unique -List $missingPaths -Value ([string] $source.metadata_path)
}
else {
    try {
        $sourceMetadata = Get-Content -LiteralPath $sourceMetadataPath -Raw | ConvertFrom-Json
    }
    catch {
        throw "Public MSP FFI release metadata is not valid JSON: $sourceMetadataPath`n$($_.Exception.Message)"
    }
}

$expectedExports = @()
$expectedExportCount = 0
if ($null -ne $sourceMetadata) {
    if ([int] $sourceMetadata.schema_version -ne 1) {
        throw "Unsupported MSP FFI release metadata schema: $($sourceMetadata.schema_version)"
    }
    $expectedExports = @($sourceMetadata.expected_exports | ForEach-Object { [string] $_ })
    $expectedExportCount = [int] $sourceMetadata.export_count
    if ($expectedExportCount -ne 29 -or $expectedExports.Count -ne 29 -or
        $expectedExports.Count -ne (@($expectedExports | Sort-Object -Unique).Count)) {
        throw "MSP FFI release metadata must define exactly 29 unique exports."
    }
}

$sourceHashes = [ordered]@{}
foreach ($field in $sourceFields | Where-Object { $null -ne $_.key }) {
    $relativePath = [string] $source.($field.name)
    $fullPath = Resolve-RepoPath -RelativePath $relativePath -Field "source.$($field.name)"
    $expectedHash = $null
    if ($null -ne $sourceMetadata) {
        $expectedHash = [string] $sourceMetadata.hashes.($field.key)
    }
    if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) {
        Add-Unique -List $missingPaths -Value $relativePath
        $sourceHashes[$field.key] = $null
        continue
    }
    if ($expectedHash -notmatch '^[0-9a-fA-F]{64}$') {
        throw "MSP FFI release metadata has no valid SHA-256 for $($field.key)."
    }
    $actualHash = (Get-FileHash -LiteralPath $fullPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $expectedHash.ToLowerInvariant()) {
        throw "Source $($field.key) SHA-256 mismatch. Expected $expectedHash, observed $actualHash."
    }
    $sourceHashes[$field.key] = $actualHash
}

$rustupPath = Get-ExecutablePath -Name "rustup"
$installedTargets = New-Object 'System.Collections.Generic.List[string]'
if ($null -eq $rustupPath) {
    Add-Unique -List $missingCommands -Value "rustup target list --installed"
}
else {
    $rustupOutput = @(& $rustupPath target list --installed 2>&1)
    $rustupExitCode = $LASTEXITCODE
    if ($rustupExitCode -ne 0) {
        Add-Unique -List $missingCommands -Value "rustup target list --installed"
    }
    else {
        foreach ($line in $rustupOutput) {
            $targetName = ([string] $line).Trim()
            if ($targetName -match '^[a-z0-9_]+-pc-windows-msvc$' -and -not $installedTargets.Contains($targetName)) {
                $installedTargets.Add($targetName)
            }
        }
    }
}

$declaredTargets = @($evidence.targets)
if ($declaredTargets.Count -ne $targetDefinitions.Count) {
    throw "Public MSP FFI evidence must declare exactly $($targetDefinitions.Count) targets."
}

$targetResults = @()
foreach ($definition in $targetDefinitions) {
    $declared = @($declaredTargets | Where-Object { [string] $_.runtime -eq $definition.runtime })
    if ($declared.Count -ne 1) {
        throw "Public MSP FFI evidence must declare exactly one record for $($definition.runtime)."
    }
    $declared = $declared[0]
    if ([string] $declared.rust_target -cne $definition.rust_target -or
        [string] $declared.machine -cne $definition.machine -or
        ([string] $declared.machine_value).ToUpperInvariant() -ne $definition.machine_value.ToUpperInvariant() -or
        ([string] $declared.optional_magic).ToUpperInvariant() -ne $definition.optional_magic.ToUpperInvariant() -or
        [string] $declared.dll_path -cne $definition.dll_path) {
        throw "Public MSP FFI evidence mapping does not match the verifier mapping for $($definition.runtime)."
    }
    if (@("available", "blocked") -notcontains [string] $declared.status) {
        throw "Unexpected declared status for $($definition.runtime): $($declared.status)"
    }
    Assert-RelativePath -RelativePath $definition.dll_path -Field "$($definition.runtime).dll_path"

    $targetResult = [ordered]@{
        runtime = $definition.runtime
        rust_target = $definition.rust_target
        machine = $definition.machine
        machine_value = $definition.machine_value
        optional_magic = $definition.optional_magic
        declared_status = [string] $declared.status
        status = "blocked"
        toolchain_available = $installedTargets.Contains($definition.rust_target)
        dll_path = $definition.dll_path
        dll_exists = $false
        hashes = [ordered]@{
            dll = $null
            manifest = $null
            header = $null
            export_definition = $null
        }
        pe = $null
        missing_paths = @()
        missing_commands = @()
        notes = @()
    }
    $targetMissingPaths = New-Object 'System.Collections.Generic.List[string]'
    $targetMissingCommands = New-Object 'System.Collections.Generic.List[string]'
    $targetNotes = New-Object 'System.Collections.Generic.List[string]'

    if (-not $targetResult.toolchain_available) {
        Add-Unique -List $targetMissingCommands -Value "rustup target add $($definition.rust_target)"
        Add-Unique -List $missingCommands -Value "rustup target add $($definition.rust_target)"
    }

    $dllFullPath = Resolve-RepoPath -RelativePath $definition.dll_path -Field "$($definition.runtime).dll_path"
    if (-not (Test-Path -LiteralPath $dllFullPath -PathType Leaf)) {
        Add-Unique -List $targetMissingPaths -Value $definition.dll_path
        Add-Unique -List $missingPaths -Value $definition.dll_path
    }
    else {
        $targetResult.dll_exists = $true
    }

    $declaredHashes = $declared.hashes
    foreach ($hashField in @("manifest", "header", "export_definition")) {
        $declaredHash = [string] $declaredHashes.$hashField
        if ($declaredHash -notmatch '^[0-9a-fA-F]{64}$') {
            $targetNotes.Add("targets.$($definition.runtime).hashes.$hashField is not a SHA-256 value.")
            continue
        }
        if ($null -ne $sourceHashes[$hashField] -and $declaredHash.ToLowerInvariant() -ne [string] $sourceHashes[$hashField]) {
            throw "$($definition.runtime) $hashField SHA-256 does not match the observed source hash. Expected $($sourceHashes[$hashField]), declared $declaredHash."
        }
        $targetResult.hashes[$hashField] = $declaredHash.ToLowerInvariant()
    }

    $declaredDllHash = [string] $declaredHashes.dll
    if ($declaredDllHash -notmatch '^[0-9a-fA-F]{64}$') {
        $targetNotes.Add("targets.$($definition.runtime).hashes.dll is not an observed SHA-256 value; the target cannot pass without a target-specific DLL hash.")
    }

    if ($targetResult.dll_exists -and $targetResult.toolchain_available -and
        $declaredDllHash -match '^[0-9a-fA-F]{64}$' -and
        $null -ne $sourceMetadata -and $expectedExportCount -gt 0) {
        $pe = Get-PeImage -DllPath $dllFullPath -ExpectedExportCount $expectedExportCount -ExpectedExports $expectedExports
        if ($pe.machine_value.ToUpperInvariant() -ne $definition.machine_value.ToUpperInvariant()) {
            throw "$($definition.runtime) PE machine mismatch. Expected $($definition.machine_value), observed $($pe.machine_value)."
        }
        if ($pe.optional_magic.ToUpperInvariant() -ne $definition.optional_magic.ToUpperInvariant()) {
            throw "$($definition.runtime) PE optional-header mismatch. Expected $($definition.optional_magic), observed $($pe.optional_magic)."
        }
        if ($pe.sha256 -ne $declaredDllHash.ToLowerInvariant()) {
            throw "$($definition.runtime) DLL SHA-256 mismatch. Expected $declaredDllHash, observed $($pe.sha256)."
        }
        $targetResult.hashes.dll = $pe.sha256
        $targetResult.pe = [ordered]@{
            machine = $pe.machine_value
            optional_magic = $pe.optional_magic
            export_count = $pe.export_count
            exports = $pe.exports
            imports = $pe.imports
        }
        if ([string] $declared.status -eq "available") {
            $targetResult.status = "available"
        }
        else {
            $targetNotes.Add("The evidence record declares $($definition.runtime) blocked, so observed artifacts are not promoted without an updated record.")
        }
    }

    if ($targetResult.status -eq "blocked" -and [string] $declared.status -eq "available") {
        $targetNotes.Add("The evidence record declares $($definition.runtime) available, but the required toolchain, artifact, or target-specific hash is unavailable.")
    }
    $targetResult.missing_paths = $targetMissingPaths.ToArray()
    $targetResult.missing_commands = $targetMissingCommands.ToArray()
    $targetResult.notes = $targetNotes.ToArray()
    $targetResults += [pscustomobject] $targetResult
}

$observedStatus = "available"
if ($missingPaths.Count -gt 0 -or $missingCommands.Count -gt 0 -or
    @($targetResults | Where-Object { [string] $_.status -ne "available" }).Count -gt 0) {
    $observedStatus = "blocked"
}
if ([string] $evidence.status -notin @("available", "blocked")) {
    throw "Unexpected declared overall evidence status: $($evidence.status)"
}
if ([string] $evidence.status -eq "available" -and $observedStatus -ne "available") {
    $notes.Add("The evidence record declares available, but the observed multiarch result is blocked.")
}
if ([string] $evidence.status -eq "blocked" -and $observedStatus -eq "available") {
    $notes.Add("The evidence record remains blocked and must be updated before availability can be claimed.")
}
$notes.Add("This check validates PE identity, static-CRT markers, the 29-name export set, and SHA-256 evidence; it does not claim execution on an unavailable target device.")

$result = [ordered]@{
    schema_version = 1
    evidence_kind = [string] $evidence.evidence_kind
    status = $observedStatus
    declared_status = [string] $evidence.status
    evidence_path = $evidenceFullPath
    source = [ordered]@{
        manifest_path = [string] $source.manifest_path
        header_path = [string] $source.header_path
        export_definition_path = [string] $source.export_definition_path
        metadata_path = [string] $source.metadata_path
        hashes = $sourceHashes
        expected_export_count = $expectedExportCount
    }
    targets = @($targetResults)
    missing_paths = $missingPaths.ToArray()
    missing_commands = $missingCommands.ToArray()
    notes = $notes.ToArray()
    validation_policy = "blocked is an honest evidence result, not a passing multiarch gate; use -RequireAvailable to reject blocked evidence."
}

if ($Json) {
    $result | ConvertTo-Json -Depth 20
}
else {
    Write-Output "Public MSP FFI multiarch evidence status: $observedStatus"
    foreach ($target in $targetResults) {
        Write-Output "Target $($target.runtime): $($target.status) (Rust $($target.rust_target), PE $($target.machine_value))"
        if ($target.missing_commands.Count -gt 0) {
            Write-Output "  Missing commands:"
            foreach ($command in $target.missing_commands) {
                Write-Output "    $command"
            }
        }
        if ($target.missing_paths.Count -gt 0) {
            Write-Output "  Missing paths:"
            foreach ($path in $target.missing_paths) {
                Write-Output "    $path"
            }
        }
        foreach ($note in $target.notes) {
            Write-Output "  Note: $note"
        }
        if ($null -ne $target.pe) {
            Write-Output "  Exports: $($target.pe.export_count); DLL SHA-256: $($target.hashes.dll)"
        }
    }
    if ($missingCommands.Count -gt 0) {
        Write-Output "Missing commands:"
        foreach ($command in $missingCommands) {
            Write-Output "  $command"
        }
    }
    if ($missingPaths.Count -gt 0) {
        Write-Output "Missing paths:"
        foreach ($path in $missingPaths) {
            Write-Output "  $path"
        }
    }
    foreach ($note in $notes) {
        Write-Output "Note: $note"
    }
}

if ($RequireAvailable -and $observedStatus -ne "available") {
    exit 1
}
exit 0
