namespace ReadOS.Msp.Hosting.Native;

public sealed record MspNativeAdapterLimits
{
    public const int DefaultMaximumRequestBytes = 1024 * 1024;
    public const int DefaultMaximumResponseBytes = 16 * 1024 * 1024;
    public const int DefaultMaximumDecodedStreamBytes = 8 * 1024 * 1024;
    public const int AbsoluteMaximumRequestBytes = 16 * 1024 * 1024;
    public const int AbsoluteMaximumResponseBytes = 64 * 1024 * 1024;
    public const int AbsoluteMaximumDecodedStreamBytes = 32 * 1024 * 1024;

    public int MaximumRequestBytes { get; init; } = DefaultMaximumRequestBytes;

    public int MaximumResponseBytes { get; init; } = DefaultMaximumResponseBytes;

    public int MaximumDecodedStreamBytes { get; init; } = DefaultMaximumDecodedStreamBytes;

    internal void Validate()
    {
        ValidateBound(
            MaximumRequestBytes,
            AbsoluteMaximumRequestBytes,
            nameof(MaximumRequestBytes));
        ValidateBound(
            MaximumResponseBytes,
            AbsoluteMaximumResponseBytes,
            nameof(MaximumResponseBytes));
        ValidateBound(
            MaximumDecodedStreamBytes,
            AbsoluteMaximumDecodedStreamBytes,
            nameof(MaximumDecodedStreamBytes));
    }

    private static void ValidateBound(int value, int absoluteMaximum, string parameterName)
    {
        if (value <= 0 || value > absoluteMaximum)
        {
            throw new ArgumentOutOfRangeException(
                parameterName,
                "Native MSP size limits must be positive and within the trusted hard cap.");
        }
    }
}

public sealed record MspNativeLibraryOptions
{
    public required string LibraryPath { get; init; }

    public MspNativeAdapterLimits Limits { get; init; } = new();

    internal string ValidateAndGetFullPath()
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(LibraryPath);
        ArgumentNullException.ThrowIfNull(Limits);
        Limits.Validate();

        if (!Path.IsPathFullyQualified(LibraryPath))
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.LibraryUnavailable,
                null);
        }

        string fullPath;
        try
        {
            fullPath = Path.GetFullPath(LibraryPath);
        }
        catch (Exception exception) when (exception is ArgumentException or NotSupportedException or PathTooLongException)
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.LibraryUnavailable,
                null);
        }

        if (!string.Equals(Path.GetExtension(fullPath), ".dll", StringComparison.OrdinalIgnoreCase))
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.LibraryUnavailable,
                null);
        }

        return fullPath;
    }
}
