namespace ReadOS.Msp.Hosting.Native;

/// <summary>
/// Selects how a new exec session is launched. Shell mode runs
/// <see cref="MspNativeExecSessionRequest.CommandText"/> through the host
/// shell; process mode launches <see cref="MspNativeExecSessionRequest.Program"/>
/// directly with explicit bounded <see cref="MspNativeExecSessionRequest.Arguments"/>.
/// </summary>
public enum MspExecSessionMode
{
    /// <summary>Runs command text through the host shell (native default).</summary>
    Shell,

    /// <summary>Launches an explicit program with bounded arguments.</summary>
    Process
}

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
    /// Command text. Required and non-empty for a new shell exec
    /// (<see cref="SessionId"/> == 0); must be null for a write_stdin and for
    /// a process-mode exec (which uses <see cref="Program"/> instead).
    /// </summary>
    public string? CommandText { get; init; }

    /// <summary>
    /// Launch mode for a new exec. Null and <see cref="MspExecSessionMode.Shell"/>
    /// both run command text through the shell; the wire omits the key for
    /// shell so the native side applies its serde default.
    /// </summary>
    public MspExecSessionMode? Mode { get; init; }

    /// <summary>
    /// Program path for process-mode exec. Required and non-empty when
    /// <see cref="Mode"/> is <see cref="MspExecSessionMode.Process"/>; must be
    /// null for shell mode.
    /// </summary>
    public string? Program { get; init; }

    /// <summary>
    /// Bounded argument list for process-mode exec. Ignored for shell mode.
    /// </summary>
    public IReadOnlyList<string>? Arguments { get; init; }

    /// <summary>
    /// Host-authorized local workspace root for process-mode exec. Required and
    /// fully-qualified when <see cref="Mode"/> is
    /// <see cref="MspExecSessionMode.Process"/>; never sent for shell mode.
    /// </summary>
    public string? WorkspaceRoot { get; init; }

    /// <summary>
    /// Optional environment entries for a process-mode exec. The map is
    /// host-authorized input and is never copied into a session result, audit
    /// record, or model-visible terminal envelope. Shell mode and
    /// write_stdin continuations omit this field from the wire request.
    /// </summary>
    public IReadOnlyDictionary<string, string>? Environment { get; init; }

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

    /// <summary>Maximum number of arguments on a process-mode exec.</summary>
    public const int MaximumProcessArguments = 1024;

    /// <summary>Maximum character length of one process-mode argument.</summary>
    public const int MaximumArgumentCharacters = 32 * 1024;

    /// <summary>Maximum number of process-mode environment entries.</summary>
    public const int MaximumProcessEnvironmentEntries = 64;

    /// <summary>
    /// Maximum UTF-8 byte length of one process-mode environment name and value
    /// combined, matching the native process boundary.
    /// </summary>
    public const int MaximumProcessEnvironmentEntryBytes = 8192;
}

internal sealed record MspNativeExecSessionRequestWire
{
    public string ContractVersion { get; init; } = MspNativeContract.Version;

    public required string Kind { get; init; }

    public string? Mode { get; init; }

    public string? CommandText { get; init; }

    public string? Program { get; init; }

    public string[]? Arguments { get; init; }

    public IReadOnlyDictionary<string, string>? Environment { get; init; }

    public string? WorkspaceRoot { get; init; }

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
