using ReadOS.App.Models.ChatUi;

namespace ReadOS.App.Tests.Models.ChatUi;

// Edge cases for the render planner's streaming fast path and presentation-only
// transitions that ChatUiRenderPlannerTests does not yet cover.

public sealed class ChatUiRenderPlannerStreamingTests
{
    private static ChatUiTimeline Timeline(ChatUiPresentation? presentation, params ChatUiMessage[] messages) => new()
    {
        SchemaVersion = ChatUiTimeline.Schema,
        Id = "test",
        Title = "test",
        Revision = 1,
        Presentation = presentation,
        Messages = messages
    };

    private static ChatUiMessage User(string id, string text) => new()
    {
        Id = id,
        Role = ChatUiRole.User,
        Status = ChatUiStatus.Success,
        Blocks = new[] { new ChatUiMarkdownBlock { Id = $"{id}:text", Text = text } }
    };

    private static ChatUiMessage Assistant(string id, string text, bool streaming, string? blockType = null) => new()
    {
        Id = id,
        Role = ChatUiRole.Assistant,
        Status = streaming ? ChatUiStatus.Running : ChatUiStatus.Success,
        IsStreaming = streaming,
        Blocks = blockType == "tool"
            ? new ChatUiBlock[] { new ChatUiToolCallBlock { Id = $"{id}:tool", ToolName = "read", Status = ChatUiStatus.Running } }
            : new ChatUiBlock[] { new ChatUiMarkdownBlock { Id = $"{id}:text", Text = text, Streaming = streaming ? true : null } }
    };

    [Fact]
    public void Collapse_change_with_same_payload_picks_presentation_only_update()
    {
        var a = Timeline(new ChatUiPresentation { Theme = "light" }, User("u1", "hi"));
        var b = Timeline(
            new ChatUiPresentation { Theme = "light", CollapsedBlocks = new Dictionary<string, bool> { ["u1:text"] = true } },
            User("u1", "hi"));

        var op = ChatUiRenderPlanner.Plan(a, b);

        Assert.IsType<ChatUiPresentationOnlyUpdateOperation>(op);
    }

    [Fact]
    public void Terminal_stream_delta_with_prefix_text_still_uses_direct_streaming_update()
    {
        // The message was streaming, the delta completed (streaming cleared) but
        // the new text is still a prefix append, so the cheap path applies.
        var prev = Timeline(null, Assistant("a1", "Hello", streaming: true));
        var next = Timeline(null, Assistant("a1", "Hello world", streaming: false));

        var op = ChatUiRenderPlanner.Plan(prev, next);

        Assert.IsType<ChatUiDirectStreamingUpdateOperation>(op);
    }

    [Fact]
    public void Non_markdown_streaming_block_change_falls_back_to_patch()
    {
        // The direct-streaming fast path only applies to markdown blocks; a
        // tool-call status change mid-stream must use a payload patch.
        var prev = Timeline(null, Assistant("a1", string.Empty, streaming: true, blockType: "tool"));
        var nextMsg = new ChatUiMessage
        {
            Id = "a1",
            Role = ChatUiRole.Assistant,
            Status = ChatUiStatus.Running,
            IsStreaming = true,
            Blocks = new ChatUiBlock[] { new ChatUiToolCallBlock { Id = "a1:tool", ToolName = "read", Status = ChatUiStatus.Success } }
        };
        var next = Timeline(null, nextMsg);

        var op = ChatUiRenderPlanner.Plan(prev, next);

        Assert.IsType<ChatUiPayloadPatchOperation>(op);
    }

    [Fact]
    public void Is_conversation_generating_change_picks_presentation_only_update()
    {
        var a = Timeline(new ChatUiPresentation { Theme = "light", IsConversationGenerating = false }, User("u1", "hi"));
        var b = Timeline(new ChatUiPresentation { Theme = "light", IsConversationGenerating = true }, User("u1", "hi"));

        var op = ChatUiRenderPlanner.Plan(a, b);

        Assert.IsType<ChatUiPresentationOnlyUpdateOperation>(op);
    }
}
