[CmdletBinding()]
param(
    [string] $RepositoryRoot
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
}
else {
    $RepositoryRoot = [System.IO.Path]::GetFullPath($RepositoryRoot)
}

function Fail([string] $Message) {
    throw "MSP command runtime FFI ABI verification failed: $Message"
}

function Assert-Sequence(
    [object[]] $Actual,
    [object[]] $Expected,
    [string] $Label
) {
    if ($Actual.Count -ne $Expected.Count) {
        Fail "$Label count drifted. Expected $($Expected.Count), found $($Actual.Count)."
    }

    for ($index = 0; $index -lt $Expected.Count; $index++) {
        if ([string] $Actual[$index] -cne [string] $Expected[$index]) {
            Fail "$Label drifted at index $index. Expected '$($Expected[$index])', found '$($Actual[$index])'."
        }
    }
}

function Get-IntegerExpressionValue([string] $Expression) {
    $normalized = $Expression
    $normalized = $normalized -replace '\\', ' '
    $normalized = $normalized -replace '(?m)//.*$', ' '
    $normalized = $normalized -replace '/\*.*?\*/', ' '
    $normalized = $normalized -replace '(?:UINT|INT)(?:8|16|32|64)?_C\(\s*(\d+)\s*\)', '$1'
    $normalized = $normalized -replace '(\d)[uUlL]+\b', '$1'
    $normalized = $normalized -replace '(?<=\d)_(?=\d)', ''
    $normalized = $normalized.Trim()
    $normalized = $normalized.Trim('(', ')', ' ', "`r", "`n", "`t")
    if ($normalized -notmatch '^\d+(?:\s*\*\s*\d+)*$') {
        Fail "Unsupported integer expression '$Expression'."
    }

    [uint64] $value = 1
    foreach ($part in ($normalized -split '\s*\*\s*')) {
        $value = $value * [uint64] $part
    }

    return $value
}

function Get-CMacroExpression([string] $Text, [string] $Name) {
    $match = [regex]::Match(
        $Text,
        "(?ms)^#define\s+$([regex]::Escape($Name))\s+(?<expression>.*?)(?=^#define|^#if|^#else|^#endif|\z)")
    if (-not $match.Success) {
        Fail "C header is missing $Name."
    }

    return $match.Groups["expression"].Value
}

function Get-RustConstantExpression([string] $Text, [string] $Name) {
    $match = [regex]::Match(
        $Text,
        "(?m)^pub const\s+$([regex]::Escape($Name))\s*:\s*[^=]+?\s*=\s*(?<expression>[^;]+);\s*$")
    if (-not $match.Success) {
        Fail "Rust FFI source is missing $Name."
    }

    return $match.Groups["expression"].Value
}

function Get-CSharpConstantExpression([string] $Text, [string] $Name) {
    $match = [regex]::Match(
        $Text,
        "(?m)^\s*public const int\s+$([regex]::Escape($Name))\s*=\s*(?<expression>[^;]+);\s*$")
    if (-not $match.Success) {
        Fail ".NET FFI contract is missing $Name."
    }

    return $match.Groups["expression"].Value
}

function Get-KotlinConstantExpression([string] $Text, [string] $Name) {
    $match = [regex]::Match(
        $Text,
        "(?m)^\s*const val\s+$([regex]::Escape($Name))\s*:\s*Long\s*=\s*(?<expression>[^\r\n]+)")
    if (-not $match.Success) {
        Fail "Kotlin FFI contract is missing $Name."
    }

    return $match.Groups["expression"].Value
}

$manifestPath = Join-Path $RepositoryRoot "native\msp-command-runtime-ffi\abi\msp_command_runtime_ffi.v1.json"
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    Fail "canonical ABI manifest is missing: $manifestPath"
}
$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json

$headerPath = Join-Path $RepositoryRoot "native\msp-command-runtime-ffi\include\msp_command_runtime_ffi.h"
$header = Get-Content -Raw -LiteralPath $headerPath
$definitionPath = Join-Path $RepositoryRoot "native\msp-command-runtime-ffi\exports\msp_command_runtime_ffi.def"
$definition = Get-Content -Raw -LiteralPath $definitionPath
$rustPath = Join-Path $RepositoryRoot "native\msp-command-runtime-ffi\src\lib.rs"
$rust = Get-Content -Raw -LiteralPath $rustPath
$nativeCSharpPath = Join-Path $RepositoryRoot "src\ReadOS.Msp.Hosting\Native\RuntimeFfi\MspCommandRuntimeFfiNative.cs"
$nativeCSharp = Get-Content -Raw -LiteralPath $nativeCSharpPath
$contractsCSharpPath = Join-Path $RepositoryRoot "src\ReadOS.Msp.Hosting\Native\RuntimeFfi\MspCommandRuntimeFfiContracts.cs"
$contractsCSharp = Get-Content -Raw -LiteralPath $contractsCSharpPath
$adapterCSharpPath = Join-Path $RepositoryRoot "src\ReadOS.Msp.Hosting\Native\RuntimeFfi\MspCommandRuntimeFfiAdapter.cs"
$adapterCSharp = Get-Content -Raw -LiteralPath $adapterCSharpPath
$abiCSharpPath = Join-Path $RepositoryRoot "src\ReadOS.Msp.Hosting\Native\RuntimeFfi\MspCommandRuntimeFfiAbi.cs"
$abiCSharp = Get-Content -Raw -LiteralPath $abiCSharpPath
$kotlinPath = Join-Path $RepositoryRoot "native\msp-command-runtime-ffi\android\src\main\kotlin\com\reados\msp\commandruntime\MspCommandRuntimeFfiAbi.kt"
$kotlin = Get-Content -Raw -LiteralPath $kotlinPath
$gradlePath = Join-Path $RepositoryRoot "native\msp-command-runtime-ffi\android\msp-command-runtime-ffi\build.gradle.kts"
$gradle = Get-Content -Raw -LiteralPath $gradlePath

if ($manifest.abiVersion -ne (Get-IntegerExpressionValue (Get-CMacroExpression $header "MCR_FFI_ABI_VERSION"))) {
    Fail "ABI version differs between manifest and C header."
}
if ($manifest.requestSchemaVersion -ne (Get-IntegerExpressionValue (Get-CMacroExpression $header "MCR_FFI_REQUEST_SCHEMA_VERSION"))) {
    Fail "request schema version differs between manifest and C header."
}
if ($header -notmatch ('#define\s+MCR_FFI_HEADER_VERSION_STRING\s+"' + [regex]::Escape([string] $manifest.version) + '"')) {
    Fail "header version differs between manifest and C header."
}

$headerExports = @(
    ([regex]::Matches($header, '(?m)^msp_command_runtime_ffi_[A-Za-z0-9_]+\s*\(') |
        ForEach-Object { $_.Value.Trim() -replace '\s*\($', '' })
)
Assert-Sequence $headerExports @($manifest.exports) "C header export declarations"

$definitionExports = @()
$reading = $false
foreach ($line in (Get-Content -LiteralPath $definitionPath)) {
    $trimmed = $line.Trim()
    if ($trimmed -eq "EXPORTS") {
        $reading = $true
        continue
    }
    if ($reading -and $trimmed -and -not $trimmed.StartsWith(';')) {
        if ($trimmed -notmatch '^[A-Za-z_][A-Za-z0-9_]*$') {
            Fail "malformed export definition line '$line'."
        }
        $definitionExports += $trimmed
    }
}
Assert-Sequence $definitionExports @($manifest.exports) "export definition"

$rustExportMatches = [regex]::Matches($rust, '(?m)^pub\s+(?:unsafe\s+)?extern\s+"C"\s+fn\s+([A-Za-z0-9_]+)')
$rustExports = @($rustExportMatches | ForEach-Object { $_.Groups[1].Value })
Assert-Sequence $rustExports @($manifest.exports) "Rust FFI exports"

$rustVersion = [regex]::Match($rust, 'pub const VERSION_DATA: &\[u8\] = b"([^"]+)";')
if (-not $rustVersion.Success -or $rustVersion.Groups[1].Value -cne [string] $manifest.version) {
    Fail "Rust VERSION_DATA differs from manifest."
}

$managedAbi = [regex]::Match($abiCSharp, '(?ms)internal static class MspCommandRuntimeFfiAbi\s*\{(?<body>.*?)\n\}')
if (-not $managedAbi.Success) {
    Fail ".NET ABI metadata class is missing."
}
$managedAbiBody = $managedAbi.Groups["body"].Value
$managedAbiVersion = [regex]::Match($managedAbiBody, '(?m)internal const uint AbiVersion\s*=\s*(\d+)')
$managedSchemaVersion = [regex]::Match($managedAbiBody, '(?m)internal const uint RequestSchemaVersion\s*=\s*(\d+)')
$managedHeaderVersion = [regex]::Match($managedAbiBody, '(?m)internal const string HeaderVersion\s*=\s*"([^"]+)"')
if (-not $managedAbiVersion.Success -or [uint32] $managedAbiVersion.Groups[1].Value -ne [uint32] $manifest.abiVersion) {
    Fail ".NET ABI version differs from manifest."
}
if (-not $managedSchemaVersion.Success -or [uint32] $managedSchemaVersion.Groups[1].Value -ne [uint32] $manifest.requestSchemaVersion) {
    Fail ".NET request schema version differs from manifest."
}
if (-not $managedHeaderVersion.Success -or $managedHeaderVersion.Groups[1].Value -cne [string] $manifest.version) {
    Fail ".NET header version differs from manifest."
}

$limitMap = @{}
foreach ($property in $manifest.limits.psobject.Properties) {
    $limitMap[$property.Name] = [uint64] $property.Value
    $headerValue = Get-IntegerExpressionValue (Get-CMacroExpression $header ("MCR_FFI_" + $property.Name))
    if ($headerValue -ne $limitMap[$property.Name]) {
        Fail "C header limit $($property.Name) differs from manifest."
    }

    $rustValue = Get-IntegerExpressionValue (Get-RustConstantExpression $rust $property.Name)
    if ($rustValue -ne $limitMap[$property.Name]) {
        Fail "Rust limit $($property.Name) differs from manifest."
    }
}

$csharpLimitNames = @{
    MAX_JSON_REQUEST_BYTES = "DefaultMaximumJsonRequestBytes"
    MAX_COMMAND_BYTES = "DefaultMaximumCommandBytes"
    MAX_CWD_BYTES = "DefaultMaximumVirtualPathBytes"
    MAX_FILE_BYTES = "DefaultMaximumFileBytes"
    MAX_WORKSPACE_BYTES = "DefaultMaximumWorkspaceBytes"
    MAX_WORKSPACE_FILES = "DefaultMaximumWorkspaceFiles"
    MAX_LIST_ENTRIES = "DefaultMaximumDirectoryEntries"
    MAX_STDIN_BYTES = "DefaultMaximumStdinBytes"
    MAX_OUTPUT_BYTES = "DefaultMaximumOutputBytes"
    MAX_DIAGNOSTIC_BYTES = "DefaultMaximumDiagnosticBytes"
    MAX_VARIABLES = "DefaultMaximumVariables"
    MAX_VARIABLE_NAME_BYTES = "DefaultMaximumVariableNameBytes"
    MAX_VARIABLE_VALUE_BYTES = "DefaultMaximumVariableValueBytes"
    MAX_VARIABLE_TOTAL_BYTES = "DefaultMaximumVariableTotalBytes"
}
foreach ($entry in $csharpLimitNames.GetEnumerator()) {
    $value = Get-IntegerExpressionValue (Get-CSharpConstantExpression $contractsCSharp $entry.Value)
    if ($value -ne $limitMap[$entry.Key]) {
        Fail ".NET limit $($entry.Value) differs from manifest."
    }
}

$kotlinLimitNames = @{
    MAX_JSON_REQUEST_BYTES = "MAX_JSON_REQUEST_BYTES"
    MAX_COMMAND_BYTES = "MAX_COMMAND_BYTES"
    MAX_CWD_BYTES = "MAX_CWD_BYTES"
    MAX_FILE_BYTES = "MAX_FILE_BYTES"
    MAX_WORKSPACE_BYTES = "MAX_WORKSPACE_BYTES"
    MAX_WORKSPACE_FILES = "MAX_WORKSPACE_FILES"
    MAX_LIST_ENTRIES = "MAX_LIST_ENTRIES"
    MAX_STDIN_BYTES = "MAX_STDIN_BYTES"
    MAX_OUTPUT_BYTES = "MAX_OUTPUT_BYTES"
    MAX_DIAGNOSTIC_BYTES = "MAX_DIAGNOSTIC_BYTES"
    MAX_VARIABLES = "MAX_VARIABLES"
    MAX_VARIABLE_NAME_BYTES = "MAX_VARIABLE_NAME_BYTES"
    MAX_VARIABLE_VALUE_BYTES = "MAX_VARIABLE_VALUE_BYTES"
    MAX_VARIABLE_TOTAL_BYTES = "MAX_VARIABLE_TOTAL_BYTES"
}
foreach ($entry in $kotlinLimitNames.GetEnumerator()) {
    $value = Get-IntegerExpressionValue (Get-KotlinConstantExpression $kotlin $entry.Value)
    if ($value -ne $limitMap[$entry.Key]) {
        Fail "Kotlin limit $($entry.Value) differs from manifest."
    }
}

$statusMap = @{}
foreach ($property in $manifest.statusCodes.psobject.Properties) {
    $statusMap[$property.Name] = [int] $property.Value
    $headerValue = [int](Get-IntegerExpressionValue (Get-CMacroExpression $header ("MCR_FFI_STATUS_" + $property.Name)))
    if ($headerValue -ne $statusMap[$property.Name]) {
        Fail "C header status $($property.Name) differs from manifest."
    }

    $rustValue = [int](Get-IntegerExpressionValue (Get-RustConstantExpression $rust ("STATUS_" + $property.Name)))
    if ($rustValue -ne $statusMap[$property.Name]) {
        Fail "Rust status $($property.Name) differs from manifest."
    }
}
if ([uint64] $manifest.diagnostics.maxBytes -ne $limitMap["MAX_DIAGNOSTIC_BYTES"]) {
    Fail "manifest diagnostic maxBytes must equal MAX_DIAGNOSTIC_BYTES."
}

$csharpExportClass = [regex]::Match($nativeCSharp, '(?ms)internal static class MspCommandRuntimeFfiExports\s*\{(?<body>.*?)(?=\r?\n\})')
if (-not $csharpExportClass.Success) {
    Fail ".NET export class is missing."
}
$csharpExports = @(
    [regex]::Matches($csharpExportClass.Groups["body"].Value, '(?m)^\s*internal const string \w+\s*=\s*"([^"]+)";') |
        ForEach-Object { $_.Groups[1].Value }
)
Assert-Sequence $csharpExports @($manifest.exports) ".NET export constants"

$kotlinExportBlock = [regex]::Match($kotlin, '(?ms)private val exportNames = arrayOf\((?<body>.*?)\n\s*\)')
if (-not $kotlinExportBlock.Success) {
    Fail "Kotlin export list is missing."
}
$kotlinExports = @(
    [regex]::Matches($kotlinExportBlock.Groups["body"].Value, '"([^"]+)"') |
        ForEach-Object { $_.Groups[1].Value }
)
Assert-Sequence $kotlinExports @($manifest.exports) "Kotlin export constants"

$gradleExportBlock = [regex]::Match($gradle, '(?ms)val expectedRustExports = setOf\((?<body>.*?)\n\)')
if (-not $gradleExportBlock.Success) {
    Fail "Android Gradle export list is missing."
}
$gradleExports = @(
    [regex]::Matches($gradleExportBlock.Groups["body"].Value, '"([^"]+)"') |
        ForEach-Object { $_.Groups[1].Value }
)
Assert-Sequence $gradleExports @($manifest.exports) "Android Gradle export list"

$allowedProperties = @(
    [regex]::Matches(
        ([regex]::Match($contractsCSharp, '(?ms)AllowedWireProperties\s*=\s*\[(?<body>.*?)\];').Groups["body"].Value),
        '"([^"]+)"') | ForEach-Object { $_.Groups[1].Value }
)
Assert-Sequence ($manifest.request.required + $manifest.request.optional) $allowedProperties "manifest request field order"
Assert-Sequence $allowedProperties ($manifest.request.required + $manifest.request.optional) ".NET allowed request fields"

$requestStruct = [regex]::Match($rust, '(?ms)struct RequestDto\s*\{(?<body>.*?)\n\}')
if (-not $requestStruct.Success) {
    Fail "Rust RequestDto is missing."
}
$requestBody = $requestStruct.Groups["body"].Value
if ($requestBody -match '(?m)^\s*(?:pub\s+)?environment\s*:|serde\(rename\s*=\s*"environment"') {
    Fail "Rust RequestDto exposes the forbidden environment field."
}
if ($rust -match '\.with_environment\s*\(') {
    Fail "Rust execution still accepts an environment authority."
}

$forbidden = @($manifest.request.forbidden)
foreach ($field in $forbidden) {
    if ($field -eq "environment" -and ($contractsCSharp -match 'public\s+[^\r\n]*Environment\s*\{' -or $kotlin -match '(?i)\benvironment\b')) {
        Fail "a managed request surface exposes forbidden field '$field'."
    }
}

$exportCount = @($manifest.exports).Count
$limitCount = @($manifest.limits.psobject.Properties).Count
Write-Host "MSP command runtime FFI ABI verified: manifest v$($manifest.abiVersion) ($($manifest.version)); $exportCount exports, $limitCount limits, and shared request/authority boundary match Rust/.NET/Android."
