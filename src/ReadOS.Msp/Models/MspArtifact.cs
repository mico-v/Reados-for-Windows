namespace ReadOS.Msp.Models;

public sealed record MspArtifact
{
    public required string Path { get; init; }

    public string MediaType { get; init; } = "text/plain";

    public long? SizeBytes { get; init; }

    public string? Description { get; init; }

    public string? SourceCommand { get; init; }

    public string Actor { get; init; } = "agent";

    public string? SessionId { get; init; }

    public IReadOnlyList<string> SourcePaths { get; init; } = Array.Empty<string>();

    public IReadOnlyList<string> SourceDocuments { get; init; } = Array.Empty<string>();

    public IReadOnlyList<string> SourcePages { get; init; } = Array.Empty<string>();

    public DateTimeOffset CreatedAt { get; init; } = DateTimeOffset.UtcNow;

    public DateTimeOffset UpdatedAt { get; init; } = DateTimeOffset.UtcNow;

    public string? Preview { get; init; }
}
