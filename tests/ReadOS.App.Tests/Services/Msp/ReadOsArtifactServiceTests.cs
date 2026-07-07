using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsArtifactServiceTests
{
    [Fact]
    public void GetVisibleArtifacts_filters_by_query_and_orders_by_update_time_then_path()
    {
        var workspace = new WorkspaceState();
        workspace.Artifacts.Add(CreateArtifact(
            "old",
            "/artifacts/beta.md",
            updatedAt: new DateTimeOffset(2026, 7, 7, 9, 0, 0, TimeSpan.Zero)));
        workspace.Artifacts.Add(CreateArtifact(
            "new",
            "/artifacts/alpha.md",
            description: "Evidence report",
            updatedAt: new DateTimeOffset(2026, 7, 7, 10, 0, 0, TimeSpan.Zero)));
        workspace.Artifacts.Add(CreateArtifact(
            "same-time",
            "/artifacts/aardvark.md",
            sourceCommand: "workflow run review-evidence",
            updatedAt: new DateTimeOffset(2026, 7, 7, 10, 0, 0, TimeSpan.Zero)));
        var service = new ReadOsArtifactService();

        var all = service.GetVisibleArtifacts(workspace, string.Empty);
        var evidence = service.GetVisibleArtifacts(workspace, "evidence");

        Assert.Equal(new[] { "/artifacts/aardvark.md", "/artifacts/alpha.md", "/artifacts/beta.md" }, all.Select(item => item.Path));
        Assert.Equal(new[] { "/artifacts/aardvark.md", "/artifacts/alpha.md" }, evidence.Select(item => item.Path));
    }

    [Fact]
    public void BuildLineage_projects_deduplicated_sources_with_openable_artifact_state()
    {
        var sourceArtifact = CreateArtifact("source", "/artifacts/workflows/evidence.json", description: "Structured evidence");
        var derivedArtifact = CreateArtifact("derived", "/artifacts/workflows/evidence-review.md");
        derivedArtifact.SourcePaths.Add("/artifacts/workflows/evidence.json");
        derivedArtifact.SourcePaths.Add("/artifacts/workflows/evidence.json");
        derivedArtifact.SourcePaths.Add("/artifacts/workflows/evidence.json.manifest.json");
        derivedArtifact.SourcePaths.Add("/documents/doc-1/pages/2.txt");
        derivedArtifact.SourcePaths.Add("/sessions/reados-workbench.json");
        derivedArtifact.SourcePaths.Add("/transcripts/transcript-1.json");
        derivedArtifact.SourcePaths.Add("/external/source.txt");
        derivedArtifact.SourceDocuments.Add("doc-1");
        derivedArtifact.SourceDocuments.Add("doc-1");
        derivedArtifact.SourcePages.Add("doc-1:2-3");
        var service = new ReadOsArtifactService();

        var lineage = service.BuildLineage(derivedArtifact, new[] { sourceArtifact, derivedArtifact });

        Assert.Equal(8, lineage.Count);
        Assert.Contains(lineage, item =>
            item.Path == "/artifacts/workflows/evidence.json" &&
            item.KindLabel == "Artifact" &&
            item.Detail == "Structured evidence" &&
            item.CanOpenArtifact);
        Assert.Contains(lineage, item =>
            item.Path == "/artifacts/workflows/evidence.json.manifest.json" &&
            item.KindLabel == "Manifest");
        Assert.Contains(lineage, item =>
            item.Path == "/documents/doc-1/pages/2.txt" &&
            item.KindLabel == "Page" &&
            item.Detail == "Document doc-1, page 2");
        Assert.Contains(lineage, item => item.Path == "/sessions/reados-workbench.json" && item.KindLabel == "Session");
        Assert.Contains(lineage, item => item.Path == "/transcripts/transcript-1.json" && item.KindLabel == "Transcript");
        Assert.Contains(lineage, item => item.Path == "/external/source.txt" && item.KindLabel == "Source");
        Assert.Contains(lineage, item => item.Path == "doc-1" && item.KindLabel == "Document");
        Assert.Contains(lineage, item =>
            item.Path == "doc-1:2-3" &&
            item.KindLabel == "Pages" &&
            item.Detail == "Document doc-1, pages 2-3");
    }

    [Fact]
    public void Content_preview_and_export_metadata_use_artifact_content_then_preview_and_media_type_fallbacks()
    {
        var markdown = CreateArtifact(
            "markdown",
            "/artifacts/workflows/review.md",
            content: "# Review",
            preview: "preview",
            mediaType: "text/markdown");
        var json = CreateArtifact(
            "json",
            "/artifacts/workflows/evidence",
            content: string.Empty,
            preview: "{ }",
            mediaType: "application/json");
        var text = CreateArtifact(
            "text",
            "/artifacts/workflows/plain",
            content: string.Empty,
            preview: string.Empty,
            mediaType: "text/plain");
        var service = new ReadOsArtifactService();

        Assert.Equal("# Review", service.GetArtifactContent(markdown));
        Assert.Equal("{ }", service.GetArtifactContent(json));
        Assert.Equal("(empty artifact)", service.GetArtifactPreviewText(text));

        var markdownExport = service.GetExportMetadata(markdown);
        var jsonExport = service.GetExportMetadata(json);
        var textExport = service.GetExportMetadata(text);

        Assert.Equal("review.md", markdownExport.SuggestedName);
        Assert.Equal(".md", markdownExport.Extension);
        Assert.Equal("evidence.json", jsonExport.SuggestedName);
        Assert.Equal(".json", jsonExport.Extension);
        Assert.Equal("plain.txt", textExport.SuggestedName);
        Assert.Equal(".txt", textExport.Extension);
    }

    [Fact]
    public void Workflow_artifact_paths_are_deterministic_and_slugged()
    {
        var service = new ReadOsArtifactService();
        var documentPath = service.BuildDocumentWorkflowArtifactPath(
            "Guide.PDF",
            "3.2 Service Layer",
            4,
            "evidence",
            ".json");
        var fallbackDocumentPath = service.BuildDocumentWorkflowArtifactPath(
            string.Empty,
            string.Empty,
            4,
            "evidence",
            ".json");
        var derivedPath = service.BuildDerivedArtifactPath(
            "/artifacts/workflows/evidence.manifest.json",
            "refined",
            ".md");
        var failurePath = service.BuildFailureReviewArtifactPath(new MspTranscriptEntry
        {
            Id = "transcript-1",
            SessionId = "reados-workbench"
        });

        Assert.Equal("/artifacts/workflows/guide-3-2-service-layer-evidence.json", documentPath);
        Assert.Equal("/artifacts/workflows/section-4-evidence.json", fallbackDocumentPath);
        Assert.Equal("/artifacts/workflows/evidence-refined.md", derivedPath);
        Assert.Equal("/artifacts/workflows/reados-workbench-transcript-1-failures.md", failurePath);
    }

    [Fact]
    public void FindArtifact_and_IsEvidenceArtifact_use_case_insensitive_virtual_paths()
    {
        var artifact = CreateArtifact("artifact", "/artifacts/workflows/Evidence.JSON", mediaType: "text/plain");
        var service = new ReadOsArtifactService();

        Assert.Same(artifact, service.FindArtifact(new[] { artifact }, "/ARTIFACTS/WORKFLOWS/evidence.json"));
        Assert.True(service.IsEvidenceArtifact(artifact));
        Assert.False(service.IsEvidenceArtifact(CreateArtifact("markdown", "/artifacts/notes.md", mediaType: "text/markdown")));
    }

    private static WorkspaceArtifact CreateArtifact(
        string id,
        string path,
        string content = "content",
        string description = "Artifact",
        string mediaType = "text/markdown",
        string sourceCommand = "artifact write",
        string preview = "preview",
        DateTimeOffset? updatedAt = null)
    {
        return new WorkspaceArtifact
        {
            Id = id,
            Path = path,
            Content = content,
            Description = description,
            MediaType = mediaType,
            SourceCommand = sourceCommand,
            Preview = preview,
            UpdatedAt = updatedAt ?? new DateTimeOffset(2026, 7, 7, 9, 0, 0, TimeSpan.Zero)
        };
    }
}
