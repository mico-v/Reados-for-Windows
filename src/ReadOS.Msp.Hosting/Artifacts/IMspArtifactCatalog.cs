using ReadOS.Msp.Models;

namespace ReadOS.Msp.Hosting.Artifacts;

public interface IMspArtifactCatalog
{
    IReadOnlyList<MspArtifactCatalogItem> ListArtifacts(string? query = null);

    MspArtifactCatalogItem? FindArtifact(string path);

    string ReadArtifactContent(MspArtifactCatalogItem artifact);

    bool IsEvidenceArtifact(MspArtifactCatalogItem? artifact);
}
