using ReadOS.App.Models;

namespace ReadOS.App.Services.Msp;

internal readonly record struct ReadOsArtifactContentPreparation(
    bool Succeeded,
    string? Content,
    string StatusMessage);

internal readonly record struct ReadOsArtifactExportPreparation(
    bool Succeeded,
    string? Content,
    ArtifactExportMetadata? ExportMetadata,
    string StatusMessage);

internal readonly record struct ReadOsArtifactAttachmentPreparation(
    bool Succeeded,
    ChatAttachment? Attachment,
    string DefaultComposerPrompt,
    string StatusMessage);

internal sealed class ReadOsArtifactReuseService
{
    public const string ArtifactComposerPrompt = "请基于我附加的产物继续分析。";
    private const string MissingArtifactStatus = "请选择一个产物。";

    private readonly ReadOsArtifactService artifactService;
    private readonly ReadOsAttachmentQueueService attachmentQueueService;

    public ReadOsArtifactReuseService(
        ReadOsArtifactService artifactService,
        ReadOsAttachmentQueueService attachmentQueueService)
    {
        this.artifactService = artifactService;
        this.attachmentQueueService = attachmentQueueService;
    }

    public ReadOsArtifactContentPreparation PreparePreview(WorkspaceArtifact? artifact)
    {
        if (artifact is null)
        {
            return new ReadOsArtifactContentPreparation(false, null, MissingArtifactStatus);
        }

        return new ReadOsArtifactContentPreparation(
            true,
            artifactService.GetArtifactContent(artifact),
            $"已打开产物预览：{artifact.Path}");
    }

    public ReadOsArtifactContentPreparation PrepareCopy(WorkspaceArtifact? artifact)
    {
        if (artifact is null)
        {
            return new ReadOsArtifactContentPreparation(false, null, MissingArtifactStatus);
        }

        return new ReadOsArtifactContentPreparation(
            true,
            artifactService.GetArtifactContent(artifact),
            $"已复制产物内容：{artifact.Path}");
    }

    public ReadOsArtifactExportPreparation PrepareExport(WorkspaceArtifact? artifact)
    {
        if (artifact is null)
        {
            return new ReadOsArtifactExportPreparation(false, null, null, MissingArtifactStatus);
        }

        return new ReadOsArtifactExportPreparation(
            true,
            artifactService.GetArtifactContent(artifact),
            artifactService.GetExportMetadata(artifact),
            string.Empty);
    }

    public string BuildExportedStatus(string path)
    {
        return $"产物已导出：{path}";
    }

    public ReadOsArtifactAttachmentPreparation PrepareAttachment(
        WorkspaceArtifact? artifact,
        string? documentId)
    {
        if (artifact is null)
        {
            return new ReadOsArtifactAttachmentPreparation(false, null, string.Empty, MissingArtifactStatus);
        }

        return new ReadOsArtifactAttachmentPreparation(
            true,
            attachmentQueueService.CreateArtifactAttachment(artifact, documentId),
            ArtifactComposerPrompt,
            $"已附加产物：{artifact.Path}");
    }
}
