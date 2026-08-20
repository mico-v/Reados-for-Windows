using System.Runtime.InteropServices;

namespace MspFfi;

/// <summary>
/// Owns one native result. The byte accessors copy arbitrary native bytes and
/// never assume NUL termination or UTF-8 text.
/// </summary>
public sealed class MspResult : IDisposable
{
    private readonly object gate = new();
    private SafeMspResultHandle? handle;
    private int disposed;

    internal MspResult(IntPtr nativeHandle)
    {
        if (nativeHandle == IntPtr.Zero)
        {
            throw new MspNativeException("The native ABI returned a null result handle.");
        }

        handle = new SafeMspResultHandle(nativeHandle);
    }

    internal static MspResult FromNative(IntPtr nativeHandle)
    {
        return new MspResult(nativeHandle);
    }

    public int ExitCode
    {
        get
        {
            lock (gate)
            {
                var result = EnsureHandle();
                return NativeMethods.ResultExitCode(result);
            }
        }
    }

    /// <summary>Compatibility spelling for <see cref="ExitCode"/>.</summary>
    public int Status => ExitCode;

    /// <summary>Gets whether the native result has a zero exit code.</summary>
    public bool IsSuccess => ExitCode == MspStatus.Ok;

    /// <summary>Copies stdout while preserving every byte.</summary>
    public byte[] StdoutBytes
    {
        get
        {
            lock (gate)
            {
                return CopyData(EnsureHandle(), NativeMethods.ResultStdoutData);
            }
        }
    }

    /// <summary>Copies stderr while preserving every byte.</summary>
    public byte[] StderrBytes
    {
        get
        {
            lock (gate)
            {
                return CopyData(EnsureHandle(), NativeMethods.ResultStderrData);
            }
        }
    }

    /// <summary>Compatibility spelling for <see cref="StdoutBytes"/>.</summary>
    public byte[] Stdout => StdoutBytes;

    /// <summary>Compatibility spelling for <see cref="StderrBytes"/>.</summary>
    public byte[] Stderr => StderrBytes;

    /// <summary>
    /// Throws a managed exception containing a copy of native stderr when the
    /// result has a non-zero exit code.
    /// </summary>
    public void EnsureSuccess(string operation)
    {
        ArgumentException.ThrowIfNullOrEmpty(operation);

        lock (gate)
        {
            var result = EnsureHandle();
            var exitCode = NativeMethods.ResultExitCode(result);
            if (exitCode == MspStatus.Ok)
            {
                return;
            }

            throw new MspResultException(
                operation,
                exitCode,
                CopyData(result, NativeMethods.ResultStderrData));
        }
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

    private SafeMspResultHandle EnsureHandle()
    {
        ObjectDisposedException.ThrowIf(Volatile.Read(ref disposed) != 0, this);
        return handle ?? throw new ObjectDisposedException(nameof(MspResult));
    }

    private delegate IntPtr ResultDataAccessor(
        SafeMspResultHandle result,
        out nuint length);

    private static byte[] CopyData(
        SafeMspResultHandle result,
        ResultDataAccessor accessor)
    {
        var pointer = accessor(result, out var length);
        if (length == 0)
        {
            return Array.Empty<byte>();
        }

        if (length > int.MaxValue)
        {
            throw new MspNativeException("The native result buffer is too large for a managed byte array.");
        }

        if (pointer == IntPtr.Zero)
        {
            throw new MspNativeException("The native result reported bytes with a null pointer.");
        }

        var output = new byte[(int)length];
        Marshal.Copy(pointer, output, 0, output.Length);
        return output;
    }
}
