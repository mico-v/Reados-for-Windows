namespace ReadOS.Msp.Hosting.Native;

/// <summary>
/// One model-facing exec session request routed through ABI v2 operation 5
/// (<c>MSP_ABI_V2_OPERATION_SESSION_EXEC</c>). A zero <see cref="SessionId"/>
/// selects a new synchronous exec; a nonzero <see cref="SessionId"/> selects a
/// write_stdin continuation for an existing session. For a write_stdin, an
/// empty or omitted <see cref="Chars"/> is an empty poll that returns the
/// retained terminal text/status once and then closes the session.
/// </summary>
public sealed record MspNativeExecSessionRequest
{
    /// <summary>
    /// Command text. Required and non-empty for a new exec
    /// (<see cref="SessionId"/> == 0); must be null for a write_stdin.
    /// </summary>
    public string? CommandText { get; init; }

    /// <summary>0 for a new exec; the retained session id for write_stdin.</summary>
    public ulong SessionId { get; init; }

    public string? WorkingDirectory { get; init; }

    public string? Actor { get; init; }

    public bool DryRun { get; init; }

    public int? YieldTimeMs { get; init; }

    public int? MaxOutputTokens { get; init; }

    /// <summary>Stdin chars for a write_stdin; omitted or empty polls.</summary>
    public string? Chars { get; init; }
}

/// <summary>
/// Result of one exec session request. <see cref="Ok"/> discriminates a
/// completed session read from a native-level failure carried by
/// <see cref="Error"/>. The model-facing terminal text is produced by the
/// host facade and is never the raw native JSON envelope.
/// </summary>
public sealed record MspNativeExecSessionResult
{
    public required string ContractVersion { get; init; }

    public required bool Ok { get; init; }

    public ulong SessionId { get; init; }

    public bool Running { get; init; }

    public string? TerminalText { get; init; }

    public int? ExitCode { get; init; }

    public double WallTimeSeconds { get; init; }

    public bool Truncated { get; init; }

    public MspNativeExecSessionError? Error { get; init; }
}

public sealed record MspNativeExecSessionError
{
    public required string Code { get; init; }

    public required string Message { get; init; }
}

/// <summary>
/// Bounds applied to a write_stdin payload before serialization. The value is
/// intentionally modest: stdin continuation text is a bounded model input and
/// must not be used to bypass the adapter's overall request size limit.
/// </summary>
public static class MspNativeExecSessionLimits
{
    public const int MaximumWriteStdinChars = 1024 * 1024;
}

internal sealed record MspNativeExecSessionRequestWire
{
    public string ContractVersion { get; init; } = MspNativeContract.Version;

    public required string Kind { get; init; }

    public string? CommandText { get; init; }

    public ulong SessionId { get; init; }

    public string? WorkingDirectory { get; init; }

    public string? Actor { get; init; }

    public bool DryRun { get; init; }

    public int? YieldTimeMs { get; init; }

    public int? MaxOutputTokens { get; init; }

    public string? Chars { get; init; }
}

internal sealed record MspNativeExecSessionResponseWire
{
    public string? ContractVersion { get; init; }

    public bool Ok { get; init; }

    public ulong SessionId { get; init; }

    public bool Running { get; init; }

    public string? TerminalText { get; init; }

    public int? ExitCode { get; init; }

    public double WallTimeSeconds { get; init; }

    public bool Truncated { get; init; }

    public MspNativeExecSessionError? Error { get; init; }

    public bool Canceled { get; init; }
}
