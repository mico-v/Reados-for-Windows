using ReadOS.Msp.Models;

namespace ReadOS.Msp.Hosting.Artifacts;

public sealed record MspArtifactCatalogItem
{
    public string Id { get; init; } = string.Empty;

    public required MspArtifact Artifact { get; init; }

    public string Content { get; init; } = string.Empty;

    public string Path => Artifact.Path;

    public string MediaType => Artifact.MediaType;

    public string Description => Artifact.Description ?? string.Empty;

    public string SourceCommand => Artifact.SourceCommand ?? string.Empty;

    public DateTimeOffset UpdatedAt => Artifact.UpdatedAt;

    public string Preview => Artifact.Preview ?? string.Empty;
}
