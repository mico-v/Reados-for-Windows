namespace ReadOS.Msp.Hosting.Native;

public enum MspNativeFailureKind
{
    PlatformUnsupported,
    LibraryUnavailable,
    ExportUnavailable,
    AbiExportSetIncomplete,
    AbiHandshakeFailed,
    AbiLayoutMismatch,
    AbiVersionMismatch,
    AbiReservedFieldInvalid,
    AbiContractMismatch,
    AbiCapabilitiesMissing,
    NativeInvalidArgument,
    NativeUnsupportedOperation,
    NativeRequestTooLarge,
    NativeRuntimePanicked,
    NativeStatusInvalid,
    InvocationFailed,
    NullResponse,
    InvalidResponseBuffer,
    ResponseTooLarge,
    InvalidUtf8,
    InvalidJson,
    UnsupportedContractVersion,
    InvalidBase64,
    InvalidResponse,
    HostPathDisclosure
}

public sealed class MspNativeAdapterException : Exception
{
    private MspNativeAdapterException(
        MspNativeFailureKind failureKind,
        MspNativeOperation? operation,
        MspNativeDiagnostic diagnostic)
        : base(diagnostic.Message)
    {
        FailureKind = failureKind;
        Operation = operation;
        Diagnostic = diagnostic;
    }

    public MspNativeFailureKind FailureKind { get; }

    public MspNativeOperation? Operation { get; }

    public MspNativeDiagnostic Diagnostic { get; }

    public override string ToString()
    {
        return $"{GetType().FullName}: " +
            $"failure={FailureKind}; " +
            $"operation={Operation?.ToString() ?? "none"}; " +
            $"code={Diagnostic.Code}; " +
            $"message={Diagnostic.Message}";
    }

    internal static MspNativeAdapterException Create(
        MspNativeFailureKind failureKind,
        MspNativeOperation? operation)
    {
        var (code, message, recoveryHint) = failureKind switch
        {
            MspNativeFailureKind.PlatformUnsupported => (
                "msp.native.platform_unsupported",
                "The native MSP runtime is supported only on Windows.",
                "Use the managed runtime or run ReadOS on a supported Windows host."),
            MspNativeFailureKind.LibraryUnavailable => (
                "msp.native.library_unavailable",
                "The native MSP runtime is unavailable.",
                "Install or package the matching native MSP runtime."),
            MspNativeFailureKind.ExportUnavailable => (
                "msp.native.export_unavailable",
                "The native MSP runtime does not expose the required contract.",
                "Install a native runtime built for the current ReadOS contract."),
            MspNativeFailureKind.AbiExportSetIncomplete => (
                "msp.native.abi_export_set_incomplete",
                "The native MSP runtime exposes an incomplete ABI v2 contract.",
                "Install a native runtime whose ABI v2 exports are complete and matched."),
            MspNativeFailureKind.AbiHandshakeFailed => (
                "msp.native.abi_handshake_failed",
                "The native MSP runtime ABI handshake failed.",
                "Install a native runtime built for the current ReadOS ABI contract."),
            MspNativeFailureKind.AbiLayoutMismatch => (
                "msp.native.abi_layout_mismatch",
                "The native MSP runtime ABI information layout is incompatible.",
                "Install a native runtime with the required fixed-width ABI layout."),
            MspNativeFailureKind.AbiVersionMismatch => (
                "msp.native.abi_version_mismatch",
                "The native MSP runtime ABI version is incompatible.",
                "Install a native runtime with the supported ABI major and minor version."),
            MspNativeFailureKind.AbiReservedFieldInvalid => (
                "msp.native.abi_reserved_field_invalid",
                "The native MSP runtime ABI reserved field is invalid.",
                "Install a native runtime that zeroes reserved ABI fields."),
            MspNativeFailureKind.AbiContractMismatch => (
                "msp.native.abi_contract_mismatch",
                "The native MSP runtime contract identity is incompatible.",
                "Install the native runtime built for this ReadOS contract."),
            MspNativeFailureKind.AbiCapabilitiesMissing => (
                "msp.native.abi_capabilities_missing",
                "The native MSP runtime is missing required ABI capabilities.",
                "Install a native runtime that supports all required operations."),
            MspNativeFailureKind.NativeInvalidArgument => (
                "msp.native.invalid_argument",
                "The native MSP runtime rejected the invocation arguments.",
                "Install a matching runtime or retry with a valid bounded request."),
            MspNativeFailureKind.NativeUnsupportedOperation => (
                "msp.native.unsupported_operation",
                "The native MSP runtime does not support the requested operation.",
                "Install a runtime that advertises and implements the required operation."),
            MspNativeFailureKind.NativeRequestTooLarge => (
                "msp.native.request_too_large",
                "The native MSP runtime rejected an oversized request.",
                "Reduce the request size or use matching configured safety limits."),
            MspNativeFailureKind.NativeRuntimePanicked => (
                "msp.native.runtime_panicked",
                "The native MSP runtime contained an internal panic.",
                "Retry with a matching runtime; report the failure if it persists."),
            MspNativeFailureKind.NativeStatusInvalid => (
                "msp.native.status_invalid",
                "The native MSP runtime returned an unknown invocation status.",
                "Install a native runtime built for the current ReadOS ABI contract."),
            MspNativeFailureKind.NullResponse => (
                "msp.native.null_response",
                "The native MSP runtime returned no response.",
                "Retry with a matching native runtime; otherwise use the managed runtime."),
            MspNativeFailureKind.InvalidResponseBuffer => (
                "msp.native.invalid_response_buffer",
                "The native MSP runtime returned an invalid response pointer and length pair.",
                "Install a native runtime that implements the matched buffer contract."),
            MspNativeFailureKind.ResponseTooLarge => (
                "msp.native.response_too_large",
                "The native MSP response exceeded the configured safety limit.",
                "Reduce command output or increase the trusted adapter limit."),
            MspNativeFailureKind.InvalidUtf8 => (
                "msp.native.invalid_utf8",
                "The native MSP response was not valid UTF-8 JSON.",
                "Install a native runtime that implements the UTF-8 JSON contract."),
            MspNativeFailureKind.InvalidJson => (
                "msp.native.invalid_json",
                "The native MSP response was not valid contract JSON.",
                "Install a native runtime that implements the current JSON contract."),
            MspNativeFailureKind.UnsupportedContractVersion => (
                "msp.native.unsupported_contract_version",
                "The native MSP contract version is not supported.",
                "Install a native runtime built for the current ReadOS contract."),
            MspNativeFailureKind.InvalidBase64 => (
                "msp.native.invalid_base64",
                "The native MSP byte stream encoding was invalid.",
                "Install a native runtime that emits authoritative Base64 byte streams."),
            MspNativeFailureKind.InvalidResponse => (
                "msp.native.invalid_response",
                "The native MSP response violated the runtime contract.",
                "Install a native runtime built for the current ReadOS contract."),
            MspNativeFailureKind.HostPathDisclosure => (
                "msp.native.host_path_disclosure",
                "The native MSP response exposed a protected host path and was rejected.",
                "Use a runtime that keeps host paths outside model-visible output."),
            _ => (
                "msp.native.invocation_failed",
                "The native MSP invocation failed.",
                "Retry with a matching native runtime; otherwise use the managed runtime.")
        };

        return new MspNativeAdapterException(
            failureKind,
            operation,
            new MspNativeDiagnostic
            {
                Severity = MspNativeDiagnosticSeverity.Error,
                Code = code,
                Message = message,
                Target = operation?.ToString(),
                RecoveryHint = recoveryHint
            });
    }
}
