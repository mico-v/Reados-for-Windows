[CmdletBinding()]
param(
    [string]$RepositoryRoot
)

$ErrorActionPreference = 'Stop'

function Fail([string]$Message) {
    throw "MSP runtime migration matrix verification failed: $Message"
}

if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = Split-Path -Parent $PSScriptRoot
}

$root = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$matrixPath = Join-Path $root 'docs\MSP_RUNTIME_MIGRATION_MATRIX.md'
if (-not (Test-Path -LiteralPath $matrixPath -PathType Leaf)) {
    Fail "matrix not found: $matrixPath"
}

$content = Get-Content -LiteralPath $matrixPath -Raw
$rowPattern = '(?m)^\|\s*`(?<path>[^`]+)`\s*\|\s*(?<disposition>port|adapt|defer|delete)\s*\|\s*(?<destination>[^|]+)\|\s*(?<owner>[^|]+)\|\s*(?<gate>[^|]+)\|\s*(?<dependency>[^|]+)\s*\|'
$rows = [regex]::Matches($content, $rowPattern)
if ($rows.Count -eq 0) {
    Fail 'no inventory rows were found'
}

$inventory = [System.Collections.Generic.List[object]]::new()
$seenPatterns = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
foreach ($match in $rows) {
    $path = $match.Groups['path'].Value.Trim()
    $disposition = $match.Groups['disposition'].Value.Trim()
    $destination = $match.Groups['destination'].Value.Trim()
    $owner = $match.Groups['owner'].Value.Trim()
    $gate = $match.Groups['gate'].Value.Trim()
    $dependency = $match.Groups['dependency'].Value.Trim()

    if (-not $seenPatterns.Add($path)) {
        Fail "duplicate inventory pattern: $path"
    }

    if ([string]::IsNullOrWhiteSpace($destination) -or
        [string]::IsNullOrWhiteSpace($owner) -or
        [string]::IsNullOrWhiteSpace($gate) -or
        [string]::IsNullOrWhiteSpace($dependency)) {
        Fail "inventory row has an empty ownership or acceptance field: $path"
    }

    $inventory.Add([pscustomobject]@{
        Path = $path
        Disposition = $disposition
    })
}

function Convert-GlobToRegex([string]$Glob) {
    $escaped = [regex]::Escape($Glob)
    $escaped = $escaped.Replace('/\*\*/', '/(?:.*/)?')
    $escaped = $escaped.Replace('\*\*', '.*')
    $escaped = $escaped.Replace('\*', '[^/]*')
    $escaped = $escaped.Replace('\?', '.')
    return '^' + $escaped + '$'
}

$legacyRoots = @(
    (Join-Path $root 'native\msp-core'),
    (Join-Path $root 'native\msp-ffi')
)
$sourceExtensions = @(
    '.rs', '.toml', '.lock', '.h', '.hpp', '.c', '.cpp', '.cs', '.js', '.ts', '.json', '.def', '.md'
)
$legacyFiles = foreach ($legacyRoot in $legacyRoots) {
    if (-not (Test-Path -LiteralPath $legacyRoot -PathType Container)) {
        continue
    }

    Get-ChildItem -LiteralPath $legacyRoot -File -Recurse -Force |
        Where-Object {
            $_.FullName -notmatch '[\\/]target[\\/]' -and
            $_.FullName -notmatch '[\\/]\.git[\\/]' -and
            ($sourceExtensions -contains $_.Extension.ToLowerInvariant() -or $_.Name -eq 'build.rs')
        }
}

if ($legacyFiles.Count -eq 0) {
    Fail 'no legacy source files were found'
}

$unmapped = [System.Collections.Generic.List[string]]::new()
$ambiguous = [System.Collections.Generic.List[string]]::new()
foreach ($file in $legacyFiles) {
    $relative = $file.FullName.Substring($root.Length + 1).Replace('\', '/')
    $matches = @($inventory | Where-Object {
        $relative -match (Convert-GlobToRegex $_.Path)
    })

    if ($matches.Count -eq 0) {
        $unmapped.Add($relative)
    } elseif ($matches.Count -gt 1) {
        $ambiguous.Add("$relative -> $($matches.Path -join ', ')")
    }
}

$orphanPatterns = [System.Collections.Generic.List[string]]::new()
foreach ($row in $inventory) {
    $pattern = Convert-GlobToRegex $row.Path
    if (-not ($legacyFiles | Where-Object {
        $relative = $_.FullName.Substring($root.Length + 1).Replace('\', '/')
        $relative -match $pattern
    })) {
        $orphanPatterns.Add($row.Path)
    }
}

if ($unmapped.Count -gt 0) {
    Fail "unmapped legacy source files: $($unmapped -join '; ')"
}
if ($ambiguous.Count -gt 0) {
    Fail "legacy source files match multiple rows: $($ambiguous -join '; ')"
}
if ($orphanPatterns.Count -gt 0) {
    Fail "inventory patterns match no current legacy source file: $($orphanPatterns -join '; ')"
}

Write-Output "MSP runtime migration matrix verified: $($legacyFiles.Count) legacy source files covered by $($inventory.Count) disposition rows."
