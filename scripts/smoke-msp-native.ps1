[CmdletBinding()]
param(
    [string] $Configuration = "release",

    [string] $DllPath
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$nativeRoot = Join-Path $repoRoot "native\msp-core"
if ([string]::IsNullOrWhiteSpace($DllPath)) {
    $DllPath = Join-Path $nativeRoot "target\$Configuration\msp_core.dll"
}

$DllPath = [System.IO.Path]::GetFullPath($DllPath)

if (-not (Test-Path -LiteralPath $DllPath -PathType Leaf)) {
    throw "Native MSP DLL was not found. Run cargo build --release in native\msp-core first: $DllPath"
}

$typeSuffix = [Guid]::NewGuid().ToString("N")
$abiInfoTypeName = "MspAbiInfoV2_$typeSuffix"
$nativeSmokeTypeName = "MspCoreNativeSmoke_$typeSuffix"
$escapedDllPath = $DllPath.Replace('"', '""')
$source = @"
using System;
using System.Runtime.InteropServices;

[StructLayout(LayoutKind.Explicit, Size = 32)]
public struct $abiInfoTypeName
{
    [FieldOffset(0)] public UInt32 StructSize;
    [FieldOffset(4)] public UInt32 AbiMajor;
    [FieldOffset(8)] public UInt32 AbiMinor;
    [FieldOffset(12)] public UInt32 Reserved;
    [FieldOffset(16)] public UInt64 ContractId;
    [FieldOffset(24)] public UInt64 Capabilities;
}

public static class $nativeSmokeTypeName
{
    [DllImport(@"$escapedDllPath", CallingConvention = CallingConvention.Cdecl, EntryPoint = "msp_get_abi_info_v2")]
    public static extern Int32 GetAbiInfoV2(out $abiInfoTypeName info, UInt32 infoSize);

    [DllImport(@"$escapedDllPath", CallingConvention = CallingConvention.Cdecl, EntryPoint = "msp_invoke_v2")]
    public static extern Int32 InvokeV2(
        UInt32 operation,
        IntPtr requestPointer,
        UInt64 requestLength,
        out IntPtr responsePointer,
        out UInt64 responseLength);

    [DllImport(@"$escapedDllPath", CallingConvention = CallingConvention.Cdecl, EntryPoint = "msp_free_buffer_v2")]
    public static extern void FreeBufferV2(IntPtr value, UInt64 length);

    [DllImport(@"$escapedDllPath", CallingConvention = CallingConvention.Cdecl, EntryPoint = "msp_execute_json")]
    public static extern IntPtr ExecuteJson(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string requestJson);

    [DllImport(@"$escapedDllPath", CallingConvention = CallingConvention.Cdecl, EntryPoint = "msp_free_string")]
    public static extern void FreeString(IntPtr value);
}
"@

Add-Type -TypeDefinition $source
$abiInfoType = $abiInfoTypeName -as [type]
$nativeSmokeType = $nativeSmokeTypeName -as [type]
if ($null -eq $abiInfoType -or $null -eq $nativeSmokeType) {
    throw "Native MSP smoke interop types could not be loaded."
}

$expectedInfoSize = 32
$expectedAbiMajor = 2
$expectedAbiMinor = 0
$expectedContractId = [UInt64]0x324D534F44414552
$requiredCapabilities = [UInt64]0x000000000000000F
$maximumAbiResponseBytes = [UInt64]67108864
$executeOperation = [UInt32]1
$strictUtf8 = [System.Text.UTF8Encoding]::new($false, $true)

$abiInfo = [Activator]::CreateInstance($abiInfoType)
$infoStatus = $nativeSmokeType::GetAbiInfoV2(
    [ref] $abiInfo,
    [UInt32] $expectedInfoSize)
if ($infoStatus -ne 0) {
    throw "msp_get_abi_info_v2 failed with status $infoStatus."
}
if ([Runtime.InteropServices.Marshal]::SizeOf($abiInfo) -ne $expectedInfoSize -or
    $abiInfo.StructSize -ne $expectedInfoSize) {
    throw "Native MSP ABI v2 reported an incompatible info structure size: $($abiInfo.StructSize)."
}
if ($abiInfo.AbiMajor -ne $expectedAbiMajor -or
    $abiInfo.AbiMinor -ne $expectedAbiMinor) {
    throw "Native MSP ABI v2 reported an unexpected version: $($abiInfo.AbiMajor).$($abiInfo.AbiMinor)."
}
if ($abiInfo.Reserved -ne 0) {
    throw "Native MSP ABI v2 did not zero its reserved info field."
}
if ($abiInfo.ContractId -ne $expectedContractId) {
    throw ("Native MSP ABI v2 reported an unexpected contract id: 0x{0:X16}." -f $abiInfo.ContractId)
}
if (($abiInfo.Capabilities -band $requiredCapabilities) -ne $requiredCapabilities) {
    throw ("Native MSP ABI v2 is missing required capabilities: 0x{0:X16}." -f $abiInfo.Capabilities)
}

function Invoke-MspV2 {
    param(
        [Parameter(Mandatory)]
        [UInt32] $Operation,

        [Parameter(Mandatory)]
        [byte[]] $RequestBytes
    )

    $requestHandle = [Runtime.InteropServices.GCHandle]::Alloc(
        $RequestBytes,
        [Runtime.InteropServices.GCHandleType]::Pinned)
    $responsePointer = [IntPtr]::Zero
    [UInt64] $responseLength = 0
    try {
        $status = $nativeSmokeType::InvokeV2(
            $Operation,
            $requestHandle.AddrOfPinnedObject(),
            [UInt64] $RequestBytes.LongLength,
            [ref] $responsePointer,
            [ref] $responseLength)
        if ($status -ne 0) {
            throw "msp_invoke_v2 failed with status $status."
        }
        if ($responsePointer -eq [IntPtr]::Zero) {
            throw "msp_invoke_v2 returned a null response pointer."
        }
        if ($responseLength -eq 0 -or
            $responseLength -gt $maximumAbiResponseBytes) {
            throw "msp_invoke_v2 returned an invalid response length: $responseLength."
        }

        $responseBytes = [byte[]]::new([Int32] $responseLength)
        [Runtime.InteropServices.Marshal]::Copy(
            $responsePointer,
            $responseBytes,
            0,
            $responseBytes.Length)
        return $strictUtf8.GetString($responseBytes)
    }
    finally {
        try {
            if ($responsePointer -ne [IntPtr]::Zero) {
                $nativeSmokeType::FreeBufferV2(
                    $responsePointer,
                    $responseLength)
                $responsePointer = [IntPtr]::Zero
            }
        }
        finally {
            if ($requestHandle.IsAllocated) {
                $requestHandle.Free()
            }
        }
    }
}

function Invoke-MspV1 {
    param(
        [Parameter(Mandatory)]
        [string] $RequestJson
    )

    $responsePointer = $nativeSmokeType::ExecuteJson($RequestJson)
    if ($responsePointer -eq [IntPtr]::Zero) {
        throw "msp_execute_json returned a null pointer during the v1 compatibility smoke."
    }
    try {
        $bytes = [System.Collections.Generic.List[byte]]::new()
        for ($offset = 0; ; $offset++) {
            $byte = [Runtime.InteropServices.Marshal]::ReadByte($responsePointer, $offset)
            if ($byte -eq 0) {
                break
            }
            $bytes.Add([byte] $byte)
        }
        return $strictUtf8.GetString($bytes.ToArray())
    }
    finally {
        $nativeSmokeType::FreeString($responsePointer)
    }
}

function Assert-RegistryBehavior {
    param(
        [Parameter(Mandatory)]
        [string] $Label,

        [Parameter(Mandatory)]
        [string] $HelpResponseJson,

        [Parameter(Mandatory)]
        [string] $UnknownResponseJson,

        [Parameter(Mandatory)]
        [string] $ExpectedActor
    )

    $expectedHelp = ":`nbasename`ncat`ncd`ncommand`ncp`ncreate`ndelete`ndf`ndirname`ndu`necho`nenv`nfalse`nfind`ngrep`nhead`nhelp`nls`nmkdir`nmv`npathchk`nprintf`npwd`nreadlink`nrealpath`nrename`nrm`nsed`nstat`ntail`ntouch`ntrue`ntype`nwc`nwhich`n"
    $expectedHelpBase64 = [Convert]::ToBase64String(
        $strictUtf8.GetBytes($expectedHelp))
    $helpResponse = $HelpResponseJson | ConvertFrom-Json
    $helpAudit = @($helpResponse.auditRecords) | Select-Object -First 1
    if ($helpResponse.contractVersion -cne "reados-msp-native/1" -or
        $helpResponse.exitCode -ne 0 -or
        $helpResponse.stdout -cne $expectedHelp -or
        $helpResponse.stderr -cne "" -or
        $helpResponse.stdoutBytesBase64 -cne $expectedHelpBase64 -or
        $helpResponse.stderrBytesBase64 -cne "" -or
        $null -ne $helpResponse.stateChange -or
        @($helpResponse.diagnostics).Count -ne 0 -or
        @($helpResponse.auditRecords).Count -ne 1 -or
        [string]::IsNullOrWhiteSpace($helpAudit.runId) -or
        $helpAudit.commandLine -cne "help" -or
        $helpAudit.commandName -cne "help" -or
        @($helpAudit.arguments).Count -ne 0 -or
        $helpAudit.exitCode -ne 0 -or
        $helpAudit.startedAtUnixMs -le 0 -or
        $helpAudit.endedAtUnixMs -lt $helpAudit.startedAtUnixMs -or
        $helpAudit.actor -cne $ExpectedActor -or
        $helpAudit.sessionId -cne "default" -or
        $helpAudit.workingDirectory -cne "/" -or
        $helpAudit.policyDecision.kind -cne "allow" -or
        $null -ne $helpAudit.policyDecision.reason -or
        $null -ne $helpAudit.policyDecision.prompt -or
        @($helpAudit.diagnostics).Count -ne 0) {
        throw "$Label registry help behavior drifted: $HelpResponseJson"
    }

    $expectedUnknownMessage = "missing-command: command not found"
    $expectedUnknownStderr = "$expectedUnknownMessage`n"
    $expectedUnknownStderrBase64 = [Convert]::ToBase64String(
        $strictUtf8.GetBytes($expectedUnknownStderr))
    $unknownResponse = $UnknownResponseJson | ConvertFrom-Json
    $unknownDiagnostic = @($unknownResponse.diagnostics) | Select-Object -First 1
    $unknownAudit = @($unknownResponse.auditRecords) | Select-Object -First 1
    $unknownAuditDiagnostic = @($unknownAudit.diagnostics) | Select-Object -First 1
    if ($unknownResponse.contractVersion -cne "reados-msp-native/1" -or
        $unknownResponse.exitCode -ne 127 -or
        $unknownResponse.stdout -cne "" -or
        $unknownResponse.stderr -cne $expectedUnknownStderr -or
        $unknownResponse.stdoutBytesBase64 -cne "" -or
        $unknownResponse.stderrBytesBase64 -cne $expectedUnknownStderrBase64 -or
        $null -ne $unknownResponse.stateChange -or
        @($unknownResponse.diagnostics).Count -ne 1 -or
        $null -eq $unknownDiagnostic -or
        $unknownDiagnostic.severity -cne "error" -or
        $unknownDiagnostic.code -cne "msp.command_not_found" -or
        $unknownDiagnostic.message -cne $expectedUnknownMessage -or
        $unknownDiagnostic.target -cne "missing-command" -or
        $unknownDiagnostic.recoveryHint -cne "Use an enabled MSP command pack command." -or
        @($unknownResponse.auditRecords).Count -ne 1 -or
        [string]::IsNullOrWhiteSpace($unknownAudit.runId) -or
        $unknownAudit.commandLine -cne "missing-command" -or
        $unknownAudit.commandName -cne "missing-command" -or
        @($unknownAudit.arguments).Count -ne 0 -or
        $unknownAudit.exitCode -ne 127 -or
        $unknownAudit.startedAtUnixMs -le 0 -or
        $unknownAudit.endedAtUnixMs -lt $unknownAudit.startedAtUnixMs -or
        $unknownAudit.actor -cne $ExpectedActor -or
        $unknownAudit.sessionId -cne "default" -or
        $unknownAudit.workingDirectory -cne "/" -or
        $unknownAudit.policyDecision.kind -cne "notEvaluated" -or
        $null -ne $unknownAudit.policyDecision.reason -or
        $null -ne $unknownAudit.policyDecision.prompt -or
        @($unknownAudit.diagnostics).Count -ne 1 -or
        $unknownAuditDiagnostic.severity -cne "error" -or
        $unknownAuditDiagnostic.code -cne "msp.command_not_found" -or
        $unknownAuditDiagnostic.message -cne $expectedUnknownMessage -or
        $unknownAuditDiagnostic.target -cne "missing-command" -or
        $unknownAuditDiagnostic.recoveryHint -cne "Use an enabled MSP command pack command.") {
        throw "$Label unknown-command behavior drifted: $UnknownResponseJson"
    }
}

$v2Request = '{"commandText":"pwd","workingDirectory":"/native","actor":"powershell-smoke-v2"}'
$v2ResponseJson = Invoke-MspV2 `
    -Operation $executeOperation `
    -RequestBytes ($strictUtf8.GetBytes($v2Request))
$v2Response = $v2ResponseJson | ConvertFrom-Json
if ($v2Response.exitCode -ne 0) {
    throw "Native MSP ABI v2 smoke command failed: $v2ResponseJson"
}
if ($v2Response.stdout -cne "/native`n") {
    throw "Native MSP ABI v2 smoke command returned unexpected stdout: $v2ResponseJson"
}
if ($v2Response.auditRecords[0].actor -cne "powershell-smoke-v2") {
    throw "Native MSP ABI v2 smoke command did not preserve actor in audit record: $v2ResponseJson"
}

$v2HelpResponseJson = Invoke-MspV2 `
    -Operation $executeOperation `
    -RequestBytes ($strictUtf8.GetBytes('{"commandText":"help","actor":"powershell-smoke-v2"}'))
$v2UnknownResponseJson = Invoke-MspV2 `
    -Operation $executeOperation `
    -RequestBytes ($strictUtf8.GetBytes('{"commandText":"missing-command","actor":"powershell-smoke-v2"}'))
Assert-RegistryBehavior `
    -Label "ABI v2" `
    -HelpResponseJson $v2HelpResponseJson `
    -UnknownResponseJson $v2UnknownResponseJson `
    -ExpectedActor "powershell-smoke-v2"

# A raw NUL between a complete JSON request and extra bytes must remain part of
# the length-delimited request. A C-string implementation would incorrectly run
# the valid prefix as pwd; ABI v2 must instead return a contract failure.
$prefixBytes = $strictUtf8.GetBytes($v2Request)
$suffixBytes = $strictUtf8.GetBytes('{"unexpected":"suffix"}')
$embeddedNullRequest = [byte[]]::new($prefixBytes.Length + 1 + $suffixBytes.Length)
[Array]::Copy($prefixBytes, 0, $embeddedNullRequest, 0, $prefixBytes.Length)
[Array]::Copy(
    $suffixBytes,
    0,
    $embeddedNullRequest,
    $prefixBytes.Length + 1,
    $suffixBytes.Length)
$embeddedNullResponseJson = Invoke-MspV2 `
    -Operation $executeOperation `
    -RequestBytes $embeddedNullRequest
$embeddedNullResponse = $embeddedNullResponseJson | ConvertFrom-Json
if ($embeddedNullResponse.exitCode -eq 0 -or
    -not ($embeddedNullResponse.diagnostics | Where-Object {
        $_.code -ceq "msp.native.invalid_request"
    })) {
    throw "Native MSP ABI v2 truncated an embedded-NUL request instead of honoring its explicit length: $embeddedNullResponseJson"
}

$v1Actor = "powershell-smoke-v1-$([char] 0x6D4B)$([char] 0x8BD5)"
$v1Request = @{
    commandText = "pwd"
    workingDirectory = "/native"
    actor = $v1Actor
} | ConvertTo-Json -Compress
$v1ResponseJson = Invoke-MspV1 -RequestJson $v1Request

$v1Response = $v1ResponseJson | ConvertFrom-Json
if ($v1Response.exitCode -ne 0) {
    throw "Native MSP v1 compatibility smoke command failed: $v1ResponseJson"
}
if ($v1Response.stdout -cne "/native`n") {
    throw "Native MSP v1 compatibility smoke command returned unexpected stdout: $v1ResponseJson"
}
if ($v1Response.auditRecords[0].actor -cne $v1Actor) {
    throw "Native MSP v1 compatibility smoke command did not preserve actor in audit record: $v1ResponseJson"
}

$v1HelpResponseJson = Invoke-MspV1 `
    -RequestJson '{"commandText":"help","actor":"powershell-smoke-v1"}'
$v1UnknownResponseJson = Invoke-MspV1 `
    -RequestJson '{"commandText":"missing-command","actor":"powershell-smoke-v1"}'
Assert-RegistryBehavior `
    -Label "ABI v1" `
    -HelpResponseJson $v1HelpResponseJson `
    -UnknownResponseJson $v1UnknownResponseJson `
    -ExpectedActor "powershell-smoke-v1"

Write-Host "Native MSP ABI v2 handshake and FFI smoke passed."
Write-Host ("  DLL:                   {0}" -f $DllPath)
Write-Host ("  ABI:                   {0}.{1}" -f $abiInfo.AbiMajor, $abiInfo.AbiMinor)
Write-Host ("  Contract id:           0x{0:X16}" -f $abiInfo.ContractId)
Write-Host ("  Capabilities:          0x{0:X16}" -f $abiInfo.Capabilities)
Write-Host "  ABI v2 pwd:            passed"
Write-Host "  Registry help/unknown: passed on ABI v2 and v1"
Write-Host "  Embedded-NUL request:  length preserved"
Write-Host "  ABI v1 compatibility:  passed"
