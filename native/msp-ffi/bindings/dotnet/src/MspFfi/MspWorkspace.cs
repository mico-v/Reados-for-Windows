namespace MspFfi;

/// <summary>
/// Owns a native in-memory virtual workspace. No method accepts or opens a
/// host filesystem path.
/// </summary>
public sealed class MspWorkspace : IDisposable
{
    private readonly object gate = new();
    private SafeMspWorkspaceHandle? handle;
    private int disposed;

    public MspWorkspace()
    {
        MspRuntime.EnsureNativeConfigured();
        var nativeHandle = NativeMethods.WorkspaceCreate();
        if (nativeHandle == IntPtr.Zero)
        {
            throw new MspNativeException("The native ABI returned a null workspace handle.");
        }

        handle = new SafeMspWorkspaceHandle(nativeHandle);
    }

    public static MspWorkspace Create() => new();

    /// <summary>Creates a session retaining the native workspace reference.</summary>
    public MspSession CreateSession()
    {
        lock (gate)
        {
            var workspace = EnsureHandle();
            return MspSession.FromNative(NativeMethods.SessionCreate(workspace));
        }
    }

    /// <summary>Stores arbitrary bytes at a virtual path.</summary>
    public int PutFile(string virtualPath, ReadOnlySpan<byte> data)
    {
        var bytes = Utf8Input.FileBytes(data, nameof(data));
        return InvokeMutation(virtualPath, (workspace, path) => NativeMethods.WorkspacePutFile(
            workspace,
            path,
            bytes.Length == 0 ? null : bytes,
            checked((nuint)bytes.Length)));
    }

    /// <summary>Array convenience overload for <see cref="PutFile(string, ReadOnlySpan{byte})"/>.</summary>
    public int PutFile(string virtualPath, byte[] data)
    {
        ArgumentNullException.ThrowIfNull(data);
        return PutFile(virtualPath, data.AsSpan());
    }

    /// <summary>Adds arbitrary bytes at a virtual path.</summary>
    public int AddFile(string virtualPath, ReadOnlySpan<byte> data)
    {
        var bytes = Utf8Input.FileBytes(data, nameof(data));
        return InvokeMutation(virtualPath, (workspace, path) => NativeMethods.WorkspaceAddFile(
            workspace,
            path,
            bytes.Length == 0 ? null : bytes,
            checked((nuint)bytes.Length)));
    }

    /// <summary>Array convenience overload for <see cref="AddFile(string, ReadOnlySpan{byte})"/>.</summary>
    public int AddFile(string virtualPath, byte[] data)
    {
        ArgumentNullException.ThrowIfNull(data);
        return AddFile(virtualPath, data.AsSpan());
    }

    /// <summary>Creates a virtual directory and any missing parents.</summary>
    public int CreateDirectory(string virtualPath)
    {
        return InvokeMutation(
            virtualPath,
            static (workspace, path) => NativeMethods.WorkspaceCreateDirectory(workspace, path));
    }

    /// <summary>Returns virtual file metadata as an owned result.</summary>
    public MspResult Stat(string virtualPath)
    {
        return InvokePathResult(
            virtualPath,
            static (workspace, path) => NativeMethods.WorkspaceStat(workspace, path));
    }

    /// <summary>Lists direct virtual children as an owned result.</summary>
    public MspResult List(string virtualPath) => ListDirectory(virtualPath);

    /// <summary>Lists direct virtual children as an owned result.</summary>
    public MspResult ListDirectory(string virtualPath)
    {
        return InvokePathResult(
            virtualPath,
            static (workspace, path) => NativeMethods.WorkspaceListDirectory(workspace, path));
    }

    /// <summary>Reads a bounded virtual file range as byte-preserving stdout.</summary>
    public MspResult Read(string virtualPath, ulong offset, int length)
    {
        if (length < 0)
        {
            throw new ArgumentOutOfRangeException(nameof(length));
        }

        return Read(virtualPath, offset, checked((nuint)length));
    }

    /// <summary>Reads a bounded virtual file range as byte-preserving stdout.</summary>
    public MspResult Read(string virtualPath, ulong offset, nuint length)
    {
        return InvokePathResult(
            virtualPath,
            (workspace, path) => NativeMethods.WorkspaceRead(workspace, path, offset, length));
    }

    /// <summary>Compatibility spelling for <see cref="Read(string, ulong, int)"/>.</summary>
    public MspResult ReadFileRange(string virtualPath, ulong offset, int length)
    {
        return Read(virtualPath, offset, length);
    }

    /// <summary>Compatibility spelling for <see cref="Read(string, ulong, nuint)"/>.</summary>
    public MspResult ReadFileRange(string virtualPath, ulong offset, nuint length)
    {
        return InvokePathResult(
            virtualPath,
            (workspace, path) => NativeMethods.WorkspaceReadFileRange(
                workspace,
                path,
                offset,
                length));
    }

    public void Dispose()
    {
        lock (gate)
        {
            if (Interlocked.Exchange(ref disposed, 1) == 0)
            {
                handle?.Dispose();
            }
        }

        GC.SuppressFinalize(this);
    }

    private int InvokeMutation(
        string virtualPath,
        Func<SafeMspWorkspaceHandle, IntPtr, int> operation)
    {
        lock (gate)
        {
            var workspace = EnsureHandle();
            using var path = Utf8Input.VirtualPath(virtualPath, nameof(virtualPath));
            return operation(workspace, path.Pointer);
        }
    }

    private MspResult InvokePathResult(
        string virtualPath,
        Func<SafeMspWorkspaceHandle, IntPtr, IntPtr> operation)
    {
        lock (gate)
        {
            var workspace = EnsureHandle();
            using var path = Utf8Input.VirtualPath(virtualPath, nameof(virtualPath));
            return MspResult.FromNative(operation(workspace, path.Pointer));
        }
    }

    private SafeMspWorkspaceHandle EnsureHandle()
    {
        ObjectDisposedException.ThrowIf(Volatile.Read(ref disposed) != 0, this);
        return handle ?? throw new ObjectDisposedException(nameof(MspWorkspace));
    }
}
