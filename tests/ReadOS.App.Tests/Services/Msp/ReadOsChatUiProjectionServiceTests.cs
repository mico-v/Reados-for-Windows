using ReadOS.App.Models;
using ReadOS.App.Models.ChatUi;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsChatUiProjectionServiceTests
{
    [Fact]
    public void Build_timeline_projects_user_message_as_markdown_block()
    {
        var service = new ReadOsChatUiProjectionService();
        var ts = new DateTimeOffset(2026, 7, 28, 13, 0, 0, TimeSpan.Zero);
        var message = new ChatMessage
        {
            Id = "u1", Role = ChatRole.User, Author = "operator", Content = "summarize chapter 3", CreatedAt = ts
        };

        var timeline = service.BuildTimeline(new[] { message }, Array.Empty<MspTranscriptEntry>(), Array.Empty<WorkspaceArtifact>());

        Assert.Equal(ChatUiTimeline.Schema, timeline.SchemaVersion);
        Assert.Single(timeline.Messages);
        var m = timeline.Messages[0];
        Assert.Equal(ChatUiRole.User, m.Role);
        Assert.Single(m.Blocks);
        var block = Assert.IsType<ChatUiMarkdownBlock>(m.Blocks[0]);
        Assert.Equal("summarize chapter 3", block.Text);
    }

    [Fact]
    public void Build_timeline_projects_msp_entry_as_tool_message_with_toolcall_block()
    {
        var service = new ReadOsChatUiProjectionService();
        var ts = new DateTimeOffset(2026, 7, 28, 13, 5, 0, TimeSpan.Zero);
        var entry = new MspTranscriptEntry
        {
            Id = "msp1", Actor = "agent", CommandText = "pdf text current 12 14",
            StartedAt = ts, CompletedAt = ts.AddSeconds(2), ExitCode = 0
        };

        var timeline = service.BuildTimeline(Array.Empty<ChatMessage>(), new[] { entry }, Array.Empty<WorkspaceArtifact>());

        var m = Assert.Single(timeline.Messages);
        Assert.Equal(ChatUiRole.Tool, m.Role);
        Assert.Equal(ChatUiStatus.Success, m.Status);
        var call = Assert.IsType<ChatUiToolCallBlock>(m.Blocks[0]);
        Assert.Equal("pdf", call.ToolName);
        Assert.Equal("pdf text current 12 14", call.Title);
    }

    [Fact]
    public void Build_timeline_projects_running_msp_entry_with_progress_block()
    {
        var service = new ReadOsChatUiProjectionService();
        var entry = new MspTranscriptEntry
        {
            Id = "msp1", CommandText = "pdf search current \"policy\"",
            StartedAt = DateTimeOffset.Now, IsRunning = true, ProgressMessage = "searching", ProgressPercent = 42
        };

        var timeline = service.BuildTimeline(Array.Empty<ChatMessage>(), new[] { entry }, Array.Empty<WorkspaceArtifact>());

        var m = Assert.Single(timeline.Messages);
        Assert.Equal(ChatUiStatus.Running, m.Status);
        Assert.Contains(m.Blocks, b => b is ChatUiProgressBlock);
        var progress = m.Blocks.OfType<ChatUiProgressBlock>().Single();
        Assert.Equal("searching", progress.Title);
        Assert.Equal(42, progress.Progress);
    }

    [Fact]
    public void Build_timeline_projects_approval_required_entry_with_notice_block()
    {
        var service = new ReadOsChatUiProjectionService();
        var entry = new MspTranscriptEntry
        {
            Id = "msp1", CommandText = "artifact delete /artifacts/old.md",
            StartedAt = DateTimeOffset.Now, Decision = "RequireConfirmation",
            PolicyPreview = "Deletes artifact /artifacts/old.md"
        };

        var timeline = service.BuildTimeline(Array.Empty<ChatMessage>(), new[] { entry }, Array.Empty<WorkspaceArtifact>());

        var m = Assert.Single(timeline.Messages);
        Assert.Equal(ChatUiStatus.Pending, m.Status);
        Assert.Contains(m.Blocks, b => b is ChatUiNoticeBlock);
        var notice = m.Blocks.OfType<ChatUiNoticeBlock>().Single();
        Assert.Contains("Deletes artifact", notice.Text);
    }

    [Fact]
    public void Build_timeline_projects_artifact_with_attachment_and_footer_blocks()
    {
        var service = new ReadOsChatUiProjectionService();
        var artifact = new WorkspaceArtifact
        {
            Id = "art1", Path = "/artifacts/summary.md", Content = "# Summary", MediaType = "text/markdown", Actor = "agent"
        };
        artifact.SourcePaths.Add("/documents/doc1/pages/12.txt");

        var timeline = service.BuildTimeline(Array.Empty<ChatMessage>(), Array.Empty<MspTranscriptEntry>(), new[] { artifact });

        var m = Assert.Single(timeline.Messages);
        Assert.Equal(ChatUiRole.Tool, m.Role);
        Assert.Contains(m.Blocks, b => b is ChatUiAttachmentBlock);
        Assert.Contains(m.Blocks, b => b is ChatUiSourcesBlock);
        Assert.Contains(m.Blocks, b => b is ChatUiFooterBlock);
    }

    [Fact]
    public void Build_timeline_sorts_entries_by_timestamp()
    {
        var service = new ReadOsChatUiProjectionService();
        var early = new DateTimeOffset(2026, 7, 28, 9, 0, 0, TimeSpan.Zero);
        var late = new DateTimeOffset(2026, 7, 28, 14, 0, 0, TimeSpan.Zero);
        var message = new ChatMessage { Id = "u1", Role = ChatRole.User, Content = "late", CreatedAt = late };
        var entry = new MspTranscriptEntry { Id = "m1", CommandText = "pwd", StartedAt = early, ExitCode = 0 };

        var timeline = service.BuildTimeline(new[] { message }, new[] { entry }, Array.Empty<WorkspaceArtifact>());

        Assert.Equal(new[] { "m1", "u1" }, timeline.Messages.Select(m => m.Id));
    }

    [Fact]
    public void Build_timeline_attaches_default_dark_presentation()
    {
        var service = new ReadOsChatUiProjectionService();

        var timeline = service.BuildTimeline(Array.Empty<ChatMessage>(), Array.Empty<MspTranscriptEntry>(), Array.Empty<WorkspaceArtifact>());

        Assert.NotNull(timeline.Presentation);
        Assert.Equal("dark", timeline.Presentation!.Theme);
        Assert.Equal("markstream-readex-fade", timeline.Presentation.MarkdownProfile);
        Assert.True(timeline.Presentation.MessageActions?.Enabled);
    }

    [Fact]
    public void Build_timeline_recovers_failure_hint_as_recovery_notice()
    {
        var service = new ReadOsChatUiProjectionService();
        var entry = new MspTranscriptEntry
        {
            Id = "m1", CommandText = "chat ask current \"explain\"",
            StartedAt = DateTimeOffset.Now, ExitCode = 1, DiagnosticsSummary = "model provider failed", RecoveryHint = "check API key"
        };

        var timeline = service.BuildTimeline(Array.Empty<ChatMessage>(), new[] { entry }, Array.Empty<WorkspaceArtifact>());

        var m = Assert.Single(timeline.Messages);
        Assert.Equal(ChatUiStatus.Failed, m.Status);
        Assert.Contains(m.Blocks, b => b is ChatUiNoticeBlock n && n.Text.Contains("Recovery"));
    }
}
