[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $DllPath
)

$ErrorActionPreference = "Stop"

$DllPath = [System.IO.Path]::GetFullPath($DllPath)
if (-not (Test-Path -LiteralPath $DllPath -PathType Leaf)) {
    throw "Native MSP DLL was not found: $DllPath"
}

$binaryText = [System.Text.Encoding]::ASCII.GetString(
    [System.IO.File]::ReadAllBytes($DllPath))

$requiredExports = @(
    "msp_execute_json",
    "msp_parse_json",
    "msp_normalize_workspace_path_json",
    "msp_free_string",
    "msp_get_abi_info_v2",
    "msp_invoke_v2",
    "msp_free_buffer_v2"
)
foreach ($requiredExport in $requiredExports) {
    if (-not ($binaryText.IndexOf(
            $requiredExport,
            [System.StringComparison]::Ordinal) -ge 0)) {
        throw "Native MSP DLL is missing required export marker: $requiredExport"
    }
}

$forbiddenDynamicCrtNames = @(
    "VCRUNTIME",
    "MSVCP140",
    "CONCRT140",
    "api-ms-win-crt-",
    "ucrtbase.dll"
)
$dynamicCrtDependency = $forbiddenDynamicCrtNames | Where-Object {
    $binaryText.IndexOf($_, [System.StringComparison]::OrdinalIgnoreCase) -ge 0
} | Select-Object -First 1
if ($dynamicCrtDependency) {
    throw "Native MSP DLL is not CRT-self-contained; found dependency marker: $dynamicCrtDependency"
}

Write-Host "Native MSP binary verification passed."
Write-Host "  DLL: $DllPath"
Write-Host "  Required exports: $($requiredExports.Count)"
Write-Host "  Dynamic CRT markers: none"
