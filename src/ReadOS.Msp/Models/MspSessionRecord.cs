namespace ReadOS.Msp.Models;

public sealed record MspSessionRecord
{
    public string Id { get; init; } = "default";

    public string Title { get; init; } = "MSP session";

    public string Actor { get; init; } = "agent";

    public DateTimeOffset StartedAt { get; init; } = DateTimeOffset.UtcNow;

    public DateTimeOffset UpdatedAt { get; init; } = DateTimeOffset.UtcNow;

    public string LastCommandText { get; init; } = string.Empty;

    public string LastDecision { get; init; } = "Allow";

    public int LastExitCode { get; init; }

    public string LastProgressMessage { get; init; } = string.Empty;

    public int CommandCount { get; init; }

    public int ApprovalCount { get; init; }

    public IReadOnlyList<string> TranscriptIds { get; init; } = Array.Empty<string>();

    public IReadOnlyList<string> ArtifactPaths { get; init; } = Array.Empty<string>();
}
