using ReadOS.Msp.Hosting.Native;

namespace ReadOS.Msp.Hosting.Runtime;

/// <summary>
/// Typed host request for a new model-facing exec session. Shell mode uses
/// <see cref="CommandText"/> (or the <see cref="Command"/> alias); process
/// mode uses <see cref="Program"/> and its bounded arguments/environment.
/// Host paths are authorization input only and never appear in a response.
/// </summary>
public sealed record MspExecSessionRequest
{
    /// <summary>Shell or explicit process launch mode.</summary>
    public MspExecSessionMode Mode { get; init; } = MspExecSessionMode.Shell;

    /// <summary>Model-visible shell command text.</summary>
    public string? CommandText { get; init; }

    /// <summary>
    /// Convenience alias for <see cref="CommandText"/>. If both are supplied,
    /// <see cref="CommandText"/> takes precedence.
    /// </summary>
    public string? Command { get; init; }

    /// <summary>Program path for process mode.</summary>
    public string? Program { get; init; }

    /// <summary>Bounded process-mode argument list.</summary>
    public IReadOnlyList<string>? Arguments { get; init; }

    /// <summary>Host-authorized fully-qualified process workspace root.</summary>
    public string? WorkspaceRoot { get; init; }

    /// <summary>
    /// Host-authorized process environment. It is forwarded only for a new
    /// process-mode session and is never copied into the response.
    /// </summary>
    public IReadOnlyDictionary<string, string>? Environment { get; init; }

    public string? WorkingDirectory { get; init; }

    public int? YieldTimeMs { get; init; }

    public int? MaxOutputTokens { get; init; }

    internal string? ResolvedCommandText => CommandText ?? Command;
}

/// <summary>
/// Typed response from an exec or stdin session operation. Terminal text is
/// already plain Codex-style terminal text for model-facing calls; native JSON,
/// host paths, and process environment values are not included.
/// </summary>
public sealed record MspExecSessionResponse
{
    public ulong SessionId { get; init; }

    public string TerminalText { get; init; } = string.Empty;

    public int? ExitCode { get; init; }

    public bool Running { get; init; }

    public bool Truncated { get; init; }

    public string? Error { get; init; }
}

/// <summary>Typed empty-poll request for a retained stdin session.</summary>
public sealed record MspExecSessionStdinPollRequest
{
    public required ulong SessionId { get; init; }

    public int? YieldTimeMs { get; init; }

    public int? MaxOutputTokens { get; init; }
}

/// <summary>
/// Typed stdin continuation request. <see cref="Chars"/> is bounded by
/// <see cref="MspNativeExecSessionLimits.MaximumWriteStdinChars"/> before it
/// reaches the native adapter.
/// </summary>
public sealed record MspExecSessionStdinContinuationRequest
{
    public required ulong SessionId { get; init; }

    public required string Chars { get; init; }

    public int? YieldTimeMs { get; init; }

    public int? MaxOutputTokens { get; init; }
}

/// <summary>
/// Common stdin request when callers choose poll-versus-write at runtime.
/// Null or empty <see cref="Chars"/> is an empty poll; non-empty text is a
/// continuation write.
/// </summary>
public sealed record MspExecSessionStdinRequest
{
    public required ulong SessionId { get; init; }

    public string? Chars { get; init; }

    public int? YieldTimeMs { get; init; }

    public int? MaxOutputTokens { get; init; }

    public bool IsPoll => string.IsNullOrEmpty(Chars);
}
