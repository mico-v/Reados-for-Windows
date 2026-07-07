namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsWorkflowDraftService
{
    public string BuildDocumentWorkflowCommand(
        string workflowName,
        string outlineSelector,
        string artifactPath)
    {
        return $"workflow run {workflowName} --document current --outline {QuoteArgument(outlineSelector)} --artifact {QuoteArgument(artifactPath)}";
    }

    public string BuildReviewEvidenceCommand(string evidencePath, string artifactPath)
    {
        return $"workflow run review-evidence --evidence {QuoteArgument(evidencePath)} --artifact {QuoteArgument(artifactPath)}";
    }

    public string BuildSynthesizeEvidenceCommand(string evidencePath, string artifactPath)
    {
        return $"workflow run synthesize-evidence --evidence {QuoteArgument(evidencePath)} --artifact {QuoteArgument(artifactPath)}";
    }

    public string BuildRefineArtifactCommand(
        string sourcePath,
        string instruction,
        string artifactPath)
    {
        return $"workflow run refine-artifact --source {QuoteArgument(sourcePath)} --instruction {QuoteArgument(instruction)} --artifact {QuoteArgument(artifactPath)}";
    }

    public string BuildReviewFailuresCommand(string artifactPath)
    {
        return $"workflow run review-failures --artifact {QuoteArgument(artifactPath)}";
    }

    public string QuoteArgument(string value)
    {
        return "\"" +
            value.Replace("\\", "\\\\", StringComparison.Ordinal)
                .Replace("\"", "\\\"", StringComparison.Ordinal) +
            "\"";
    }
}
