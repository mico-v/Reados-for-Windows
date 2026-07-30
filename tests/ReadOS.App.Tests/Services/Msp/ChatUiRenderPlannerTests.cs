using ReadOS.App.Models.ChatUi;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ChatUiRenderPlannerTests
{
    [Fact]
    public void Plan_with_no_previous_returns_full_render()
    {
        var next = Timeline(AssistantMessage("a1", "hello"));

        var op = ChatUiRenderPlanner.Plan(null, next);

        Assert.Equal(ChatUiRenderOperationKind.FullRender, op.Kind);
    }

    [Fact]
    public void Plan_with_identical_payload_and_presentation_returns_scroll_sync()
    {
        var timeline = Timeline(AssistantMessage("a1", "hello"));

        var op = ChatUiRenderPlanner.Plan(timeline, timeline);

        Assert.Equal(ChatUiRenderOperationKind.ScrollSync, op.Kind);
    }

    [Fact]
    public void Plan_with_only_presentation_change_returns_presentation_only_update()
    {
        var before = Timeline(AssistantMessage("a1", "hello"));
        var after = new ChatUiTimeline
        {
            SchemaVersion = ChatUiTimeline.Schema,
            Revision = 2,
            Presentation = new ChatUiPresentation { Theme = "light", MarkdownProfile = "markstream-readex-fade" },
            Messages = before.Messages
        };

        var op = ChatUiRenderPlanner.Plan(before, after);

        Assert.Equal(ChatUiRenderOperationKind.PresentationOnlyUpdate, op.Kind);
    }

    [Fact]
    public void Plan_with_streaming_assistant_append_returns_direct_streaming_update()
    {
        var before = Timeline(new ChatUiMessage
        {
            Id = "a1", Role = ChatUiRole.Assistant, IsStreaming = true,
            Blocks = new ChatUiBlock[] { new ChatUiMarkdownBlock { Id = "a1:text", Text = "Hel", Streaming = true } }
        });

        var after = new ChatUiTimeline
        {
            SchemaVersion = ChatUiTimeline.Schema,
            Revision = 2,
            Presentation = new ChatUiPresentation { Theme = "dark" },
            Messages = new[]
            {
                new ChatUiMessage
                {
                    Id = "a1", Role = ChatUiRole.Assistant, IsStreaming = true,
                    Blocks = new ChatUiBlock[] { new ChatUiMarkdownBlock { Id = "a1:text", Text = "Hello", Streaming = true } }
                }
            }
        };

        var op = ChatUiRenderPlanner.Plan(before, after);

        Assert.Equal(ChatUiRenderOperationKind.DirectStreamingUpdate, op.Kind);
    }

    [Fact]
    public void Plan_with_added_message_returns_payload_patch()
    {
        var before = Timeline(UserMessage("u1", "question"));
        var after = Timeline(UserMessage("u1", "question"), AssistantMessage("a1", "answer"));

        var op = ChatUiRenderPlanner.Plan(before, after);

        Assert.Equal(ChatUiRenderOperationKind.PayloadPatch, op.Kind);
    }

    [Fact]
    public void Plan_with_removed_block_returns_payload_patch()
    {
        var before = Timeline(new ChatUiMessage
        {
            Id = "m1", Role = ChatUiRole.Tool,
            Blocks = new ChatUiBlock[]
            {
                new ChatUiToolCallBlock { Id = "b1", ToolName = "echo", Status = ChatUiStatus.Success },
                new ChatUiNoticeBlock { Id = "b2", Text = "note" }
            }
        });
        var after = Timeline(new ChatUiMessage
        {
            Id = "m1", Role = ChatUiRole.Tool,
            Blocks = new ChatUiBlock[]
            {
                new ChatUiToolCallBlock { Id = "b1", ToolName = "echo", Status = ChatUiStatus.Success }
            }
        });

        var op = ChatUiRenderPlanner.Plan(before, after);

        Assert.Equal(ChatUiRenderOperationKind.PayloadPatch, op.Kind);
    }

    [Fact]
    public void Payload_diff_reports_no_changes_for_identical_timelines()
    {
        var timeline = Timeline(UserMessage("u1", "hi"));

        var patch = ChatUiPayloadDiff.BuildPatch(timeline, timeline);

        Assert.False(ChatUiPayloadDiff.HasChanges(patch));
    }

    [Fact]
    public void Payload_diff_reports_block_change_when_block_status_flips()
    {
        var before = Timeline(new ChatUiMessage
        {
            Id = "m1", Role = ChatUiRole.Tool,
            Blocks = new ChatUiBlock[] { new ChatUiToolCallBlock { Id = "b1", ToolName = "pwd", Status = ChatUiStatus.Running } }
        });
        var after = Timeline(new ChatUiMessage
        {
            Id = "m1", Role = ChatUiRole.Tool,
            Blocks = new ChatUiBlock[] { new ChatUiToolCallBlock { Id = "b1", ToolName = "pwd", Status = ChatUiStatus.Success } }
        });

        var patch = ChatUiPayloadDiff.BuildPatch(before, after);

        Assert.True(ChatUiPayloadDiff.HasChanges(patch));
    }

    private static ChatUiTimeline Timeline(params ChatUiMessage[] messages) =>
        new() { Revision = 1, Presentation = new ChatUiPresentation { Theme = "dark" }, Messages = messages };

    private static ChatUiMessage UserMessage(string id, string text) => new()
    {
        Id = id,
        Role = ChatUiRole.User,
        Blocks = new ChatUiBlock[] { new ChatUiMarkdownBlock { Id = id + ":text", Text = text } }
    };

    private static ChatUiMessage AssistantMessage(string id, string text) => new()
    {
        Id = id,
        Role = ChatUiRole.Assistant,
        IsStreaming = true,
        Blocks = new ChatUiBlock[] { new ChatUiMarkdownBlock { Id = id + ":text", Text = text, Streaming = true } }
    };
}
