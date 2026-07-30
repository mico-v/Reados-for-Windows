using System.Runtime.InteropServices;

namespace ReadOS.Msp.Hosting.Native;

[Flags]
internal enum MspNativeWorkspaceHostCapabilitiesV1 : ulong
{
    None = 0,
    Stat = 1UL << 0,
    ListDirectory = 1UL << 1,
    ReadFileRange = 1UL << 2,
    Cancellation = 1UL << 3,
    Required = Stat | ListDirectory | ReadFileRange | Cancellation
}

internal enum MspNativeWorkspaceOperationV1 : uint
{
    Stat = 1,
    ListDirectory = 2,
    ReadFileRange = 3
}

internal enum MspNativeWorkspaceCallbackStatusV1
{
    Ok = 0,
    InvalidArgument = 1,
    NotFound = 2,
    NotDirectory = 3,
    IsDirectory = 4,
    AccessDenied = 5,
    HiddenPath = 6,
    InvalidPath = 7,
    LimitExceeded = 8,
    Unsupported = 9,
    Canceled = 10,
    Io = 11
}

[StructLayout(LayoutKind.Sequential, Pack = 8, Size = MspNativeWorkspaceAbiV1.HostSize)]
internal struct MspNativeWorkspaceHostV1
{
    public uint Size;

    public uint MajorVersion;

    public uint MinorVersion;

    public uint Reserved;

    public ulong Capabilities;

    public nint Context;

    public nint Invoke;

    public nint Free;

    public nint IsCancelled;

    public ulong Reserved2;
}

[StructLayout(LayoutKind.Sequential, Pack = 8, Size = MspNativeWorkspaceAbiV1.RequestSize)]
internal struct MspNativeWorkspaceRequestV1
{
    public uint Size;

    public uint Operation;

    public ulong BackendId;

    public nint PathUtf8;

    public ulong PathLength;

    public ulong Offset;

    public ulong Length;

    public ulong Reserved;
}

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate int MspNativeWorkspaceInvokeV1Function(
    nint context,
    in MspNativeWorkspaceRequestV1 request,
    ref nint response,
    ref ulong responseLength);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate void MspNativeWorkspaceFreeV1Function(
    nint context,
    nint response,
    ulong responseLength);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate int MspNativeWorkspaceIsCancelledV1Function(nint context);

internal static class MspNativeWorkspaceAbiV1
{
    public const uint MajorVersion = 1;
    public const uint MinorVersion = 0;
    public const int HostSize = 64;
    public const int RequestSize = 56;
    public const int MaximumMountCount = 32;
    public const int MaximumPathBytes = 32 * 1024;
    public const int MaximumEntryNameBytes = 4 * 1024;
    public const int MaximumFileIdentityBytes = 4 * 1024;
    public const int MaximumDirectoryEntries = 65_536;
    public const int MaximumStatResponseBytes = 64 * 1024;
    public const int MaximumListResponseBytes = 8 * 1024 * 1024;
    public const int MaximumReadRangeBytes = 1024 * 1024;

    public const ulong RequiredCapabilities =
        (ulong)MspNativeWorkspaceHostCapabilitiesV1.Required;
}

internal sealed record MspNativeWorkspaceFileInfoWireV1
{
    public required MspNativeWorkspaceFileType FileType { get; init; }

    public ulong? SizeBytes { get; init; }

    public long? ModificationTimeUnixMs { get; init; }

    public string? FileIdentity { get; init; }
}

internal sealed record MspNativeWorkspaceDirectoryEntryWireV1
{
    public required string Name { get; init; }

    public required MspNativeWorkspaceFileInfoWireV1 Info { get; init; }
}
