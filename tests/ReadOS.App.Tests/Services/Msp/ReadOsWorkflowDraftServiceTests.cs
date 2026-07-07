using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsWorkflowDraftServiceTests
{
    [Fact]
    public void BuildDocumentWorkflowCommand_quotes_outline_and_artifact_path()
    {
        var service = new ReadOsWorkflowDraftService();

        var command = service.BuildDocumentWorkflowCommand(
            "explain-section",
            "3.2 Service Layer",
            "/artifacts/workflows/service layer.md");

        Assert.Equal(
            "workflow run explain-section --document current --outline \"3.2 Service Layer\" --artifact \"/artifacts/workflows/service layer.md\"",
            command);
    }

    [Fact]
    public void BuildReviewEvidenceCommand_quotes_paths()
    {
        var service = new ReadOsWorkflowDraftService();

        var command = service.BuildReviewEvidenceCommand(
            "/artifacts/workflows/evidence.json",
            "/artifacts/workflows/evidence-review.md");

        Assert.Equal(
            "workflow run review-evidence --evidence \"/artifacts/workflows/evidence.json\" --artifact \"/artifacts/workflows/evidence-review.md\"",
            command);
    }

    [Fact]
    public void BuildSynthesizeEvidenceCommand_quotes_paths()
    {
        var service = new ReadOsWorkflowDraftService();

        var command = service.BuildSynthesizeEvidenceCommand(
            "/artifacts/workflows/evidence.json",
            "/artifacts/workflows/evidence-synthesis.md");

        Assert.Equal(
            "workflow run synthesize-evidence --evidence \"/artifacts/workflows/evidence.json\" --artifact \"/artifacts/workflows/evidence-synthesis.md\"",
            command);
    }

    [Fact]
    public void BuildRefineArtifactCommand_quotes_source_instruction_and_artifact()
    {
        var service = new ReadOsWorkflowDraftService();

        var command = service.BuildRefineArtifactCommand(
            "/artifacts/workflows/synthesis.md",
            "tighten caveats and keep citations",
            "/artifacts/workflows/synthesis-refined.md");

        Assert.Equal(
            "workflow run refine-artifact --source \"/artifacts/workflows/synthesis.md\" --instruction \"tighten caveats and keep citations\" --artifact \"/artifacts/workflows/synthesis-refined.md\"",
            command);
    }

    [Fact]
    public void BuildReviewFailuresCommand_quotes_artifact_path()
    {
        var service = new ReadOsWorkflowDraftService();

        var command = service.BuildReviewFailuresCommand("/artifacts/workflows/review-session-failures.md");

        Assert.Equal(
            "workflow run review-failures --artifact \"/artifacts/workflows/review-session-failures.md\"",
            command);
    }

    [Fact]
    public void QuoteArgument_escapes_quotes_and_backslashes()
    {
        var service = new ReadOsWorkflowDraftService();

        var quoted = service.QuoteArgument("C:\\docs\\\"service\" layer");

        Assert.Equal("\"C:\\\\docs\\\\\\\"service\\\" layer\"", quoted);
    }
}
