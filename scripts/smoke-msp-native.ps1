[CmdletBinding()]
param(
    [string] $Configuration = "release"
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$nativeRoot = Join-Path $repoRoot "native\msp-core"
$dllPath = Join-Path $nativeRoot "target\$Configuration\msp_core.dll"

if (-not (Test-Path $dllPath -PathType Leaf)) {
    throw "Native MSP DLL was not found. Run cargo build --release in native\msp-core first: $dllPath"
}

$escapedDllPath = $dllPath.Replace('"', '""')
$source = @"
using System;
using System.Runtime.InteropServices;

public static class MspCoreNativeSmoke
{
    [DllImport(@"$escapedDllPath", CallingConvention = CallingConvention.Cdecl, EntryPoint = "msp_execute_json")]
    public static extern IntPtr ExecuteJson(string requestJson);

    [DllImport(@"$escapedDllPath", CallingConvention = CallingConvention.Cdecl, EntryPoint = "msp_free_string")]
    public static extern void FreeString(IntPtr value);
}
"@

Add-Type -TypeDefinition $source

$request = '{"commandText":"pwd","workingDirectory":"/native","actor":"powershell-smoke"}'
$responsePtr = [MspCoreNativeSmoke]::ExecuteJson($request)
if ($responsePtr -eq [IntPtr]::Zero) {
    throw "msp_execute_json returned a null pointer."
}

try {
    $responseJson = [Runtime.InteropServices.Marshal]::PtrToStringUTF8($responsePtr)
}
finally {
    [MspCoreNativeSmoke]::FreeString($responsePtr)
}

$response = $responseJson | ConvertFrom-Json
if ($response.exitCode -ne 0) {
    throw "Native MSP smoke command failed: $responseJson"
}

if ($response.stdout -ne "/native`n") {
    throw "Native MSP smoke command returned unexpected stdout: $responseJson"
}

if ($response.auditRecords[0].actor -ne "powershell-smoke") {
    throw "Native MSP smoke command did not preserve actor in audit record: $responseJson"
}

Write-Host "Native MSP FFI smoke passed."
