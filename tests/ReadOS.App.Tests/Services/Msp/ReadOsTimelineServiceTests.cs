using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsTimelineServiceTests
{
    [Fact]
    public void BuildTimeline_merges_records_and_sorts_by_created_time_then_kind()
    {
        var service = new ReadOsTimelineService();
        var timestamp = new DateTimeOffset(2026, 7, 7, 12, 0, 0, TimeSpan.Zero);
        var message = new ChatMessage
        {
            Id = "message",
            Role = ChatRole.User,
            Author = "user",
            Content = "question",
            CreatedAt = timestamp
        };
        var msp = new MspTranscriptEntry
        {
            Id = "msp",
            Actor = "agent",
            CommandText = "workspace info",
            StartedAt = timestamp,
            ExitCode = 0
        };
        var artifact = new WorkspaceArtifact
        {
            Id = "artifact",
            Path = "/artifacts/report.md",
            Content = "report",
            UpdatedAt = timestamp
        };

        var timeline = service.BuildTimeline(new[] { message }, new[] { msp }, new[] { artifact });

        Assert.Equal(new[]
        {
            TimelineItemKind.Message,
            TimelineItemKind.MspCommand,
            TimelineItemKind.Artifact
        }, timeline.Select(item => item.Kind));
        Assert.Same(message, timeline[0].Message);
        Assert.Same(msp, timeline[1].MspEntry);
        Assert.Same(artifact, timeline[2].Artifact);
    }

    [Fact]
    public void BuildTimeline_orders_older_items_before_newer_items()
    {
        var service = new ReadOsTimelineService();
        var oldTime = new DateTimeOffset(2026, 7, 7, 11, 0, 0, TimeSpan.Zero);
        var newTime = oldTime.AddMinutes(5);
        var message = new ChatMessage
        {
            Id = "new-message",
            Role = ChatRole.Assistant,
            Author = "ReadOS",
            Content = "answer",
            CreatedAt = newTime
        };
        var artifact = new WorkspaceArtifact
        {
            Id = "old-artifact",
            Path = "/artifacts/old.md",
            Content = "old",
            UpdatedAt = oldTime
        };

        var timeline = service.BuildTimeline(new[] { message }, Array.Empty<MspTranscriptEntry>(), new[] { artifact });

        Assert.Equal(new[] { "old-artifact", "new-message" }, timeline.Select(item => item.Artifact?.Id ?? item.Message?.Id));
    }

    [Fact]
    public void BuildTimeline_uses_thread_projection_for_evidence_approval_and_error_items()
    {
        var service = new ReadOsTimelineService();
        var timestamp = new DateTimeOffset(2026, 7, 7, 12, 30, 0, TimeSpan.Zero);
        var evidence = new ChatMessage
        {
            Role = ChatRole.System,
            Author = "Evidence",
            Content = "attached page",
            CreatedAt = timestamp
        };
        var pending = new MspTranscriptEntry
        {
            Id = "pending",
            Decision = "RequireConfirmation",
            CommandText = "artifact write /artifacts/pending.md pending",
            StartedAt = timestamp.AddMinutes(1),
            ExitCode = 126
        };
        var failed = new MspTranscriptEntry
        {
            Id = "failed",
            Decision = "Allow",
            CommandText = "pdf inspect missing",
            StartedAt = timestamp.AddMinutes(2),
            ExitCode = 1
        };

        var timeline = service.BuildTimeline(new[] { evidence }, new[] { pending, failed }, Array.Empty<WorkspaceArtifact>());

        Assert.Equal(new[] { TimelineItemKind.Evidence, TimelineItemKind.Approval, TimelineItemKind.Error }, timeline.Select(item => item.Kind));
    }

    [Fact]
    public void BuildTimeline_preserves_message_attachments_through_projection()
    {
        var service = new ReadOsTimelineService();
        var attachment = new ChatAttachment
        {
            Id = "attachment",
            Kind = AttachmentKind.File,
            Title = "Artifact",
            FilePath = "/artifacts/report.md"
        };
        var message = new ChatMessage
        {
            Role = ChatRole.User,
            Author = "user",
            Content = "use this",
            CreatedAt = DateTimeOffset.UtcNow
        };
        message.Attachments.Add(attachment);

        var timeline = service.BuildTimeline(new[] { message }, Array.Empty<MspTranscriptEntry>(), Array.Empty<WorkspaceArtifact>());

        var item = Assert.Single(timeline);
        Assert.Single(item.Attachments);
        Assert.Same(attachment, item.Attachments[0]);
    }
}
