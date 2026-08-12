namespace ReadOS.Msp.Models;

/// <summary>
/// The model-facing envelope for one exec session read: terminal text plus
/// status. It is plain shell-shaped text (Codex-style), never a raw native
/// JSON result. <see cref="TerminalText"/> is already formatted by the host
/// facade; <see cref="Error"/> carries a sanitized failure only when the
/// native request could not be satisfied.
/// </summary>
public sealed record MspExecSessionRead
{
    public ulong SessionId { get; init; }

    public string TerminalText { get; init; } = string.Empty;

    public int? ExitCode { get; init; }

    public bool Running { get; init; }

    public bool Truncated { get; init; }

    public string? Error { get; init; }
}
