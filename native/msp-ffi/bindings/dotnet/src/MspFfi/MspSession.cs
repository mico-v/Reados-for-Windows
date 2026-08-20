namespace MspFfi;

/// <summary>
/// Owns a native session. Its command surface is the registered in-process
/// command ABI; this wrapper contains no process-launch API.
/// </summary>
public sealed class MspSession : IDisposable
{
    private readonly object gate = new();
    private SafeMspSessionHandle? handle;
    private int disposed;

    private MspSession(SafeMspSessionHandle nativeHandle)
    {
        handle = nativeHandle;
    }

    public static MspSession CreateDefault()
    {
        MspRuntime.EnsureNativeConfigured();
        return FromNative(NativeMethods.SessionCreateDefault());
    }

    public static MspSession Create() => CreateDefault();

    public static MspSession Create(MspWorkspace workspace)
    {
        ArgumentNullException.ThrowIfNull(workspace);
        return workspace.CreateSession();
    }

    /// <summary>Runs one strict UTF-8 command through the native ABI.</summary>
    public MspResult Run(string command)
    {
        return Run(Utf8Input.Command(command, nameof(command)));
    }

    /// <summary>
    /// Runs explicit command bytes after strict UTF-8 and embedded-NUL validation.
    /// </summary>
    public MspResult Run(ReadOnlySpan<byte> command)
    {
        var bytes = Utf8Input.Command(command, nameof(command));
        lock (gate)
        {
            var session = EnsureHandle();
            return MspResult.FromNative(NativeMethods.SessionRunN(
                session,
                bytes.Length == 0 ? null : bytes,
                checked((nuint)bytes.Length)));
        }
    }

    /// <summary>Array convenience overload for <see cref="Run(ReadOnlySpan{byte})"/>.</summary>
    public MspResult Run(byte[] command)
    {
        ArgumentNullException.ThrowIfNull(command);
        return Run(command.AsSpan());
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

    internal static MspSession FromNative(IntPtr nativeHandle)
    {
        if (nativeHandle == IntPtr.Zero)
        {
            throw new MspNativeException("The native ABI returned a null session handle.");
        }

        return new MspSession(new SafeMspSessionHandle(nativeHandle));
    }

    private SafeMspSessionHandle EnsureHandle()
    {
        ObjectDisposedException.ThrowIf(Volatile.Read(ref disposed) != 0, this);
        return handle ?? throw new ObjectDisposedException(nameof(MspSession));
    }
}
