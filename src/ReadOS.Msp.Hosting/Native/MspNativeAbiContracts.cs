using System.Runtime.InteropServices;

namespace ReadOS.Msp.Hosting.Native;

public enum MspNativeAbiMode
{
    Unknown,
    LegacyV1,
    LengthDelimitedV2
}

public sealed record MspNativeRuntimeInfo(
    MspNativeAbiMode AbiMode,
    uint MajorVersion,
    uint MinorVersion,
    ulong ContractId,
    ulong Capabilities)
{
    public static MspNativeRuntimeInfo Unknown { get; } = new(
        MspNativeAbiMode.Unknown,
        0,
        0,
        0,
        0);
}

public interface IMspNativeRuntimeInfoProvider
{
    MspNativeRuntimeInfo NativeRuntimeInfo { get; }
}

[Flags]
internal enum MspNativeAbiV2Capabilities : ulong
{
    None = 0,
    LengthDelimitedJson = 1UL << 0,
    Execute = 1UL << 1,
    Parse = 1UL << 2,
    Normalize = 1UL << 3,
    WorkspaceRead = 1UL << 4
}

internal enum MspNativeInvokeStatusV2
{
    Ok = 0,
    InvalidArgument = 1,
    UnsupportedOperation = 2,
    RequestTooLarge = 3,
    ResponseTooLarge = 4,
    Panic = 5
}

[StructLayout(LayoutKind.Sequential, Pack = 8, Size = 32)]
internal struct MspAbiInfoV2
{
    public uint Size;

    public uint MajorVersion;

    public uint MinorVersion;

    public uint Reserved;

    public ulong ContractId;

    public ulong Capabilities;
}
