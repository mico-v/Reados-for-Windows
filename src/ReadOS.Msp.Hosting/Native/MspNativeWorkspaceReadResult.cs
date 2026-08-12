namespace ReadOS.Msp.Hosting.Native;

/// <summary>
/// One model-visible workspace read requested through the native MSP runtime.
/// The enum values map to the operation-4 wire names stat, listDirectory, and
/// readFileRange and mirror the reverse-P/Invoke operations served by
/// <see cref="WindowsMspNativeWorkspaceCallbacks"/>.
/// </summary>
public enum MspNativeWorkspaceReadOperation
{
    Stat,
    ListDirectory,
    ReadFileRange
}

/// <summary>
/// Discriminated result of one native workspace read. Exactly one of
/// <see cref="FileInfoResult"/>, <see cref="EntriesResult"/>, or
/// <see cref="BytesResult"/> is produced by a successful call; a closed
/// workspace failure surfaces as <see cref="MspNativeWorkspaceException"/>.
/// </summary>
public abstract record MspNativeWorkspaceReadResult
{
    public static MspNativeWorkspaceReadResult FileInfo(
        MspNativeWorkspaceFileInfo fileInfo)
    {
        ArgumentNullException.ThrowIfNull(fileInfo);
        return new FileInfoResult(fileInfo);
    }

    public static MspNativeWorkspaceReadResult Entries(
        IReadOnlyList<MspNativeWorkspaceDirectoryEntry> entries)
    {
        ArgumentNullException.ThrowIfNull(entries);
        return new EntriesResult(entries);
    }

    public static MspNativeWorkspaceReadResult Bytes(ReadOnlyMemory<byte> bytes)
    {
        return new BytesResult(bytes);
    }

    private MspNativeWorkspaceReadResult()
    {
    }

    public sealed record FileInfoResult : MspNativeWorkspaceReadResult
    {
        public FileInfoResult(MspNativeWorkspaceFileInfo fileInfo)
        {
            FileInfo = fileInfo;
        }

        public new MspNativeWorkspaceFileInfo FileInfo { get; }
    }

    public sealed record EntriesResult : MspNativeWorkspaceReadResult
    {
        public EntriesResult(IReadOnlyList<MspNativeWorkspaceDirectoryEntry> entries)
        {
            Entries = entries;
        }

        public new IReadOnlyList<MspNativeWorkspaceDirectoryEntry> Entries { get; }
    }

    public sealed record BytesResult : MspNativeWorkspaceReadResult
    {
        public BytesResult(ReadOnlyMemory<byte> bytes)
        {
            Bytes = bytes;
        }

        public new ReadOnlyMemory<byte> Bytes { get; }
    }
}

internal sealed record MspNativeWorkspaceReadRequestWireV1
{
    public required ulong Host { get; init; }

    public ulong? CallbackBaseId { get; init; }

    public IReadOnlyList<MspNativeWorkspaceReadMountWireV1>? Mounts { get; init; }

    public required MspNativeWorkspaceReadOperation Operation { get; init; }

    public required string VirtualPath { get; init; }

    public ulong Offset { get; init; }

    public ulong Length { get; init; }
}

internal sealed record MspNativeWorkspaceReadMountWireV1
{
    public required string Path { get; init; }

    public required ulong BackendId { get; init; }
}

internal sealed record MspNativeWorkspaceReadResponseWireV1
{
    public bool Ok { get; init; }

    public MspNativeWorkspaceFileInfoWireV1? FileInfo { get; init; }

    public IReadOnlyList<MspNativeWorkspaceDirectoryEntryWireV1>? Entries { get; init; }

    public string? BytesBase64 { get; init; }

    public string? ErrorKind { get; init; }

    public bool Canceled { get; init; }
}
