namespace ReadOS.Msp.Models;

public sealed record MspArtifact
{
    public required string Path { get; init; }

    public string MediaType { get; init; } = "text/plain";

    public string? Description { get; init; }
}
