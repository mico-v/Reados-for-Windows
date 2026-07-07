using System.Text;
using ReadOS.App.Models;
using ReadOS.Msp.Hosting.Artifacts;
using ReadOS.Msp.Models;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsArtifactService
{
    public IReadOnlyList<WorkspaceArtifact> GetVisibleArtifacts(WorkspaceState workspace, string? query)
    {
        var artifacts = workspace.Artifacts.ToArray();
        return CreateCatalog(artifacts)
            .ListArtifacts(query)
            .Select(item => FindMatchingWorkspaceArtifact(artifacts, item))
            .Where(item => item is not null)
            .Cast<WorkspaceArtifact>()
            .ToArray();
    }

    public WorkspaceArtifact? FindArtifact(IEnumerable<WorkspaceArtifact> artifacts, string path)
    {
        var workspaceArtifacts = artifacts.ToArray();
        var catalogItem = CreateCatalog(workspaceArtifacts).FindArtifact(path);
        return catalogItem is null
            ? null
            : FindMatchingWorkspaceArtifact(workspaceArtifacts, catalogItem);
    }

    public IReadOnlyList<ArtifactLineageItem> BuildLineage(
        WorkspaceArtifact? artifact,
        IEnumerable<WorkspaceArtifact> workspaceArtifacts)
    {
        if (artifact is null)
        {
            return Array.Empty<ArtifactLineageItem>();
        }

        var classifier = new MspArtifactSourceClassifier();
        return classifier
            .BuildReferences(
                ToCatalogItem(artifact),
                workspaceArtifacts.Select(ToCatalogItem))
            .Select(CreateLineageItem)
            .ToArray();
    }

    public string GetArtifactPreviewText(WorkspaceArtifact? artifact)
    {
        if (artifact is null)
        {
            return "选择一个产物查看内容。";
        }

        var preview = GetArtifactContent(artifact);
        if (string.IsNullOrWhiteSpace(preview))
        {
            return "(empty artifact)";
        }

        preview = preview.Trim();
        return preview.Length <= 8000 ? preview : preview[..8000] + "...";
    }

    public string GetArtifactContent(WorkspaceArtifact artifact)
    {
        return CreateCatalog(new[] { artifact })
            .ReadArtifactContent(ToCatalogItem(artifact));
    }

    public bool IsEvidenceArtifact(WorkspaceArtifact? artifact)
    {
        return artifact is not null &&
            CreateCatalog(new[] { artifact }).IsEvidenceArtifact(ToCatalogItem(artifact));
    }

    public ArtifactExportMetadata GetExportMetadata(WorkspaceArtifact artifact)
    {
        var suggestedName = GetVirtualFileName(artifact.Path);
        var extension = Path.GetExtension(suggestedName);
        if (!string.IsNullOrWhiteSpace(extension))
        {
            return new ArtifactExportMetadata(suggestedName, extension);
        }

        extension = artifact.MediaType.Contains("json", StringComparison.OrdinalIgnoreCase)
            ? ".json"
            : artifact.MediaType.Contains("markdown", StringComparison.OrdinalIgnoreCase)
                ? ".md"
                : ".txt";
        return new ArtifactExportMetadata(suggestedName + extension, extension);
    }

    public string BuildDocumentWorkflowArtifactPath(
        string? documentName,
        string? outlineTitle,
        int currentPageNumber,
        string artifactSuffix,
        string extension)
    {
        var documentSlug = string.IsNullOrWhiteSpace(documentName)
            ? string.Empty
            : GetVirtualFileNameWithoutExtension(documentName);
        var slug = BuildArtifactSlug(documentSlug, outlineTitle);
        if (string.IsNullOrWhiteSpace(slug))
        {
            slug = $"section-{Math.Max(1, currentPageNumber)}";
        }

        return $"/artifacts/workflows/{slug}-{artifactSuffix}{extension}";
    }

    public string BuildDerivedArtifactPath(string sourcePath, string artifactSuffix, string extension)
    {
        var sourceName = GetVirtualFileNameWithoutExtension(sourcePath);
        var slug = BuildArtifactSlug(sourceName);
        if (string.IsNullOrWhiteSpace(slug))
        {
            slug = "artifact";
        }

        return $"/artifacts/workflows/{slug}-{artifactSuffix}{extension}";
    }

    public string BuildFailureReviewArtifactPath(MspTranscriptEntry entry)
    {
        var slug = BuildArtifactSlug(entry.SessionId, entry.Id);
        if (string.IsNullOrWhiteSpace(slug))
        {
            slug = "current-session";
        }

        return $"/artifacts/workflows/{slug}-failures.md";
    }

    private static ArtifactLineageItem CreateLineageItem(MspArtifactSourceReference reference)
    {
        return new ArtifactLineageItem
        {
            Path = reference.Path,
            KindLabel = GetKindLabel(reference.Kind),
            Detail = reference.Detail,
            Glyph = GetGlyph(reference.Kind),
            CanOpenArtifact = reference.CanOpenArtifact
        };
    }

    private static string GetVirtualFileNameWithoutExtension(string path)
    {
        var name = GetVirtualFileName(path);
        if (name.EndsWith(".manifest.json", StringComparison.OrdinalIgnoreCase))
        {
            name = name[..^".manifest.json".Length];
        }

        var extensionStart = name.LastIndexOf('.');
        return extensionStart > 0 ? name[..extensionStart] : name;
    }

    private static string GetVirtualFileName(string path)
    {
        var nameStart = path.LastIndexOf('/');
        var name = nameStart >= 0 ? path[(nameStart + 1)..] : path;
        return string.IsNullOrWhiteSpace(name) ? "artifact.txt" : name;
    }

    private static string BuildArtifactSlug(params string?[] values)
    {
        var builder = new StringBuilder();
        foreach (var value in values)
        {
            if (string.IsNullOrWhiteSpace(value))
            {
                continue;
            }

            foreach (var character in value.Trim().ToLowerInvariant())
            {
                if ((character >= 'a' && character <= 'z') || (character >= '0' && character <= '9'))
                {
                    builder.Append(character);
                    continue;
                }

                if (builder.Length > 0 && builder[^1] != '-')
                {
                    builder.Append('-');
                }
            }

            if (builder.Length > 0 && builder[^1] != '-')
            {
                builder.Append('-');
            }
        }

        var slug = builder.ToString().Trim('-');
        return slug.Length <= 80 ? slug : slug[..80].TrimEnd('-');
    }

    private static string GetKindLabel(MspArtifactSourceKind kind)
    {
        return kind switch
        {
            MspArtifactSourceKind.Artifact => "Artifact",
            MspArtifactSourceKind.Manifest => "Manifest",
            MspArtifactSourceKind.Page => "Page",
            MspArtifactSourceKind.Document => "Document",
            MspArtifactSourceKind.Pages => "Pages",
            MspArtifactSourceKind.Session => "Session",
            MspArtifactSourceKind.Transcript => "Transcript",
            _ => "Source"
        };
    }

    private static string GetGlyph(MspArtifactSourceKind kind)
    {
        return kind switch
        {
            MspArtifactSourceKind.Page or MspArtifactSourceKind.Pages => "\uE7C3",
            MspArtifactSourceKind.Session or MspArtifactSourceKind.Transcript => "\uE8D4",
            MspArtifactSourceKind.Source => "\uE71B",
            _ => "\uE8A5"
        };
    }

    private static IMspArtifactCatalog CreateCatalog(IEnumerable<WorkspaceArtifact> artifacts)
    {
        return new MspArtifactCatalog(artifacts.Select(ToCatalogItem));
    }

    private static MspArtifactCatalogItem ToCatalogItem(WorkspaceArtifact artifact)
    {
        return new MspArtifactCatalogItem
        {
            Id = artifact.Id,
            Artifact = new MspArtifact
            {
                Path = artifact.Path,
                MediaType = artifact.MediaType,
                SizeBytes = artifact.SizeBytes,
                Description = artifact.Description,
                SourceCommand = artifact.SourceCommand,
                Actor = artifact.Actor,
                SessionId = artifact.SessionId,
                SourcePaths = artifact.SourcePaths.ToArray(),
                SourceDocuments = artifact.SourceDocuments.ToArray(),
                SourcePages = artifact.SourcePages.ToArray(),
                CreatedAt = artifact.CreatedAt,
                UpdatedAt = artifact.UpdatedAt,
                Preview = artifact.Preview
            },
            Content = artifact.Content
        };
    }

    private static WorkspaceArtifact? FindMatchingWorkspaceArtifact(
        IReadOnlyList<WorkspaceArtifact> artifacts,
        MspArtifactCatalogItem catalogItem)
    {
        if (!string.IsNullOrWhiteSpace(catalogItem.Id))
        {
            var byId = artifacts.FirstOrDefault(artifact =>
                string.Equals(artifact.Id, catalogItem.Id, StringComparison.Ordinal));
            if (byId is not null)
            {
                return byId;
            }
        }

        return artifacts.FirstOrDefault(artifact =>
            string.Equals(artifact.Path, catalogItem.Path, StringComparison.OrdinalIgnoreCase));
    }
}

internal sealed record ArtifactExportMetadata(string SuggestedName, string Extension);
