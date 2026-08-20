using System.Runtime.InteropServices;
using System.Text;

namespace MspFfi;

internal static class Utf8Input
{
    internal const int MaxCommandBytes = 64 * 1024;
    internal const int MaxPathBytes = 4 * 1024;
    internal const int MaxFileBytes = 8 * 1024 * 1024;
    internal const int MaxReadBytes = 1024 * 1024;

    private static readonly UTF8Encoding Strict = new(encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: true);

    internal static byte[] Command(string value, string parameterName)
    {
        return Utf8Bytes(value, parameterName, MaxCommandBytes);
    }

    internal static byte[] Command(ReadOnlySpan<byte> value, string parameterName)
    {
        return Utf8Bytes(value, parameterName, MaxCommandBytes);
    }

    internal static Utf8CString VirtualPath(string value, string parameterName)
    {
        ArgumentNullException.ThrowIfNull(value, parameterName);
        if (value.Length == 0 || value[0] != '/')
        {
            throw new ArgumentException(
                "The path must be an absolute virtual POSIX path.",
                parameterName);
        }

        if (value.IndexOf('\\') >= 0
            || value.IndexOf(':') >= 0
            || value.StartsWith("//", StringComparison.Ordinal))
        {
            throw new ArgumentException(
                "Host, UNC, drive, and backslash paths are not virtual workspace paths.",
                parameterName);
        }

        var bytes = Utf8Bytes(value, parameterName, MaxPathBytes);
        return new Utf8CString(bytes);
    }

    internal static byte[] FileBytes(ReadOnlySpan<byte> value, string parameterName)
    {
        if (value.Length > MaxFileBytes)
        {
            throw new ArgumentOutOfRangeException(
                parameterName,
                value.Length,
                $"File contents cannot exceed {MaxFileBytes} UTF-8-independent bytes.");
        }

        return value.ToArray();
    }

    private static byte[] Utf8Bytes(string value, string parameterName, int maximumBytes)
    {
        ArgumentNullException.ThrowIfNull(value, parameterName);
        try
        {
            return Utf8Bytes(Strict.GetBytes(value), parameterName, maximumBytes);
        }
        catch (EncoderFallbackException exception)
        {
            throw new ArgumentException(
                "The value is not valid UTF-8.",
                parameterName,
                exception);
        }
    }

    private static byte[] Utf8Bytes(
        ReadOnlySpan<byte> value,
        string parameterName,
        int maximumBytes)
    {
        if (value.Length > maximumBytes)
        {
            throw new ArgumentOutOfRangeException(
                parameterName,
                value.Length,
                $"UTF-8 input cannot exceed {maximumBytes} bytes.");
        }

        if (value.Contains((byte)0))
        {
            throw new ArgumentException(
                "UTF-8 input cannot contain an embedded NUL byte.",
                parameterName);
        }

        try
        {
            _ = Strict.GetCharCount(value);
        }
        catch (DecoderFallbackException exception)
        {
            throw new ArgumentException(
                "The value is not valid UTF-8.",
                parameterName,
                exception);
        }

        return value.ToArray();
    }
}

/// <summary>Owns one temporary NUL-terminated UTF-8 argument buffer.</summary>
internal sealed class Utf8CString : IDisposable
{
    private IntPtr pointer;

    internal Utf8CString(byte[] utf8Bytes)
    {
        pointer = Marshal.AllocHGlobal(utf8Bytes.Length + 1);
        try
        {
            Marshal.Copy(utf8Bytes, 0, pointer, utf8Bytes.Length);
            Marshal.WriteByte(pointer, utf8Bytes.Length, 0);
        }
        catch
        {
            Marshal.FreeHGlobal(pointer);
            pointer = IntPtr.Zero;
            throw;
        }
    }

    internal IntPtr Pointer
    {
        get
        {
            ObjectDisposedException.ThrowIf(pointer == IntPtr.Zero, this);
            return pointer;
        }
    }

    public void Dispose()
    {
        var value = Interlocked.Exchange(ref pointer, IntPtr.Zero);
        if (value != IntPtr.Zero)
        {
            Marshal.FreeHGlobal(value);
        }
    }
}
