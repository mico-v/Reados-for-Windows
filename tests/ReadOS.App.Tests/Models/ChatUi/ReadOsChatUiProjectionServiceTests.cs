using ReadOS.App.Models.ChatUi;
using ReadOS.App.Services.Msp;
using ReadOS.App.Models;

namespace ReadOS.App.Tests.Models.ChatUi;

// Verifies the ReadOS-side projection adapter produces a canonical timeline
// that satisfies the msp.chat-ui.timeline.v1 schema. Mirrors the conformance
// fixtures under MSP/Implementations/UI/MSPChatUI/Conformance/fixtures/.

public sealed class ReadOsChatUiProjectionServiceTests
{
    [Fact]
    public void Empty_inputs_produce_a_valid_empty_timeline()
    {
        var service = new ReadOsChatUiProjectionService();
        var timeline = service.BuildTimeline(
            Enumerable.Empty<ChatMessage>(),
            Enumerable.Empty<MspTranscriptEntry>(),
            Enumerable.Empty<WorkspaceArtifact>());

        Assert.Equal(ChatUiTimeline.Schema, timeline.SchemaVersion);
        Assert.Empty(timeline.Messages);
        Assert.NotNull(timeline.Presentation);
        Assert.Equal("markstream-readex-fade", timeline.Presentation!.MarkdownProfile);
    }

    [Fact]
    public void User_message_projects_to_a_user_message_with_markdown_block()
    {
        var service = new ReadOsChatUiProjectionService();
        var message = new ChatMessage
        {
            Id = "u1",
            Role = ChatRole.User,
            Content = "Hello",
            CreatedAt = DateTimeOffset.Parse("2026-07-28T10:00:00Z")
        };

        var timeline = service.BuildTimeline(new[] { message }, Enumerable.Empty<MspTranscriptEntry>(), Enumerable.Empty<WorkspaceArtifact>());

        var projected = Assert.Single(timeline.Messages);
        Assert.Equal(ChatUiRole.User, projected.Role);
        Assert.Equal("u1", projected.Id);
        var block = Assert.IsType<ChatUiMarkdownBlock>(Assert.Single(projected.Blocks));
        Assert.Equal("Hello", block.Text);
    }

    [Fact]
    public void Assistant_message_with_empty_text_marks_block_as_streaming()
    {
        var service = new ReadOsChatUiProjectionService();
        var message = new ChatMessage
        {
            Id = "a1",
            Role = ChatRole.Assistant,
            Content = "",
            Author = "MSP Default",
            CreatedAt = DateTimeOffset.Parse("2026-07-28T10:00:00Z")
        };

        var timeline = service.BuildTimeline(new[] { message }, Enumerable.Empty<MspTranscriptEntry>(), Enumerable.Empty<WorkspaceArtifact>());

        var projected = Assert.Single(timeline.Messages);
        Assert.Equal(ChatUiRole.Assistant, projected.Role);
        Assert.Equal("MSP Default", projected.ModelName);
        var block = Assert.IsType<ChatUiMarkdownBlock>(Assert.Single(projected.Blocks));
        Assert.True(block.Streaming ?? false);
    }

    [Fact]
    public void Msp_transcript_running_entry_projects_a_tool_message_with_progress_block()
    {
        var service = new ReadOsChatUiProjectionService();
        var entry = new MspTranscriptEntry
        {
            Id = "msp1",
            Actor = "agent",
            CommandText = "pdf search current alpha",
            IsRunning = true,
            ProgressPercent = 40,
            ProgressMessage = "Searching PDF",
            StartedAt = DateTimeOffset.Parse("2026-07-28T10:00:00Z")
        };

        var timeline = service.BuildTimeline(
            Enumerable.Empty<ChatMessage>(),
            new[] { entry },
            Enumerable.Empty<WorkspaceArtifact>());

        var projected = Assert.Single(timeline.Messages);
        Assert.Equal(ChatUiRole.Tool, projected.Role);
        Assert.Equal(ChatUiStatus.Running, projected.Status);
        Assert.Contains(projected.Blocks, b => b is ChatUiToolCallBlock);
        Assert.Contains(projected.Blocks, b => b is ChatUiProgressBlock);
    }

    [Fact]
    public void Msp_failed_entry_projects_an_error_notice_block()
    {
        var service = new ReadOsChatUiProjectionService();
        var entry = new MspTranscriptEntry
        {
            Id = "msp1",
            Actor = "agent",
            CommandText = "artifact write /tmp/x.md x",
            ExitCode = 1,
            WasCanceled = false,
            DiagnosticsSummary = "permission denied",
            RecoveryHint = "Check workspace policy",
            StartedAt = DateTimeOffset.Parse("2026-07-28T10:00:00Z"),
            CompletedAt = DateTimeOffset.Parse("2026-07-28T10:00:05Z")
        };

        var timeline = service.BuildTimeline(
            Enumerable.Empty<ChatMessage>(),
            new[] { entry },
            Enumerable.Empty<WorkspaceArtifact>());

        var projected = Assert.Single(timeline.Messages);
        Assert.Equal(ChatUiStatus.Failed, projected.Status);
        Assert.Contains(projected.Blocks, b => b is ChatUiNoticeBlock n && n.Status == ChatUiStatus.Failed);
    }

    [Fact]
    public void Workspace_artifact_projects_attachment_and_footer_blocks()
    {
        var service = new ReadOsChatUiProjectionService();
        var artifact = new WorkspaceArtifact
        {
            Id = "art1",
            Path = "/artifacts/summary.md",
            MediaType = "text/markdown",
            Actor = "agent",
            Preview = "# Summary\n\nBody",
            UpdatedAt = DateTimeOffset.Parse("2026-07-28T10:00:00Z")
        };
        artifact.SourcePaths.Add("/docs/p1.md");

        var timeline = service.BuildTimeline(
            Enumerable.Empty<ChatMessage>(),
            Enumerable.Empty<MspTranscriptEntry>(),
            new[] { artifact });

        var projected = Assert.Single(timeline.Messages);
        Assert.Equal(ChatUiRole.Tool, projected.Role);
        Assert.Contains(projected.Blocks, b => b is ChatUiAttachmentBlock);
        Assert.Contains(projected.Blocks, b => b is ChatUiMarkdownBlock);
        Assert.Contains(projected.Blocks, b => b is ChatUiSourcesBlock);
        Assert.Contains(projected.Blocks, b => b is ChatUiFooterBlock);
    }

    [Fact]
    public void Multiple_transcript_entries_in_a_session_project_a_tool_group()
    {
        var service = new ReadOsChatUiProjectionService();
        var e1 = new MspTranscriptEntry
        {
            Id = "msp1",
            SessionId = "sess1",
            CommandText = "pdf search current alpha",
            StartedAt = DateTimeOffset.Parse("2026-07-28T10:00:00Z")
        };
        var e2 = new MspTranscriptEntry
        {
            Id = "msp2",
            SessionId = "sess1",
            CommandText = "artifact write /tmp/x.md y",
            StartedAt = DateTimeOffset.Parse("2026-07-28T10:01:00Z")
        };

        var timeline = service.BuildTimeline(
            Enumerable.Empty<ChatMessage>(),
            new[] { e1, e2 },
            Enumerable.Empty<WorkspaceArtifact>());

        var projected = Assert.Single(timeline.Messages);
        var group = Assert.IsType<ChatUiToolGroupBlock>(Assert.Single(projected.Blocks, b => b is ChatUiToolGroupBlock));
        Assert.Equal(2, group.ToolCalls.Count);
    }

    [Fact]
    public void Running_entry_projects_a_processing_block()
    {
        var service = new ReadOsChatUiProjectionService();
        var entry = new MspTranscriptEntry
        {
            Id = "msp1",
            CommandText = "pdf search current alpha",
            IsRunning = true,
            ProgressPercent = 40,
            ProgressMessage = "Searching PDF",
            StartedAt = DateTimeOffset.Parse("2026-07-28T10:00:00Z")
        };

        var timeline = service.BuildTimeline(
            Enumerable.Empty<ChatMessage>(),
            new[] { entry },
            Enumerable.Empty<WorkspaceArtifact>());

        var projected = Assert.Single(timeline.Messages);
        Assert.Contains(projected.Blocks, b => b is ChatUiProcessingBlock);
    }

    [Fact]
    public void Assistant_message_with_thinking_section_projects_a_reasoning_block()
    {
        var service = new ReadOsChatUiProjectionService();
        var message = new ChatMessage
        {
            Id = "a1",
            Role = ChatRole.Assistant,
            Content = "<thinking>Let me reason about this.</thinking>\nThe answer is 42.",
            CreatedAt = DateTimeOffset.Parse("2026-07-28T10:00:00Z")
        };

        var timeline = service.BuildTimeline(new[] { message }, Enumerable.Empty<MspTranscriptEntry>(), Enumerable.Empty<WorkspaceArtifact>());

        var projected = Assert.Single(timeline.Messages);
        var reasoning = Assert.IsType<ChatUiReasoningBlock>(Assert.Single(projected.Blocks, b => b is ChatUiReasoningBlock));
        Assert.Equal("Let me reason about this.", reasoning.Text);
        var markdown = Assert.IsType<ChatUiMarkdownBlock>(Assert.Single(projected.Blocks, b => b is ChatUiMarkdownBlock));
        Assert.Equal("The answer is 42.", markdown.Text);
    }

    [Fact]
    public void Succeeded_search_command_projects_a_search_results_block()
    {
        var service = new ReadOsChatUiProjectionService();
        var entry = new MspTranscriptEntry
        {
            Id = "msp1",
            CommandText = "web search quantum computing",
            ExitCode = 0,
            StartedAt = DateTimeOffset.Parse("2026-07-28T10:00:00Z"),
            CompletedAt = DateTimeOffset.Parse("2026-07-28T10:00:05Z")
        };

        var timeline = service.BuildTimeline(
            Enumerable.Empty<ChatMessage>(),
            new[] { entry },
            Enumerable.Empty<WorkspaceArtifact>());

        var projected = Assert.Single(timeline.Messages);
        var search = Assert.IsType<ChatUiSearchResultsBlock>(Assert.Single(projected.Blocks, b => b is ChatUiSearchResultsBlock));
        Assert.NotNull(search.SearchQueries);
        Assert.Equal(new[] { "quantum computing" }, search.SearchQueries!);
    }

    [Fact]
    public void Running_search_command_projects_a_search_progress_block()
    {
        var service = new ReadOsChatUiProjectionService();
        var entry = new MspTranscriptEntry
        {
            Id = "msp1",
            CommandText = "pdf search current alpha",
            IsRunning = true,
            ProgressPercent = 30,
            ProgressMessage = "Searching PDF",
            StartedAt = DateTimeOffset.Parse("2026-07-28T10:00:00Z")
        };

        var timeline = service.BuildTimeline(
            Enumerable.Empty<ChatMessage>(),
            new[] { entry },
            Enumerable.Empty<WorkspaceArtifact>());

        var projected = Assert.Single(timeline.Messages);
        Assert.Contains(projected.Blocks, b => b is ChatUiSearchProgressBlock);
    }

    [Fact]
    public void Video_command_projects_a_video_progress_block()
    {
        var service = new ReadOsChatUiProjectionService();
        var entry = new MspTranscriptEntry
        {
            Id = "msp1",
            CommandText = "video render clip.mp4",
            IsRunning = true,
            ProgressPercent = 50,
            ProgressMessage = "Rendering",
            StartedAt = DateTimeOffset.Parse("2026-07-28T10:00:00Z")
        };

        var timeline = service.BuildTimeline(
            Enumerable.Empty<ChatMessage>(),
            new[] { entry },
            Enumerable.Empty<WorkspaceArtifact>());

        var projected = Assert.Single(timeline.Messages);
        var video = Assert.IsType<ChatUiVideoProgressBlock>(Assert.Single(projected.Blocks, b => b is ChatUiVideoProgressBlock));
        Assert.Equal(50, video.Progress);
    }

    [Fact]
    public void Plan_artifact_projects_a_proposed_plan_block()
    {
        var service = new ReadOsChatUiProjectionService();
        var artifact = new WorkspaceArtifact
        {
            Id = "art1",
            Path = "/artifacts/plan.md",
            MediaType = "application/x-msp-plan",
            Actor = "agent",
            Description = "proposed plan",
            Preview = "## Plan\n\n1. Read the chapter\n2. Summarize",
            UpdatedAt = DateTimeOffset.Parse("2026-07-28T10:00:00Z")
        };

        var timeline = service.BuildTimeline(
            Enumerable.Empty<ChatMessage>(),
            Enumerable.Empty<MspTranscriptEntry>(),
            new[] { artifact });

        var projected = Assert.Single(timeline.Messages);
        Assert.Contains(projected.Blocks, b => b is ChatUiProposedPlanBlock);
        Assert.Empty(projected.Blocks.OfType<ChatUiAttachmentBlock>());
    }

    [Fact]
    public void Image_artifact_projects_an_image_block()
    {
        var service = new ReadOsChatUiProjectionService();
        var artifact = new WorkspaceArtifact
        {
            Id = "art1",
            Path = "/artifacts/diagram.png",
            MediaType = "image/png",
            Actor = "agent",
            UpdatedAt = DateTimeOffset.Parse("2026-07-28T10:00:00Z")
        };

        var timeline = service.BuildTimeline(
            Enumerable.Empty<ChatMessage>(),
            Enumerable.Empty<MspTranscriptEntry>(),
            new[] { artifact });

        var projected = Assert.Single(timeline.Messages);
        Assert.Contains(projected.Blocks, b => b is ChatUiImageBlock);
    }

    [Fact]
    public void User_message_with_image_attachment_projects_an_image_block()
    {
        var service = new ReadOsChatUiProjectionService();
        var message = new ChatMessage
        {
            Id = "u1",
            Role = ChatRole.User,
            Content = "Look at this",
            CreatedAt = DateTimeOffset.Parse("2026-07-28T10:00:00Z")
        };
        message.Attachments.Add(new ChatAttachment
        {
            Id = "att1",
            Kind = AttachmentKind.File,
            FilePath = "c:/tmp/photo.png",
            Title = "photo"
        });

        var timeline = service.BuildTimeline(new[] { message }, Enumerable.Empty<MspTranscriptEntry>(), Enumerable.Empty<WorkspaceArtifact>());

        var projected = Assert.Single(timeline.Messages);
        Assert.Contains(projected.Blocks, b => b is ChatUiImageBlock);
    }

    [Fact]
    public void User_message_with_region_attachment_projects_a_text_selection_block()
    {
        var service = new ReadOsChatUiProjectionService();
        var message = new ChatMessage
        {
            Id = "u1",
            Role = ChatRole.User,
            Content = "Explain this part",
            CreatedAt = DateTimeOffset.Parse("2026-07-28T10:00:00Z")
        };
        message.Attachments.Add(new ChatAttachment
        {
            Id = "att1",
            Kind = AttachmentKind.Region,
            DocumentId = "doc1",
            StartPage = 3,
            RegionX = 10,
            RegionY = 20,
            RegionWidth = 100,
            RegionHeight = 50
        });

        var timeline = service.BuildTimeline(new[] { message }, Enumerable.Empty<MspTranscriptEntry>(), Enumerable.Empty<WorkspaceArtifact>());

        var projected = Assert.Single(timeline.Messages);
        var selection = Assert.IsType<ChatUiTextSelectionBlock>(Assert.Single(projected.Blocks, b => b is ChatUiTextSelectionBlock));
        Assert.NotNull(selection.TextSelection);
        Assert.Equal("doc1", selection.TextSelection!["documentId"]);
        Assert.Equal(3, selection.TextSelection!["page"]);
    }
}
