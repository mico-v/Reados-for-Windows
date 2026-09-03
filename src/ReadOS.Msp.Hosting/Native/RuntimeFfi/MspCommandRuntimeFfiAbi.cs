namespace ReadOS.Msp.Hosting.Native.RuntimeFfi;

/// <summary>
/// Managed names for the versioned modular runtime FFI contract. Numeric
/// values and the export order are checked against the canonical ABI manifest
/// by <c>verify-msp-command-runtime-ffi-abi.ps1</c>.
/// </summary>
internal static class MspCommandRuntimeFfiAbi
{
    internal const uint AbiVersion = 1;
    internal const uint RequestSchemaVersion = 1;
    internal const string HeaderVersion = "0.1.0";
}
