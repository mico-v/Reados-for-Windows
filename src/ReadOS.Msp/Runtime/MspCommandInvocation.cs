namespace ReadOS.Msp.Runtime;

public sealed record MspCommandInvocation
{
    public static MspCommandInvocation Empty { get; } = new();

    public string Actor { get; init; } = "agent";

    public string CommandText { get; init; } = string.Empty;

    public string CommandName { get; init; } = string.Empty;

    public string SessionId { get; init; } = "default";

    public bool DryRun { get; init; }

    public DateTimeOffset StartedAt { get; init; } = DateTimeOffset.UtcNow;
}
