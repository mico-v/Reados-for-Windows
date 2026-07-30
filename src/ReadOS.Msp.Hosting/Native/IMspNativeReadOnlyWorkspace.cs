namespace ReadOS.Msp.Hosting.Native;

/// <summary>
/// Safe managed read surface exposed to the native WorkspaceFS callback layer.
/// Implementations must return only model-visible virtual metadata and bytes.
/// </summary>
public interface IMspNativeReadOnlyWorkspace
{
    ValueTask<MspNativeWorkspaceFileInfo> StatAsync(
        string virtualPath,
        CancellationToken cancellationToken = default);

    ValueTask<IReadOnlyList<MspNativeWorkspaceDirectoryEntry>> ListDirectoryAsync(
        string virtualPath,
        CancellationToken cancellationToken = default);

    ValueTask<ReadOnlyMemory<byte>> ReadFileRangeAsync(
        string virtualPath,
        ulong offset,
        int length,
        CancellationToken cancellationToken = default);
}

public enum MspNativeWorkspaceFileType
{
    RegularFile,
    Directory,
    SymbolicLink,
    Other
}

public sealed record MspNativeWorkspaceFileInfo
{
    public required MspNativeWorkspaceFileType FileType { get; init; }

    public ulong? SizeBytes { get; init; }

    public long? ModificationTimeUnixMs { get; init; }

    public string? FileIdentity { get; init; }
}

public sealed record MspNativeWorkspaceDirectoryEntry
{
    public required string Name { get; init; }

    public required MspNativeWorkspaceFileInfo Info { get; init; }
}

public enum MspNativeWorkspaceErrorKind
{
    NotFound,
    NotDirectory,
    IsDirectory,
    AccessDenied,
    HiddenPath,
    InvalidPath,
    LimitExceeded,
    Unsupported,
    Io
}

/// <summary>
/// Reports one closed, host-path-free WorkspaceFS error to the callback layer.
/// The exception message is intentionally generic and is never sent to native code.
/// </summary>
public sealed class MspNativeWorkspaceException : Exception
{
    public MspNativeWorkspaceException(MspNativeWorkspaceErrorKind errorKind)
        : base("The managed native workspace operation failed.")
    {
        ErrorKind = errorKind;
    }

    public MspNativeWorkspaceErrorKind ErrorKind { get; }
}
