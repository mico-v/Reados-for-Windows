using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsWorkflowPreparationServiceTests
{
    [Fact]
    public void PrepareDocumentWorkflow_requires_pdf_outline_context()
    {
        var service = CreateService();
        var markdown = new LibraryItem
        {
            Kind = LibraryItemKind.Markdown
        };

        var missingDocument = service.PrepareDocumentWorkflow(
            null,
            CreateOutline(),
            "explain-section",
            "explanation",
            ".md",
            "ready");
        var missingOutline = service.PrepareDocumentWorkflow(
            new LibraryItem { Kind = LibraryItemKind.Pdf },
            null,
            "explain-section",
            "explanation",
            ".md",
            "ready");
        var nonPdf = service.PrepareDocumentWorkflow(
            markdown,
            CreateOutline(),
            "explain-section",
            "explanation",
            ".md",
            "ready");

        AssertFailure(missingDocument, "请选择 PDF 目录项。");
        AssertFailure(missingOutline, "请选择 PDF 目录项。");
        AssertFailure(nonPdf, "请选择 PDF 目录项。");
    }

    [Fact]
    public void PrepareDocumentWorkflow_requires_outline_selector()
    {
        var service = CreateService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            Name = "Service Manual.pdf"
        };
        var outline = new OutlineItem
        {
            Id = string.Empty,
            Title = " ",
            Page = 32
        };

        var result = service.PrepareDocumentWorkflow(
            document,
            outline,
            "explain-section",
            "explanation",
            ".md",
            "ready");

        AssertFailure(result, "当前目录项缺少可用选择器。");
    }

    [Fact]
    public void PrepareDocumentWorkflow_builds_command_and_artifact_path()
    {
        var service = CreateService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            Name = "Service Manual.pdf"
        };
        var outline = CreateOutline();

        var result = service.PrepareDocumentWorkflow(
            document,
            outline,
            "explain-section",
            "explanation",
            ".md",
            "已准备章节讲解工作流。");

        Assert.True(result.Succeeded);
        Assert.Equal(
            "workflow run explain-section --document current --outline \"outline-32\" --artifact \"/artifacts/workflows/service-manual-3-2-service-layer-explanation.md\"",
            result.CommandText);
        Assert.Equal("已准备章节讲解工作流。", result.StatusMessage);
    }

    [Fact]
    public void PrepareReviewEvidenceWorkflow_requires_json_evidence_artifact()
    {
        var service = CreateService();

        var missing = service.PrepareReviewEvidenceWorkflow(null);
        var nonEvidence = service.PrepareReviewEvidenceWorkflow(new WorkspaceArtifact
        {
            Path = "/artifacts/workflows/notes.md",
            MediaType = "text/markdown"
        });

        AssertFailure(missing, "请选择一个证据产物。");
        AssertFailure(nonEvidence, "请选择 JSON 证据产物来生成复核工作流。");
    }

    [Fact]
    public void PrepareReviewEvidenceWorkflow_builds_review_command()
    {
        var service = CreateService();
        var evidence = CreateEvidenceArtifact();

        var result = service.PrepareReviewEvidenceWorkflow(evidence);

        Assert.True(result.Succeeded);
        Assert.Equal(
            "workflow run review-evidence --evidence \"/artifacts/workflows/evidence.json\" --artifact \"/artifacts/workflows/evidence-review.md\"",
            result.CommandText);
        Assert.Equal("已准备证据复核工作流。", result.StatusMessage);
    }

    [Fact]
    public void PrepareSynthesizeEvidenceWorkflow_builds_synthesis_command()
    {
        var service = CreateService();
        var evidence = CreateEvidenceArtifact();

        var result = service.PrepareSynthesizeEvidenceWorkflow(evidence);

        Assert.True(result.Succeeded);
        Assert.Equal(
            "workflow run synthesize-evidence --evidence \"/artifacts/workflows/evidence.json\" --artifact \"/artifacts/workflows/evidence-synthesis.md\"",
            result.CommandText);
        Assert.Equal("已准备证据综合工作流。", result.StatusMessage);
    }

    [Fact]
    public void PrepareRefineArtifactWorkflow_requires_artifact_and_builds_refine_command()
    {
        var service = CreateService();
        var artifact = new WorkspaceArtifact
        {
            Path = "/artifacts/workflows/evidence-synthesis.md"
        };

        var missing = service.PrepareRefineArtifactWorkflow(null, "tighten caveats");
        var result = service.PrepareRefineArtifactWorkflow(artifact, "tighten caveats");

        AssertFailure(missing, "请选择一个产物。");
        Assert.True(result.Succeeded);
        Assert.Equal(
            "workflow run refine-artifact --source \"/artifacts/workflows/evidence-synthesis.md\" --instruction \"tighten caveats\" --artifact \"/artifacts/workflows/evidence-synthesis-refined.md\"",
            result.CommandText);
        Assert.Equal("已准备产物精炼工作流。", result.StatusMessage);
    }

    [Fact]
    public void PrepareComposerRefineArtifactWorkflow_requires_artifact_and_instruction()
    {
        var service = CreateService();
        var artifact = new WorkspaceArtifact
        {
            Path = "/artifacts/workflows/evidence-synthesis.md"
        };

        var missingArtifact = service.PrepareComposerRefineArtifactWorkflow(null, "make shorter");
        var missingInstruction = service.PrepareComposerRefineArtifactWorkflow(artifact, "  ");

        AssertFailure(missingArtifact, "请选择一个产物。");
        AssertFailure(missingInstruction, "请先在 composer 输入精炼指令。");
    }

    [Fact]
    public void PrepareComposerRefineArtifactWorkflow_trims_instruction_and_builds_steered_command()
    {
        var service = CreateService();
        var artifact = new WorkspaceArtifact
        {
            Path = "/artifacts/workflows/evidence-synthesis.md"
        };

        var result = service.PrepareComposerRefineArtifactWorkflow(artifact, "  make shorter  ");

        Assert.True(result.Succeeded);
        Assert.Equal(
            "workflow run refine-artifact --source \"/artifacts/workflows/evidence-synthesis.md\" --instruction \"make shorter\" --artifact \"/artifacts/workflows/evidence-synthesis-steered.md\"",
            result.CommandText);
        Assert.Equal("已准备 composer 精炼工作流。", result.StatusMessage);
    }

    [Fact]
    public void PrepareReviewFailuresWorkflow_requires_failed_transcript()
    {
        var service = CreateService();
        var success = new MspTranscriptEntry
        {
            ExitCode = 0
        };
        var pending = new MspTranscriptEntry
        {
            Decision = "RequireConfirmation",
            ExitCode = 126
        };
        var running = new MspTranscriptEntry
        {
            IsRunning = true
        };

        AssertFailure(service.PrepareReviewFailuresWorkflow(null), "请选择一个失败的 MSP 记录。");
        AssertFailure(service.PrepareReviewFailuresWorkflow(success), "请选择一个失败的 MSP 记录。");
        AssertFailure(service.PrepareReviewFailuresWorkflow(pending), "请选择一个失败的 MSP 记录。");
        AssertFailure(service.PrepareReviewFailuresWorkflow(running), "请选择一个失败的 MSP 记录。");
    }

    [Fact]
    public void PrepareReviewFailuresWorkflow_builds_failure_review_command()
    {
        var service = CreateService();
        var failed = new MspTranscriptEntry
        {
            Id = "failed-command",
            SessionId = "review-session",
            ExitCode = 1
        };

        var result = service.PrepareReviewFailuresWorkflow(failed);

        Assert.True(result.Succeeded);
        Assert.Equal(
            "workflow run review-failures --artifact \"/artifacts/workflows/review-session-failed-command-failures.md\"",
            result.CommandText);
        Assert.Equal("已准备失败复查工作流。", result.StatusMessage);
    }

    private static ReadOsWorkflowPreparationService CreateService()
    {
        return new ReadOsWorkflowPreparationService(
            new ReadOsArtifactService(),
            new ReadOsWorkflowDraftService());
    }

    private static OutlineItem CreateOutline()
    {
        return new OutlineItem
        {
            Id = "outline-32",
            Title = "3.2 Service Layer",
            Page = 32
        };
    }

    private static WorkspaceArtifact CreateEvidenceArtifact()
    {
        return new WorkspaceArtifact
        {
            Path = "/artifacts/workflows/evidence.json",
            MediaType = "application/json"
        };
    }

    private static void AssertFailure(ReadOsWorkflowPreparationResult result, string statusMessage)
    {
        Assert.False(result.Succeeded);
        Assert.Null(result.CommandText);
        Assert.Equal(statusMessage, result.StatusMessage);
    }
}
