namespace ReadOS.Msp.Hosting.Artifacts;

public sealed record MspArtifactSourceReference
{
    public required string Path { get; init; }

    public required MspArtifactSourceKind Kind { get; init; }

    public string Detail { get; init; } = string.Empty;

    public bool CanOpenArtifact { get; init; }
}
