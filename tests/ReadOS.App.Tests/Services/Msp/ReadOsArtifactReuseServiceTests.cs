using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsArtifactReuseServiceTests
{
    [Fact]
    public void PreparePreview_requires_selected_artifact()
    {
        var service = CreateService();

        var result = service.PreparePreview(null);

        Assert.False(result.Succeeded);
        Assert.Null(result.Content);
        Assert.Equal("请选择一个产物。", result.StatusMessage);
    }

    [Fact]
    public void PreparePreview_returns_artifact_content_and_status()
    {
        var service = CreateService();
        var artifact = new WorkspaceArtifact
        {
            Path = "/artifacts/report.md",
            Content = "# Report",
            Preview = "preview"
        };

        var result = service.PreparePreview(artifact);

        Assert.True(result.Succeeded);
        Assert.Equal("# Report", result.Content);
        Assert.Equal("已打开产物预览：/artifacts/report.md", result.StatusMessage);
    }

    [Fact]
    public void PrepareCopy_uses_preview_when_content_is_empty()
    {
        var service = CreateService();
        var artifact = new WorkspaceArtifact
        {
            Path = "/artifacts/report.md",
            Content = " ",
            Preview = "preview text"
        };

        var result = service.PrepareCopy(artifact);

        Assert.True(result.Succeeded);
        Assert.Equal("preview text", result.Content);
        Assert.Equal("已复制产物内容：/artifacts/report.md", result.StatusMessage);
    }

    [Fact]
    public void PrepareExport_requires_selected_artifact()
    {
        var service = CreateService();

        var result = service.PrepareExport(null);

        Assert.False(result.Succeeded);
        Assert.Null(result.Content);
        Assert.Null(result.ExportMetadata);
        Assert.Equal("请选择一个产物。", result.StatusMessage);
    }

    [Fact]
    public void PrepareExport_returns_content_and_export_metadata()
    {
        var service = CreateService();
        var artifact = new WorkspaceArtifact
        {
            Path = "/artifacts/report",
            MediaType = "text/markdown",
            Content = "# Report"
        };

        var result = service.PrepareExport(artifact);

        Assert.True(result.Succeeded);
        Assert.Equal("# Report", result.Content);
        Assert.NotNull(result.ExportMetadata);
        Assert.Equal("report.md", result.ExportMetadata?.SuggestedName);
        Assert.Equal(".md", result.ExportMetadata?.Extension);
        Assert.Equal(string.Empty, result.StatusMessage);
    }

    [Fact]
    public void BuildExportedStatus_formats_export_path()
    {
        var service = CreateService();

        var status = service.BuildExportedStatus("C:\\exports\\report.md");

        Assert.Equal("产物已导出：C:\\exports\\report.md", status);
    }

    [Fact]
    public void PrepareAttachment_requires_selected_artifact()
    {
        var service = CreateService();

        var result = service.PrepareAttachment(null, "doc-1");

        Assert.False(result.Succeeded);
        Assert.Null(result.Attachment);
        Assert.Equal(string.Empty, result.DefaultComposerPrompt);
        Assert.Equal("请选择一个产物。", result.StatusMessage);
    }

    [Fact]
    public void PrepareAttachment_builds_attachment_prompt_and_status()
    {
        var service = CreateService();
        var artifact = new WorkspaceArtifact
        {
            Path = "/artifacts/report.md"
        };

        var result = service.PrepareAttachment(artifact, "doc-1");

        Assert.True(result.Succeeded);
        Assert.NotNull(result.Attachment);
        Assert.Equal(AttachmentKind.File, result.Attachment?.Kind);
        Assert.Equal("doc-1", result.Attachment?.DocumentId);
        Assert.Equal("产物 · /artifacts/report.md", result.Attachment?.Title);
        Assert.Equal("/artifacts/report.md", result.Attachment?.FilePath);
        Assert.Equal(ReadOsArtifactReuseService.ArtifactComposerPrompt, result.DefaultComposerPrompt);
        Assert.Equal("已附加产物：/artifacts/report.md", result.StatusMessage);
    }

    private static ReadOsArtifactReuseService CreateService()
    {
        return new ReadOsArtifactReuseService(
            new ReadOsArtifactService(),
            new ReadOsAttachmentQueueService());
    }
}
