using ReadOS.Msp.Policy;

namespace ReadOS.Msp.Models;

public sealed record MspAuditRecord
{
    public DateTimeOffset Timestamp { get; init; } = DateTimeOffset.UtcNow;

    public string Actor { get; init; } = "agent";

    public required string CommandName { get; init; }

    public required string CommandText { get; init; }

    public MspPolicyDecision Decision { get; init; } = MspPolicyDecision.Allow;

    public MspCommandEffects Effects { get; init; } = MspCommandEffects.None;

    public MspCommandPreview Preview { get; init; } = MspCommandPreview.Empty;

    public int ExitCode { get; init; }

    public string WorkingDirectory { get; init; } = "/";

    public string? Message { get; init; }
}
