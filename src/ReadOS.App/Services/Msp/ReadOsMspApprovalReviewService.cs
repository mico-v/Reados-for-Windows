using System.Collections.ObjectModel;
using ReadOS.App.Models;
using ReadOS.Msp.Models;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsMspApprovalReviewService
{
    public bool CanReview(MspTranscriptEntry? entry)
    {
        return entry?.IsApprovalRequired == true;
    }

    public int RemovePendingApproval(
        ObservableCollection<MspTranscriptEntry> transcripts,
        MspTranscriptEntry? entry)
    {
        if (!CanReview(entry))
        {
            return -1;
        }

        var index = transcripts.IndexOf(entry!);
        if (index >= 0)
        {
            transcripts.RemoveAt(index);
        }

        return index;
    }

    public MspTranscriptEntry CreateDeniedEntry(MspTranscriptEntry approvalEntry, DateTimeOffset completedAt)
    {
        return MspTranscriptEntry.FromRecord(new MspCommandTranscriptRecord
        {
            Actor = approvalEntry.Actor,
            SessionId = approvalEntry.SessionId,
            CommandText = approvalEntry.CommandText,
            StartedAt = approvalEntry.StartedAt,
            CompletedAt = completedAt,
            ExitCode = 126,
            Stderr = "Operator denied MSP command.",
            Decision = "Deny",
            Effects = approvalEntry.Effects,
            ArtifactsSummary = approvalEntry.ArtifactsSummary,
            PolicyPreview = approvalEntry.PolicyPreview,
            DiagnosticsSummary = "error msp.policy.denied: Operator denied MSP command." +
                Environment.NewLine +
                "recovery: Revise the command before retrying.",
            RecoveryHint = "Revise the command before retrying."
        });
    }
}
