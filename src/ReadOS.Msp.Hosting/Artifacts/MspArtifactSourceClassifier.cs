namespace ReadOS.Msp.Hosting.Artifacts;

public sealed class MspArtifactSourceClassifier
{
    public IReadOnlyList<MspArtifactSourceReference> BuildReferences(
        MspArtifactCatalogItem? artifact,
        IEnumerable<MspArtifactCatalogItem> workspaceArtifacts)
    {
        if (artifact is null)
        {
            return Array.Empty<MspArtifactSourceReference>();
        }

        var artifacts = workspaceArtifacts.ToArray();
        var references = new List<MspArtifactSourceReference>();
        var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

        foreach (var path in artifact.Artifact.SourcePaths
            .Where(path => !string.IsNullOrWhiteSpace(path))
            .Select(path => path.Trim()))
        {
            if (seen.Add($"path:{path}"))
            {
                references.Add(ClassifySourcePath(path, artifacts));
            }
        }

        foreach (var documentId in artifact.Artifact.SourceDocuments
            .Where(documentId => !string.IsNullOrWhiteSpace(documentId))
            .Select(documentId => documentId.Trim()))
        {
            if (seen.Add($"document:{documentId}"))
            {
                references.Add(new MspArtifactSourceReference
                {
                    Path = documentId,
                    Kind = MspArtifactSourceKind.Document,
                    Detail = "Source document"
                });
            }
        }

        foreach (var pageRange in artifact.Artifact.SourcePages
            .Where(pageRange => !string.IsNullOrWhiteSpace(pageRange))
            .Select(pageRange => pageRange.Trim()))
        {
            if (seen.Add($"pages:{pageRange}"))
            {
                references.Add(new MspArtifactSourceReference
                {
                    Path = pageRange,
                    Kind = MspArtifactSourceKind.Pages,
                    Detail = FormatSourcePageRangeDetail(pageRange)
                });
            }
        }

        return references;
    }

    public MspArtifactSourceReference ClassifySourcePath(
        string path,
        IEnumerable<MspArtifactCatalogItem> workspaceArtifacts)
    {
        var artifact = workspaceArtifacts.FirstOrDefault(item =>
            string.Equals(item.Path, path, StringComparison.OrdinalIgnoreCase));
        if (path.EndsWith(".manifest.json", StringComparison.OrdinalIgnoreCase))
        {
            return new MspArtifactSourceReference
            {
                Path = path,
                Kind = MspArtifactSourceKind.Manifest,
                Detail = "Sidecar provenance manifest"
            };
        }

        if (artifact is not null || path.StartsWith("/artifacts/", StringComparison.OrdinalIgnoreCase))
        {
            return new MspArtifactSourceReference
            {
                Path = path,
                Kind = MspArtifactSourceKind.Artifact,
                Detail = artifact is null ? "Source artifact path" : artifact.Description,
                CanOpenArtifact = artifact is not null
            };
        }

        if (TryParseDocumentPageSourcePath(path, out var documentId, out var pageNumber))
        {
            return new MspArtifactSourceReference
            {
                Path = path,
                Kind = MspArtifactSourceKind.Page,
                Detail = $"Document {documentId}, page {pageNumber}"
            };
        }

        if (path.StartsWith("/documents/", StringComparison.OrdinalIgnoreCase))
        {
            return new MspArtifactSourceReference
            {
                Path = path,
                Kind = MspArtifactSourceKind.Document,
                Detail = "Virtual document source"
            };
        }

        if (path.StartsWith("/sessions/", StringComparison.OrdinalIgnoreCase))
        {
            return new MspArtifactSourceReference
            {
                Path = path,
                Kind = MspArtifactSourceKind.Session,
                Detail = "MSP session projection"
            };
        }

        if (path.StartsWith("/transcripts/", StringComparison.OrdinalIgnoreCase))
        {
            return new MspArtifactSourceReference
            {
                Path = path,
                Kind = MspArtifactSourceKind.Transcript,
                Detail = "MSP transcript projection"
            };
        }

        return new MspArtifactSourceReference
        {
            Path = path,
            Kind = MspArtifactSourceKind.Source,
            Detail = "Source path"
        };
    }

    private static bool TryParseDocumentPageSourcePath(
        string path,
        out string documentId,
        out string pageNumber)
    {
        const string prefix = "/documents/";
        const string marker = "/pages/";
        const string suffix = ".txt";

        documentId = string.Empty;
        pageNumber = string.Empty;

        if (!path.StartsWith(prefix, StringComparison.OrdinalIgnoreCase) ||
            !path.EndsWith(suffix, StringComparison.OrdinalIgnoreCase))
        {
            return false;
        }

        var remainder = path[prefix.Length..^suffix.Length];
        var markerIndex = remainder.IndexOf(marker, StringComparison.OrdinalIgnoreCase);
        if (markerIndex <= 0)
        {
            return false;
        }

        documentId = remainder[..markerIndex];
        pageNumber = remainder[(markerIndex + marker.Length)..];
        return !string.IsNullOrWhiteSpace(documentId) && !string.IsNullOrWhiteSpace(pageNumber);
    }

    private static string FormatSourcePageRangeDetail(string pageRange)
    {
        var separatorIndex = pageRange.IndexOf(':', StringComparison.Ordinal);
        if (separatorIndex <= 0 || separatorIndex == pageRange.Length - 1)
        {
            return "Source pages";
        }

        return $"Document {pageRange[..separatorIndex]}, pages {pageRange[(separatorIndex + 1)..]}";
    }
}
