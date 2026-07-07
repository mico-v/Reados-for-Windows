using System.Collections.ObjectModel;
using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsMspApprovalReviewServiceTests
{
    [Fact]
    public void CanReview_requires_pending_approval_entry()
    {
        var service = new ReadOsMspApprovalReviewService();

        Assert.False(service.CanReview(null));
        Assert.False(service.CanReview(new MspTranscriptEntry
        {
            Decision = "Allow"
        }));
        Assert.True(service.CanReview(CreateApprovalEntry()));
    }

    [Fact]
    public void RemovePendingApproval_removes_matching_pending_entry_and_returns_original_index()
    {
        var first = new MspTranscriptEntry
        {
            Id = "first",
            Decision = "Allow"
        };
        var approval = CreateApprovalEntry();
        var last = new MspTranscriptEntry
        {
            Id = "last",
            Decision = "Allow"
        };
        var transcripts = new ObservableCollection<MspTranscriptEntry> { first, approval, last };
        var service = new ReadOsMspApprovalReviewService();

        var index = service.RemovePendingApproval(transcripts, approval);

        Assert.Equal(1, index);
        Assert.Equal(new[] { "first", "last" }, transcripts.Select(entry => entry.Id));
    }

    [Fact]
    public void RemovePendingApproval_ignores_non_reviewable_or_missing_entries()
    {
        var transcripts = new ObservableCollection<MspTranscriptEntry>
        {
            new()
            {
                Id = "existing",
                Decision = "Allow"
            }
        };
        var service = new ReadOsMspApprovalReviewService();

        var nullIndex = service.RemovePendingApproval(transcripts, null);
        var nonApprovalIndex = service.RemovePendingApproval(transcripts, transcripts[0]);
        var missingApprovalIndex = service.RemovePendingApproval(transcripts, CreateApprovalEntry());

        Assert.Equal(-1, nullIndex);
        Assert.Equal(-1, nonApprovalIndex);
        Assert.Equal(-1, missingApprovalIndex);
        Assert.Single(transcripts);
    }

    [Fact]
    public void CreateDeniedEntry_preserves_context_and_records_policy_denial()
    {
        var approval = CreateApprovalEntry();
        var completedAt = new DateTimeOffset(2026, 7, 7, 11, 0, 0, TimeSpan.Zero);
        var service = new ReadOsMspApprovalReviewService();

        var denied = service.CreateDeniedEntry(approval, completedAt);

        Assert.NotEqual(approval.Id, denied.Id);
        Assert.Equal(approval.Actor, denied.Actor);
        Assert.Equal(approval.SessionId, denied.SessionId);
        Assert.Equal(approval.CommandText, denied.CommandText);
        Assert.Equal(approval.StartedAt, denied.StartedAt);
        Assert.Equal(completedAt, denied.CompletedAt);
        Assert.Equal(126, denied.ExitCode);
        Assert.Equal("Operator denied MSP command.", denied.Stderr);
        Assert.Equal("Deny", denied.Decision);
        Assert.Equal(approval.Effects, denied.Effects);
        Assert.Equal(approval.ArtifactsSummary, denied.ArtifactsSummary);
        Assert.Equal(approval.PolicyPreview, denied.PolicyPreview);
        Assert.Contains("msp.policy.denied", denied.DiagnosticsSummary);
        Assert.Contains("Revise the command before retrying.", denied.DiagnosticsSummary);
        Assert.Equal("Revise the command before retrying.", denied.RecoveryHint);
    }

    private static MspTranscriptEntry CreateApprovalEntry()
    {
        return new MspTranscriptEntry
        {
            Id = "approval",
            Actor = "agent",
            SessionId = "reados-workbench",
            CommandText = "artifact write /artifacts/pending.md pending",
            StartedAt = new DateTimeOffset(2026, 7, 7, 10, 0, 0, TimeSpan.Zero),
            CompletedAt = new DateTimeOffset(2026, 7, 7, 10, 0, 1, TimeSpan.Zero),
            Decision = "RequireConfirmation",
            Effects = "WriteWorkspace, CreateArtifact",
            ArtifactsSummary = "/artifacts/pending.md",
            PolicyPreview = "Write artifact"
        };
    }
}
