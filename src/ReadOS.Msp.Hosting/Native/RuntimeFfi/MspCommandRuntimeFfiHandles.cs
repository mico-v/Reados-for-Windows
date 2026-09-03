using Microsoft.Win32.SafeHandles;
using System.Runtime.InteropServices;

namespace ReadOS.Msp.Hosting.Native.RuntimeFfi;

internal sealed class MspCommandRuntimeFfiRuntimeSafeHandle : SafeHandleZeroOrMinusOneIsInvalid
{
    private readonly MspCommandRuntimeFfiRuntimeFreeDelegate free;
    private readonly IDisposable lease;

    internal MspCommandRuntimeFfiRuntimeSafeHandle(
        nint nativeHandle,
        MspCommandRuntimeFfiRuntimeFreeDelegate free,
        IDisposable lease)
        : base(ownsHandle: true)
    {
        this.free = free;
        this.lease = lease;
        SetHandle(nativeHandle);
    }

    protected override bool ReleaseHandle()
    {
        try
        {
            if (!IsInvalid)
            {
                try
                {
                    free(handle);
                }
                catch (Exception exception) when (!IsFatal(exception))
                {
                    // SafeHandle release must not escape finalization.
                }
                finally
                {
                    handle = nint.Zero;
                }
            }
        }
        finally
        {
            lease.Dispose();
        }

        return true;
    }

    private static bool IsFatal(Exception exception) =>
        exception is OutOfMemoryException or StackOverflowException;
}

internal sealed class MspCommandRuntimeFfiWorkspaceSafeHandle : SafeHandleZeroOrMinusOneIsInvalid
{
    private readonly MspCommandRuntimeFfiWorkspaceFreeDelegate free;
    private readonly IDisposable lease;

    internal MspCommandRuntimeFfiWorkspaceSafeHandle(
        nint nativeHandle,
        MspCommandRuntimeFfiWorkspaceFreeDelegate free,
        IDisposable lease)
        : base(ownsHandle: true)
    {
        this.free = free;
        this.lease = lease;
        SetHandle(nativeHandle);
    }

    protected override bool ReleaseHandle()
    {
        try
        {
            if (!IsInvalid)
            {
                try
                {
                    free(handle);
                }
                catch (Exception exception) when (!IsFatal(exception))
                {
                    // SafeHandle release must not escape finalization.
                }
                finally
                {
                    handle = nint.Zero;
                }
            }
        }
        finally
        {
            lease.Dispose();
        }

        return true;
    }

    private static bool IsFatal(Exception exception) =>
        exception is OutOfMemoryException or StackOverflowException;
}

internal sealed class MspCommandRuntimeFfiResultSafeHandle : SafeHandleZeroOrMinusOneIsInvalid
{
    private readonly MspCommandRuntimeFfiResultFreeDelegate free;
    private readonly IDisposable lease;

    internal MspCommandRuntimeFfiResultSafeHandle(
        nint nativeHandle,
        MspCommandRuntimeFfiResultFreeDelegate free,
        IDisposable lease)
        : base(ownsHandle: true)
    {
        this.free = free;
        this.lease = lease;
        SetHandle(nativeHandle);
    }

    protected override bool ReleaseHandle()
    {
        try
        {
            if (!IsInvalid)
            {
                try
                {
                    free(handle);
                }
                catch (Exception exception) when (!IsFatal(exception))
                {
                    // SafeHandle release must not escape finalization.
                }
                finally
                {
                    handle = nint.Zero;
                }
            }
        }
        finally
        {
            lease.Dispose();
        }

        return true;
    }

    private static bool IsFatal(Exception exception) =>
        exception is OutOfMemoryException or StackOverflowException;
}

internal sealed class MspCommandRuntimeFfiNativeReference : IDisposable
{
    private SafeHandle? handle;

    private MspCommandRuntimeFfiNativeReference(SafeHandle handle)
    {
        this.handle = handle;
        Pointer = handle.DangerousGetHandle();
    }

    internal nint Pointer { get; }

    internal static MspCommandRuntimeFfiNativeReference Acquire(SafeHandle handle)
    {
        ArgumentNullException.ThrowIfNull(handle);
        var added = false;
        try
        {
            handle.DangerousAddRef(ref added);
            if (!added || handle.IsInvalid)
            {
                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.NullHandle);
            }

            return new MspCommandRuntimeFfiNativeReference(handle);
        }
        catch
        {
            if (added)
            {
                handle.DangerousRelease();
            }

            throw;
        }
    }

    public void Dispose()
    {
        Interlocked.Exchange(ref handle, null)?.DangerousRelease();
    }
}

public sealed class MspCommandRuntimeFfiRuntime : IDisposable
{
    private readonly object gate = new();
    private readonly MspCommandRuntimeFfiRuntimeSafeHandle nativeHandle;
    private int disposed;

    internal MspCommandRuntimeFfiRuntime(
        object moduleIdentity,
        MspCommandRuntimeFfiRuntimeSafeHandle nativeHandle)
    {
        ModuleIdentity = moduleIdentity;
        this.nativeHandle = nativeHandle;
    }

    internal object ModuleIdentity { get; }

    internal MspCommandRuntimeFfiNativeReference AcquireNativeReference()
    {
        lock (gate)
        {
            ThrowIfDisposed();
            return MspCommandRuntimeFfiNativeReference.Acquire(nativeHandle);
        }
    }

    public void Dispose()
    {
        lock (gate)
        {
            if (Interlocked.Exchange(ref disposed, 1) == 0)
            {
                nativeHandle.Dispose();
            }
        }

        GC.SuppressFinalize(this);
    }

    private void ThrowIfDisposed()
    {
        if (Volatile.Read(ref disposed) != 0)
        {
            throw new ObjectDisposedException(nameof(MspCommandRuntimeFfiRuntime));
        }
    }
}

public sealed class MspCommandRuntimeFfiWorkspace : IDisposable
{
    private readonly object gate = new();
    private readonly MspCommandRuntimeFfiWorkspaceSafeHandle nativeHandle;
    private readonly MspCommandRuntimeFfiWorkspacePutFileDelegate putFile;
    private readonly MspCommandRuntimeFfiLimits limits;
    private readonly Dictionary<string, int> fileSizes = new(StringComparer.Ordinal);
    private long totalFileBytes;
    private int fileCount;
    private int disposed;

    internal MspCommandRuntimeFfiWorkspace(
        object moduleIdentity,
        MspCommandRuntimeFfiWorkspaceSafeHandle nativeHandle,
        MspCommandRuntimeFfiWorkspacePutFileDelegate putFile,
        MspCommandRuntimeFfiLimits limits)
    {
        ModuleIdentity = moduleIdentity;
        this.nativeHandle = nativeHandle;
        this.putFile = putFile;
        this.limits = limits;
    }

    internal object ModuleIdentity { get; }

    public void PutFile(string virtualPath, ReadOnlySpan<byte> data)
    {
        lock (gate)
        {
            ThrowIfDisposed();
            _ = MspCommandRuntimeFfiUtf8.Encode(virtualPath, limits.MaximumVirtualPathBytes);
            if (!MspCommandRuntimeFfiUtf8.IsValidVirtualPath(
                    virtualPath,
                    allowRoot: false,
                    limits.MaximumVirtualPathBytes))
            {
                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
            }

            if (data.Length > limits.MaximumFileBytes)
            {
                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LimitExceeded);
            }

            var hasExistingFile = fileSizes.TryGetValue(virtualPath, out var existingSize);
            var previousSize = hasExistingFile ? existingSize : 0;
            var nextTotal = checked(totalFileBytes - previousSize + data.Length);
            var nextCount = fileCount + (hasExistingFile ? 0 : 1);
            if (nextTotal > limits.MaximumWorkspaceBytes || nextCount > limits.MaximumWorkspaceFiles)
            {
                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LimitExceeded);
            }

            var pathBytes = MspCommandRuntimeFfiUtf8.Encode(
                virtualPath,
                limits.MaximumVirtualPathBytes);
            using var reference = MspCommandRuntimeFfiNativeReference.Acquire(nativeHandle);
            using var pathPin = PinnedBuffer.Create(pathBytes);
            using var dataPin = PinnedBuffer.Create(data);
            int status;
            try
            {
                status = putFile(
                    reference.Pointer,
                    pathPin.Pointer,
                    checked((nuint)pathBytes.Length),
                    dataPin.Pointer,
                    checked((nuint)data.Length));
            }
            catch (Exception exception) when (!IsFatal(exception))
            {
                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.NativeInternalError);
            }

            if (status != (int)MspCommandRuntimeFfiStatus.Ok)
            {
                throw MspCommandRuntimeFfiException.ForStatus(status);
            }

            fileSizes[virtualPath] = data.Length;
            totalFileBytes = nextTotal;
            fileCount = nextCount;
        }
    }

    internal IDisposable EnterOperation()
    {
        Monitor.Enter(gate);
        try
        {
            ThrowIfDisposed();
            return new GateLease(gate);
        }
        catch
        {
            Monitor.Exit(gate);
            throw;
        }
    }

    internal MspCommandRuntimeFfiNativeReference AcquireNativeReferenceUnderOperation()
    {
        ThrowIfDisposed();
        return MspCommandRuntimeFfiNativeReference.Acquire(nativeHandle);
    }

    public void Dispose()
    {
        lock (gate)
        {
            if (Interlocked.Exchange(ref disposed, 1) == 0)
            {
                nativeHandle.Dispose();
            }
        }

        GC.SuppressFinalize(this);
    }

    private void ThrowIfDisposed()
    {
        if (Volatile.Read(ref disposed) != 0)
        {
            throw new ObjectDisposedException(nameof(MspCommandRuntimeFfiWorkspace));
        }
    }

    private static bool IsFatal(Exception exception) =>
        exception is OutOfMemoryException or StackOverflowException;

    private sealed class GateLease(object gate) : IDisposable
    {
        private object? gate = gate;

        public void Dispose()
        {
            var current = Interlocked.Exchange(ref gate, null);
            if (current is not null)
            {
                Monitor.Exit(current);
            }
        }
    }
}

internal sealed class PinnedBuffer : IDisposable
{
    private GCHandle handle;

    private PinnedBuffer(GCHandle handle, nint pointer)
    {
        this.handle = handle;
        Pointer = pointer;
    }

    internal nint Pointer { get; }

    internal static PinnedBuffer Create(ReadOnlySpan<byte> bytes)
    {
        if (bytes.Length == 0)
        {
            return new PinnedBuffer(default, nint.Zero);
        }

        var array = bytes.ToArray();
        var handle = GCHandle.Alloc(array, GCHandleType.Pinned);
        return new PinnedBuffer(handle, handle.AddrOfPinnedObject());
    }

    internal static PinnedBuffer Create(byte[] bytes)
    {
        ArgumentNullException.ThrowIfNull(bytes);
        if (bytes.Length == 0)
        {
            return new PinnedBuffer(default, nint.Zero);
        }

        var handle = GCHandle.Alloc(bytes, GCHandleType.Pinned);
        return new PinnedBuffer(handle, handle.AddrOfPinnedObject());
    }

    public void Dispose()
    {
        if (handle.IsAllocated)
        {
            handle.Free();
        }
    }
}
