using ReadOS.App.Models.ChatUi;

namespace ReadOS.App.Tests.Models.ChatUi;

// Verifies the canonical runtime event reducer mirrors the upstream
// Projection/runtime/timeline-store.js behaviour. Each event type should
// return an immutable next timeline and bump the revision; unsupported
// events leave payload unchanged except for the revision bump.

public sealed class ChatUiTimelineStoreTests
{
    private static ChatUiTimeline Empty() => new()
    {
        SchemaVersion = ChatUiTimeline.Schema,
        Id = "test",
        Title = "test",
        Revision = 0,
        Messages = Array.Empty<ChatUiMessage>()
    };

    private static ChatUiMessage User(string id, string text) => new()
    {
        Id = id,
        Role = ChatUiRole.User,
        Status = ChatUiStatus.Success,
        Blocks = new[] { new ChatUiMarkdownBlock { Id = $"{id}:text", Text = text } }
    };

    private static ChatUiMessage Assistant(string id, string text) => new()
    {
        Id = id,
        Role = ChatUiRole.Assistant,
        Status = ChatUiStatus.Running,
        IsStreaming = true,
        Blocks = new[] { new ChatUiMarkdownBlock { Id = $"{id}:text", Text = text, Streaming = true } }
    };

    [Fact]
    public void Timeline_replace_event_swaps_messages_and_preserves_presentation()
    {
        var initial = Empty();
        var replacement = new ChatUiTimeline
        {
            SchemaVersion = ChatUiTimeline.Schema,
            Id = "test",
            Title = "test",
            Revision = 5,
            Presentation = new ChatUiPresentation { Theme = "dark" },
            Messages = new[] { User("u1", "hi") }
        };

        var next = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiTimelineReplaceEvent { Timeline = replacement });

        Assert.NotSame(initial, next);
        Assert.Single(next.Messages);
        Assert.Equal("u1", next.Messages[0].Id);
        Assert.Equal("dark", next.Presentation?.Theme);
    }

    [Fact]
    public void Message_upsert_appends_when_missing()
    {
        var initial = Empty();
        var next = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiMessageUpsertEvent { Message = User("u1", "hello") });

        Assert.Single(next.Messages);
        Assert.Equal("u1", next.Messages[0].Id);
        Assert.True(next.Revision > initial.Revision);
    }

    [Fact]
    public void Message_upsert_replaces_existing_id_in_place()
    {
        var initial = Empty();
        var added = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiMessageUpsertEvent { Message = User("u1", "hello") });
        var updated = ChatUiTimelineStore.ApplyRuntimeEvent(added,
            new ChatUiMessageUpsertEvent { Message = User("u1", "world") });

        Assert.Single(updated.Messages);
        Assert.Equal("world", ((ChatUiMarkdownBlock)updated.Messages[0].Blocks[0]).Text);
    }

    [Fact]
    public void Message_remove_drops_message_by_id()
    {
        var initial = Empty();
        var a = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiMessageUpsertEvent { Message = User("u1", "hello") });
        var b = ChatUiTimelineStore.ApplyRuntimeEvent(a,
            new ChatUiMessageUpsertEvent { Message = User("u2", "world") });
        var c = ChatUiTimelineStore.ApplyRuntimeEvent(b,
            new ChatUiMessageRemoveEvent { MessageID = "u1" });

        Assert.Single(c.Messages);
        Assert.Equal("u2", c.Messages[0].Id);
    }

    [Fact]
    public void Stream_delta_appends_text_to_markdown_block()
    {
        var initial = Empty();
        var withAssistant = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiMessageUpsertEvent { Message = Assistant("a1", "Hello") });
        var next = ChatUiTimelineStore.ApplyRuntimeEvent(withAssistant,
            new ChatUiStreamDeltaEvent { MessageID = "a1", BlockID = "a1:text", TextDelta = " world" });

        var block = (ChatUiMarkdownBlock)next.Messages[0].Blocks[0];
        Assert.Equal("Hello world", block.Text);
        Assert.True(block.Streaming);
    }

    [Fact]
    public void Stream_delta_terminal_status_clears_streaming_flag()
    {
        var initial = Empty();
        var withAssistant = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiMessageUpsertEvent { Message = Assistant("a1", "Hello") });
        var next = ChatUiTimelineStore.ApplyRuntimeEvent(withAssistant,
            new ChatUiStreamDeltaEvent
            {
                MessageID = "a1",
                BlockID = "a1:text",
                TextDelta = " world",
                Status = ChatUiStatus.Success
            });

        var block = (ChatUiMarkdownBlock)next.Messages[0].Blocks[0];
        Assert.Equal("Hello world", block.Text);
        Assert.False(block.Streaming);
    }

    [Fact]
    public void Tool_lifecycle_upserts_tool_call_block()
    {
        var initial = Empty();
        var withMessage = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiMessageUpsertEvent { Message = Assistant("a1", "") });
        var next = ChatUiTimelineStore.ApplyRuntimeEvent(withMessage,
            new ChatUiToolLifecycleEvent
            {
                MessageID = "a1",
                BlockID = "a1:tool-read",
                Status = ChatUiStatus.Running,
                ToolCall = new ChatUiToolCallBlock
                {
                    Id = "a1:tool-read",
                    ToolName = "read_file",
                    Title = "reading manifest"
                }
            });

        Assert.Equal(2, next.Messages[0].Blocks.Count);
        Assert.IsType<ChatUiToolCallBlock>(next.Messages[0].Blocks[1]);
        Assert.Equal("read_file", ((ChatUiToolCallBlock)next.Messages[0].Blocks[1]).ToolName);
    }

    [Fact]
    public void Block_upsert_replaces_existing_block_by_id()
    {
        var initial = Empty();
        var withMessage = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiMessageUpsertEvent { Message = Assistant("a1", "Hello") });
        var next = ChatUiTimelineStore.ApplyRuntimeEvent(withMessage,
            new ChatUiBlockUpsertEvent
            {
                MessageID = "a1",
                BlockID = "a1:text",
                Block = new ChatUiMarkdownBlock { Id = "a1:text", Text = "Replaced" }
            });

        Assert.Equal("Replaced", ((ChatUiMarkdownBlock)next.Messages[0].Blocks[0]).Text);
    }

    [Fact]
    public void Block_remove_drops_block_by_id()
    {
        var initial = Empty();
        var withMessage = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiMessageUpsertEvent { Message = Assistant("a1", "Hello") });
        var next = ChatUiTimelineStore.ApplyRuntimeEvent(withMessage,
            new ChatUiBlockRemoveEvent { MessageID = "a1", BlockID = "a1:text" });

        Assert.Empty(next.Messages[0].Blocks);
    }

    [Fact]
    public void Interaction_collapse_updates_presentation_collapsed_blocks()
    {
        var initial = Empty();
        var withMessage = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiMessageUpsertEvent { Message = Assistant("a1", "Hello") });
        var next = ChatUiTimelineStore.ApplyRuntimeEvent(withMessage,
            new ChatUiInteractionCollapseEvent
            {
                MessageID = "a1",
                BlockID = "a1:text",
                Collapsed = true
            });

        Assert.NotNull(next.Presentation);
        Assert.True(next.Presentation!.CollapsedBlocks?["a1:text"]);
    }

    [Fact]
    public void Presentation_update_merges_theme_only()
    {
        var initial = new ChatUiTimeline
        {
            SchemaVersion = ChatUiTimeline.Schema,
            Id = "test",
            Title = "test",
            Revision = 1,
            Presentation = new ChatUiPresentation { Theme = "light", CodeTheme = "vitesse" },
            Messages = new[] { User("u1", "hi") }
        };

        var next = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiPresentationUpdateEvent
            {
                Presentation = new ChatUiPresentation { Theme = "dark" }
            });

        Assert.Equal("dark", next.Presentation?.Theme);
        Assert.Equal("vitesse", next.Presentation?.CodeTheme);
    }

    [Fact]
    public void Unknown_event_type_only_bumps_revision()
    {
        var initial = new ChatUiTimeline
        {
            SchemaVersion = ChatUiTimeline.Schema,
            Id = "test",
            Title = "test",
            Revision = 3,
            Messages = new[] { User("u1", "hi") }
        };

        var unknown = new CustomEvent();
        var next = ChatUiTimelineStore.ApplyRuntimeEvent(initial, unknown);

        Assert.Equal(3 + 1, next.Revision);
        Assert.Same(initial.Messages, next.Messages);
    }

    private sealed class CustomEvent : ChatUiRuntimeEvent
    {
        public override string Type => "unknown.custom.event";
    }
}
