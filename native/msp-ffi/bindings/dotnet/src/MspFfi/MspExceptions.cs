namespace MspFfi;

/// <summary>Reports a failure at the native MSP FFI boundary.</summary>
public sealed class MspNativeException : Exception
{
    public MspNativeException(string message)
        : base(message)
    {
    }

    public MspNativeException(string message, Exception innerException)
        : base(message, innerException)
    {
    }
}

/// <summary>Reports a non-zero exit result returned by the native ABI.</summary>
public sealed class MspResultException : Exception
{
    public MspResultException(string operation, int exitCode, byte[] stderrBytes)
        : base(CreateMessage(operation, exitCode, stderrBytes))
    {
        Operation = operation;
        ExitCode = exitCode;
        StderrBytes = stderrBytes.ToArray();
    }

    public string Operation { get; }

    public int ExitCode { get; }

    /// <summary>Copies the native diagnostic bytes without decoding them.</summary>
    public byte[] StderrBytes { get; }

    private static string CreateMessage(string operation, int exitCode, byte[] stderrBytes)
    {
        ArgumentException.ThrowIfNullOrEmpty(operation);
        var diagnostic = stderrBytes.Length == 0
            ? "no native diagnostic"
            : $"stderr {Convert.ToHexString(stderrBytes)}";
        return $"{operation} failed with exit code {exitCode} ({diagnostic}).";
    }
}

/// <summary>Stable status values returned by workspace mutation calls.</summary>
public static class MspStatus
{
    public const int Ok = 0;
    public const int Error = 1;
    public const int InvalidArgument = 2;
    public const int LimitExceeded = 3;
    public const int Panic = 4;
}

/// <summary>Runtime ABI information reported by the loaded native library.</summary>
public readonly record struct MspRuntimeInfo(uint AbiVersion, string Version);
