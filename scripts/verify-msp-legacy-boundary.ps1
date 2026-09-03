[CmdletBinding()]
param(
    [string]$RepositoryRoot,
    [switch]$SkipMigrationMatrix
)

$ErrorActionPreference = 'Stop'

function Fail([string]$Message) {
    throw "MSP legacy boundary verification failed: $Message"
}

if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = Split-Path -Parent $PSScriptRoot
}

$root = (Resolve-Path -LiteralPath $RepositoryRoot).Path

if (-not $SkipMigrationMatrix) {
    $matrixVerifier = Join-Path $root 'scripts\verify-msp-runtime-migration-matrix.ps1'
    if (-not (Test-Path -LiteralPath $matrixVerifier -PathType Leaf)) {
        Fail "migration matrix verifier not found: $matrixVerifier"
    }

    & $matrixVerifier -RepositoryRoot $root
    if ($LASTEXITCODE -and $LASTEXITCODE -ne 0) {
        Fail "migration matrix verifier returned exit code $LASTEXITCODE"
    }
}

# Only the Hosting native adapter may mention the retained legacy ABI. Product
# services, managed runtime contracts, and domain code must consume the adapter
# or the modular command-runtime FFI instead of loading old DLLs or exports.
$productRoots = @(
    (Join-Path $root 'src\ReadOS.App'),
    (Join-Path $root 'src\ReadOS.Msp'),
    (Join-Path $root 'src\ReadOS.Msp.Hosting')
)
$allowedLegacyBoundaryRoot = (Join-Path $root 'src\ReadOS.Msp.Hosting\Native') + [IO.Path]::DirectorySeparatorChar
$sourceExtensions = @('.cs', '.csproj', '.xaml', '.props', '.targets', '.xml')
$forbiddenPatterns = @(
    '(?i)native[\\/]msp-core',
    '(?i)native[\\/]msp-ffi',
    '(?i)\bmsp_core\.dll\b',
    '(?i)\bmsp_ffi(?:\.dll|[_\.])',
    '(?i)\bMspFfi\b',
    '(?i)\bDllImportAttribute\b|\bDllImport\b',
    '(?i)\bNativeLibrary\.(?:Load|Free)\b',
    '(?i)\bmsp_(?:get_abi_info_v2|invoke_v2|free_buffer_v2|execute_json|free_string)\b'
)

$violations = [System.Collections.Generic.List[string]]::new()
foreach ($productRoot in $productRoots) {
    if (-not (Test-Path -LiteralPath $productRoot -PathType Container)) {
        continue
    }

    $files = Get-ChildItem -LiteralPath $productRoot -File -Recurse -Force |
        Where-Object {
            $_.FullName -notmatch '[\\/]bin[\\/]' -and
            $_.FullName -notmatch '[\\/]obj[\\/]' -and
            ($sourceExtensions -contains $_.Extension.ToLowerInvariant())
        }

    foreach ($file in $files) {
        if ($file.FullName.StartsWith($allowedLegacyBoundaryRoot, [StringComparison]::OrdinalIgnoreCase)) {
            continue
        }

        $text = Get-Content -LiteralPath $file.FullName -Raw
        foreach ($forbiddenPattern in $forbiddenPatterns) {
            $match = [regex]::Match($text, $forbiddenPattern)
            if ($match.Success) {
                $relative = $file.FullName.Substring($root.Length + 1).Replace('\', '/')
                $line = ($text.Substring(0, $match.Index) -split "`n").Count
                $violations.Add("${relative}:${line} matches $forbiddenPattern")
            }
        }
    }
}

if ($violations.Count -gt 0) {
    Fail ($violations -join '; ')
}

Write-Output 'MSP legacy boundary verified: product code uses Hosting/modular adapters and has no direct legacy ABI or DLL references.'
