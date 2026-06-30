namespace ReadOS.Msp.Models;

public sealed record MspCommandTranscriptRecord
{
    public string Id { get; init; } = Guid.NewGuid().ToString("N");

    public string Actor { get; init; } = "agent";

    public required string CommandText { get; init; }

    public DateTimeOffset StartedAt { get; init; } = DateTimeOffset.UtcNow;

    public DateTimeOffset CompletedAt { get; init; } = DateTimeOffset.UtcNow;

    public int ExitCode { get; init; }

    public string Stdout { get; init; } = string.Empty;

    public string Stderr { get; init; } = string.Empty;

    public string Decision { get; init; } = "Allow";

    public string Effects { get; init; } = "None";

    public string ArtifactsSummary { get; init; } = string.Empty;

    public string PolicyPreview { get; init; } = string.Empty;
}
