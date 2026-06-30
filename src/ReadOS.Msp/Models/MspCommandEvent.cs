using ReadOS.Msp.Policy;

namespace ReadOS.Msp.Models;

public enum MspCommandEventKind
{
    Started,
    Progress,
    PolicyDecision,
    Completed,
    Canceled
}

public sealed record MspCommandEvent
{
    public DateTimeOffset Timestamp { get; init; } = DateTimeOffset.UtcNow;

    public MspCommandEventKind Kind { get; init; }

    public string Actor { get; init; } = "agent";

    public string SessionId { get; init; } = "default";

    public string CommandText { get; init; } = string.Empty;

    public string CommandName { get; init; } = string.Empty;

    public string Message { get; init; } = string.Empty;

    public int? Percent { get; init; }

    public int? ExitCode { get; init; }

    public MspCommandResult? Result { get; init; }

    public MspPolicyDecision? Decision { get; init; }

    public MspCommandEffects Effects { get; init; } = MspCommandEffects.None;

    public MspCommandPreview Preview { get; init; } = MspCommandPreview.Empty;
}
