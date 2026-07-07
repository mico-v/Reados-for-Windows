using ReadOS.App.Models;

namespace ReadOS.App.Services.Msp;

internal readonly record struct ReadOsWorkflowPreparationResult(
    bool Succeeded,
    string? CommandText,
    string StatusMessage);

internal sealed class ReadOsWorkflowPreparationService
{
    private readonly ReadOsArtifactService artifactService;
    private readonly ReadOsWorkflowDraftService draftService;

    public ReadOsWorkflowPreparationService(
        ReadOsArtifactService artifactService,
        ReadOsWorkflowDraftService draftService)
    {
        this.artifactService = artifactService;
        this.draftService = draftService;
    }

    public ReadOsWorkflowPreparationResult PrepareDocumentWorkflow(
        LibraryItem? selectedDocument,
        OutlineItem? selectedOutlineItem,
        string workflowName,
        string artifactSuffix,
        string extension,
        string successStatusMessage)
    {
        if (selectedDocument?.Kind != LibraryItemKind.Pdf || selectedOutlineItem is null)
        {
            return Failure("请选择 PDF 目录项。");
        }

        var outlineSelector = string.IsNullOrWhiteSpace(selectedOutlineItem.Id)
            ? selectedOutlineItem.Title
            : selectedOutlineItem.Id;
        if (string.IsNullOrWhiteSpace(outlineSelector))
        {
            return Failure("当前目录项缺少可用选择器。");
        }

        var artifactPath = artifactService.BuildDocumentWorkflowArtifactPath(
            selectedDocument.Name,
            selectedOutlineItem.Title,
            selectedOutlineItem.Page,
            artifactSuffix,
            extension);
        return Success(
            draftService.BuildDocumentWorkflowCommand(workflowName, outlineSelector, artifactPath),
            successStatusMessage);
    }

    public ReadOsWorkflowPreparationResult PrepareReviewEvidenceWorkflow(WorkspaceArtifact? selectedArtifact)
    {
        if (selectedArtifact is null)
        {
            return Failure("请选择一个证据产物。");
        }

        if (!artifactService.IsEvidenceArtifact(selectedArtifact))
        {
            return Failure("请选择 JSON 证据产物来生成复核工作流。");
        }

        var artifactPath = artifactService.BuildDerivedArtifactPath(selectedArtifact.Path, "review", ".md");
        return Success(
            draftService.BuildReviewEvidenceCommand(selectedArtifact.Path, artifactPath),
            "已准备证据复核工作流。");
    }

    public ReadOsWorkflowPreparationResult PrepareSynthesizeEvidenceWorkflow(WorkspaceArtifact? selectedArtifact)
    {
        if (selectedArtifact is null)
        {
            return Failure("请选择一个证据产物。");
        }

        if (!artifactService.IsEvidenceArtifact(selectedArtifact))
        {
            return Failure("请选择 JSON 证据产物来生成综合工作流。");
        }

        var artifactPath = artifactService.BuildDerivedArtifactPath(selectedArtifact.Path, "synthesis", ".md");
        return Success(
            draftService.BuildSynthesizeEvidenceCommand(selectedArtifact.Path, artifactPath),
            "已准备证据综合工作流。");
    }

    public ReadOsWorkflowPreparationResult PrepareRefineArtifactWorkflow(
        WorkspaceArtifact? selectedArtifact,
        string instruction)
    {
        if (selectedArtifact is null)
        {
            return Failure("请选择一个产物。");
        }

        var artifactPath = artifactService.BuildDerivedArtifactPath(selectedArtifact.Path, "refined", ".md");
        return Success(
            draftService.BuildRefineArtifactCommand(
                selectedArtifact.Path,
                instruction,
                artifactPath),
            "已准备产物精炼工作流。");
    }

    public ReadOsWorkflowPreparationResult PrepareComposerRefineArtifactWorkflow(
        WorkspaceArtifact? selectedArtifact,
        string composerDraft)
    {
        if (selectedArtifact is null)
        {
            return Failure("请选择一个产物。");
        }

        var instruction = composerDraft.Trim();
        if (string.IsNullOrWhiteSpace(instruction))
        {
            return Failure("请先在 composer 输入精炼指令。");
        }

        var artifactPath = artifactService.BuildDerivedArtifactPath(selectedArtifact.Path, "steered", ".md");
        return Success(
            draftService.BuildRefineArtifactCommand(selectedArtifact.Path, instruction, artifactPath),
            "已准备 composer 精炼工作流。");
    }

    public ReadOsWorkflowPreparationResult PrepareReviewFailuresWorkflow(MspTranscriptEntry? selectedTranscript)
    {
        if (selectedTranscript is null ||
            selectedTranscript.IsRunning ||
            selectedTranscript.IsApprovalRequired ||
            selectedTranscript.Succeeded)
        {
            return Failure("请选择一个失败的 MSP 记录。");
        }

        var artifactPath = artifactService.BuildFailureReviewArtifactPath(selectedTranscript);
        return Success(
            draftService.BuildReviewFailuresCommand(artifactPath),
            "已准备失败复查工作流。");
    }

    private static ReadOsWorkflowPreparationResult Success(string commandText, string statusMessage)
    {
        return new ReadOsWorkflowPreparationResult(true, commandText, statusMessage);
    }

    private static ReadOsWorkflowPreparationResult Failure(string statusMessage)
    {
        return new ReadOsWorkflowPreparationResult(false, null, statusMessage);
    }
}
