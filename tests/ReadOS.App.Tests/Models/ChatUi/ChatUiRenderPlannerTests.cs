using ReadOS.App.Models.ChatUi;

namespace ReadOS.App.Tests.Models.ChatUi;

// Verifies the canonical render operation planner picks the cheapest operation
// for each transition. Mirrors Projection/runtime/render-planner.js + the
// upstream parity checklist under
// src/ReadOS.Web/MSPChatUI/Conformance/DefaultParityChecklist.md.

public sealed class ChatUiRenderPlannerTests
{
    private static ChatUiTimeline Timeline(params ChatUiMessage[] messages) => new()
    {
        SchemaVersion = ChatUiTimeline.Schema,
        Id = "test",
        Title = "test",
        Revision = 1,
        Presentation = new ChatUiPresentation { Theme = "light" },
        Messages = messages
    };

    private static ChatUiMessage User(string id, string text) => new()
    {
        Id = id,
        Role = ChatUiRole.User,
        Status = ChatUiStatus.Success,
        Blocks = new[] { new ChatUiMarkdownBlock { Id = $"{id}:text", Text = text } }
    };

    private static ChatUiMessage Assistant(string id, string text, bool streaming = false) => new()
    {
        Id = id,
        Role = ChatUiRole.Assistant,
        Status = streaming ? ChatUiStatus.Running : ChatUiStatus.Success,
        IsStreaming = streaming,
        Blocks = new[]
        {
            new ChatUiMarkdownBlock { Id = $"{id}:text", Text = text, Streaming = streaming ? true : null }
        }
    };

    [Fact]
    public void First_render_always_full_render()
    {
        var next = Timeline(User("u1", "hi"));

        var op = ChatUiRenderPlanner.Plan(null, next);

        Assert.IsType<ChatUiFullRenderOperation>(op);
    }

    [Fact]
    public void Identical_payload_chooses_scroll_sync()
    {
        var a = Timeline(User("u1", "hi"));
        var b = Timeline(User("u1", "hi"));

        var op = ChatUiRenderPlanner.Plan(a, b);

        Assert.IsType<ChatUiScrollSyncOperation>(op);
    }

    [Fact]
    public void Presentation_only_change_picks_presentation_only_update()
    {
        var a = Timeline(User("u1", "hi"));
        var b = new ChatUiTimeline
        {
            SchemaVersion = a.SchemaVersion,
            Id = a.Id,
            Title = a.Title,
            Revision = a.Revision + 1,
            Presentation = new ChatUiPresentation { Theme = "dark" },
            Messages = a.Messages
        };

        var op = ChatUiRenderPlanner.Plan(a, b);

        Assert.IsType<ChatUiPresentationOnlyUpdateOperation>(op);
    }

    [Fact]
    public void Streaming_text_append_picks_direct_streaming_update()
    {
        var prev = Timeline(Assistant("a1", "Hello", streaming: true));
        var next = Timeline(Assistant("a1", "Hello world", streaming: true));

        var op = ChatUiRenderPlanner.Plan(prev, next);

        Assert.IsType<ChatUiDirectStreamingUpdateOperation>(op);
    }

    [Fact]
    public void Added_message_picks_payload_patch()
    {
        var prev = Timeline(User("u1", "hi"));
        var next = Timeline(User("u1", "hi"), User("u2", "second"));

        var op = ChatUiRenderPlanner.Plan(prev, next);

        Assert.IsType<ChatUiPayloadPatchOperation>(op);
    }

    [Fact]
    public void Changed_block_text_picks_payload_patch()
    {
        var prev = Timeline(User("u1", "hi"));
        var next = Timeline(User("u1", "edited"));

        var op = ChatUiRenderPlanner.Plan(prev, next);

        Assert.IsType<ChatUiPayloadPatchOperation>(op);
    }

    [Fact]
    public void Streaming_update_on_non_assistant_role_falls_back_to_patch()
    {
        var prev = Timeline(User("u1", "Hello"));
        var next = Timeline(User("u1", "Hello world"));

        var op = ChatUiRenderPlanner.Plan(prev, next);

        Assert.IsType<ChatUiPayloadPatchOperation>(op);
    }

    [Fact]
    public void Streaming_update_on_non_prefix_text_falls_back_to_patch()
    {
        // Block text changed entirely (not a prefix append), so the direct
        // streaming fast path should not apply.
        var prev = Timeline(Assistant("a1", "Hello", streaming: true));
        var next = Timeline(Assistant("a1", "Hi there", streaming: true));

        var op = ChatUiRenderPlanner.Plan(prev, next);

        Assert.IsType<ChatUiPayloadPatchOperation>(op);
    }
}
