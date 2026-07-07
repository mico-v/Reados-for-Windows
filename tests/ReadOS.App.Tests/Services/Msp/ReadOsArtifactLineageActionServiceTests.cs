using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsArtifactLineageActionServiceTests
{
    [Fact]
    public void Resolve_ignores_null_lineage_item()
    {
        var service = CreateService();

        var action = service.Resolve(null, Array.Empty<WorkspaceArtifact>());

        Assert.False(action.ShouldApply);
        Assert.Null(action.Artifact);
        Assert.Null(action.StatusMessage);
    }

    [Fact]
    public void Resolve_reports_non_openable_source_when_artifact_collection_is_missing()
    {
        var service = CreateService();
        var item = new ArtifactLineageItem
        {
            Path = "/documents/doc/pages/1.txt"
        };

        var action = service.Resolve(item, null);

        Assert.True(action.ShouldApply);
        Assert.Null(action.Artifact);
        Assert.Equal("该来源不是可打开的产物：/documents/doc/pages/1.txt", action.StatusMessage);
    }

    [Fact]
    public void Resolve_reports_non_openable_source_when_artifact_is_missing()
    {
        var service = CreateService();
        var item = new ArtifactLineageItem
        {
            Path = "/artifacts/missing.md"
        };
        var artifact = new WorkspaceArtifact
        {
            Path = "/artifacts/report.md"
        };

        var action = service.Resolve(item, new[] { artifact });

        Assert.True(action.ShouldApply);
        Assert.Null(action.Artifact);
        Assert.Equal("该来源不是可打开的产物：/artifacts/missing.md", action.StatusMessage);
    }

    [Fact]
    public void Resolve_finds_artifact_case_insensitively_and_returns_open_status()
    {
        var service = CreateService();
        var item = new ArtifactLineageItem
        {
            Path = "/ARTIFACTS/REPORT.MD"
        };
        var artifact = new WorkspaceArtifact
        {
            Id = "artifact",
            Path = "/artifacts/report.md"
        };

        var action = service.Resolve(item, new[] { artifact });

        Assert.True(action.ShouldApply);
        Assert.Same(artifact, action.Artifact);
        Assert.Equal("已打开来源产物：/artifacts/report.md", action.StatusMessage);
    }

    private static ReadOsArtifactLineageActionService CreateService()
    {
        return new ReadOsArtifactLineageActionService(new ReadOsArtifactService());
    }
}
