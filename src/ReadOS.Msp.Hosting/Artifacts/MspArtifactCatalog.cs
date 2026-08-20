namespace ReadOS.Msp.Hosting.Artifacts;

public sealed class MspArtifactCatalog : IMspArtifactCatalog
{
    private readonly Func<IReadOnlyList<MspArtifactCatalogItem>> artifactsProvider;

    public MspArtifactCatalog(IEnumerable<MspArtifactCatalogItem> artifacts)
        : this(() => artifacts.ToArray())
    {
    }

    public MspArtifactCatalog(Func<IReadOnlyList<MspArtifactCatalogItem>> artifactsProvider)
    {
        this.artifactsProvider = artifactsProvider;
    }

    public IReadOnlyList<MspArtifactCatalogItem> ListArtifacts(string? query = null)
    {
        query = query?.Trim() ?? string.Empty;
        return artifactsProvider()
            .Where(item => string.IsNullOrWhiteSpace(query) ||
                item.Path.Contains(query, StringComparison.OrdinalIgnoreCase) ||
                item.Description.Contains(query, StringComparison.OrdinalIgnoreCase) ||
                item.MediaType.Contains(query, StringComparison.OrdinalIgnoreCase) ||
                item.SourceCommand.Contains(query, StringComparison.OrdinalIgnoreCase) ||
                item.Preview.Contains(query, StringComparison.OrdinalIgnoreCase))
            .OrderByDescending(item => item.UpdatedAt)
            .ThenBy(item => item.Path, StringComparer.OrdinalIgnoreCase)
            .ToArray();
    }

    public MspArtifactCatalogItem? FindArtifact(string path)
    {
        return artifactsProvider().FirstOrDefault(item =>
            string.Equals(item.Path, path, StringComparison.OrdinalIgnoreCase));
    }

    public string ReadArtifactContent(MspArtifactCatalogItem artifact)
    {
        return string.IsNullOrWhiteSpace(artifact.Content)
            ? artifact.Preview
            : artifact.Content;
    }

    public bool IsEvidenceArtifact(MspArtifactCatalogItem? artifact)
    {
        return artifact is not null &&
            (artifact.MediaType.Contains("json", StringComparison.OrdinalIgnoreCase) ||
                artifact.Path.EndsWith(".json", StringComparison.OrdinalIgnoreCase));
    }
}
