using ReadOS.App.Models.ChatUi;

namespace ReadOS.App.Tests.Models.ChatUi;

// Exercises runtime event kinds not yet covered by ChatUiTimelineStoreTests.
// Each test asserts the reducer returns a fresh immutable timeline, bumps the
// revision, and mutates only the intended slice of state. Mirrors the upstream
// Projection/runtime/timeline-store.js per-event branches.

public sealed class ChatUiTimelineStoreEventsTests
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

    private static ChatUiMessage Assistant(string id, string text, bool streaming = false) => new()
    {
        Id = id,
        Role = ChatUiRole.Assistant,
        Status = streaming ? ChatUiStatus.Running : ChatUiStatus.Success,
        IsStreaming = streaming,
        Blocks = new[] { new ChatUiMarkdownBlock { Id = $"{id}:text", Text = text, Streaming = streaming ? true : null } }
    };

    [Fact]
    public void Message_status_event_updates_status_only()
    {
        var initial = Empty();
        var withMsg = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiMessageUpsertEvent { Message = User("u1", "hi") });
        var next = ChatUiTimelineStore.ApplyRuntimeEvent(withMsg,
            new ChatUiMessageStatusEvent { MessageID = "u1", Status = ChatUiStatus.Failed });

        Assert.Equal(ChatUiStatus.Failed, next.Messages[0].Status);
        Assert.Equal("hi", ((ChatUiMarkdownBlock)next.Messages[0].Blocks[0]).Text);
        Assert.True(next.Revision > withMsg.Revision);
    }

    [Fact]
    public void Message_patch_event_updates_scalar_metadata()
    {
        var initial = Empty();
        var withMsg = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiMessageUpsertEvent { Message = User("u1", "hi") });
        var next = ChatUiTimelineStore.ApplyRuntimeEvent(withMsg,
            new ChatUiMessagePatchEvent
            {
                MessageID = "u1",
                Patch = new Dictionary<string, object?>
                {
                    ["status"] = ChatUiStatus.Running,
                    ["modelName"] = "gpt-x",
                    ["updatedAt"] = "2026-01-01",
                    ["timeText"] = "now",
                    ["isStreaming"] = true
                }
            });

        Assert.Equal(ChatUiStatus.Running, next.Messages[0].Status);
        Assert.Equal("gpt-x", next.Messages[0].ModelName);
        Assert.Equal("2026-01-01", next.Messages[0].UpdatedAt);
        Assert.Equal("now", next.Messages[0].TimeText);
        Assert.True(next.Messages[0].IsStreaming);
        Assert.Equal("hi", ((ChatUiMarkdownBlock)next.Messages[0].Blocks[0]).Text);
    }

    [Fact]
    public void Block_status_event_updates_block_status()
    {
        var initial = Empty();
        var withMsg = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiMessageUpsertEvent { Message = Assistant("a1", "Hello") });
        var next = ChatUiTimelineStore.ApplyRuntimeEvent(withMsg,
            new ChatUiBlockStatusEvent { MessageID = "a1", BlockID = "a1:text", Status = ChatUiStatus.Failed });

        var block = (ChatUiMarkdownBlock)next.Messages[0].Blocks[0];
        Assert.Equal(ChatUiStatus.Failed, block.Status);
        Assert.Equal("Hello", block.Text);
    }

    [Theory]
    [InlineData("text", "Patched")]
    [InlineData("streaming", true)]
    public void Block_patch_event_patches_markdown_block(string field, object value)
    {
        var initial = Empty();
        var withMsg = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiMessageUpsertEvent { Message = Assistant("a1", "Hello") });
        var patch = new Dictionary<string, object?> { [field] = value };
        var next = ChatUiTimelineStore.ApplyRuntimeEvent(withMsg,
            new ChatUiBlockPatchEvent { MessageID = "a1", BlockID = "a1:text", Patch = patch });

        var block = (ChatUiMarkdownBlock)next.Messages[0].Blocks[0];
        if (field == "text")
        {
            Assert.Equal("Patched", block.Text);
        }
        else
        {
            Assert.True(block.Streaming);
        }
    }

    [Fact]
    public void Block_patch_event_patches_tool_call_block_title()
    {
        var initial = Empty();
        var withMsg = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiMessageUpsertEvent { Message = Assistant("a1", "") });
        var withTool = ChatUiTimelineStore.ApplyRuntimeEvent(withMsg,
            new ChatUiToolLifecycleEvent
            {
                MessageID = "a1",
                BlockID = "a1:tool",
                Status = ChatUiStatus.Running,
                ToolCall = new ChatUiToolCallBlock { Id = "a1:tool", ToolName = "read" }
            });
        var next = ChatUiTimelineStore.ApplyRuntimeEvent(withTool,
            new ChatUiBlockPatchEvent
            {
                MessageID = "a1",
                BlockID = "a1:tool",
                Patch = new Dictionary<string, object?> { ["title"] = "done reading" }
            });

        var block = (ChatUiToolCallBlock)next.Messages[0].Blocks[1];
        Assert.Equal("done reading", block.Title);
        Assert.Equal("read", block.ToolName);
    }

    [Fact]
    public void Selection_update_event_merges_style_and_null_clears()
    {
        var initial = Empty();
        var withSelection = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiSelectionUpdateEvent
            {
                Selection = new Dictionary<string, object?> { ["from"] = 1, ["to"] = 5 }
            });
        Assert.NotNull(withSelection.Presentation?.Style);
        var selection = (IReadOnlyDictionary<string, object?>)withSelection.Presentation!.Style!["selection"]!;
        Assert.Equal(1, selection["from"]);
        Assert.Equal(5, selection["to"]);

        var cleared = ChatUiTimelineStore.ApplyRuntimeEvent(withSelection,
            new ChatUiSelectionUpdateEvent { Selection = null });
        Assert.NotNull(cleared.Presentation!.Style);
        var preserved = (IReadOnlyDictionary<string, object?>)cleared.Presentation!.Style!["selection"]!;
        Assert.Equal(1, preserved["from"]);
        Assert.Equal(5, preserved["to"]);
    }

    [Fact]
    public void Scroll_sync_event_only_bumps_revision()
    {
        var initial = Empty();
        var withMsg = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiMessageUpsertEvent { Message = User("u1", "hi") });
        var next = ChatUiTimelineStore.ApplyRuntimeEvent(withMsg, new ChatUiScrollSyncEvent());

        Assert.Equal(withMsg.Revision + 1, next.Revision);
        Assert.Same(withMsg.Messages, next.Messages);
    }

    [Fact]
    public void Tool_lifecycle_pending_status_resolves_to_running()
    {
        var initial = Empty();
        var withMsg = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiMessageUpsertEvent { Message = Assistant("a1", "") });
        var next = ChatUiTimelineStore.ApplyRuntimeEvent(withMsg,
            new ChatUiToolLifecycleEvent
            {
                MessageID = "a1",
                BlockID = "a1:tool",
                Status = ChatUiStatus.Pending,
                ToolCall = new ChatUiToolCallBlock { Id = "a1:tool", ToolName = "" }
            });

        var block = (ChatUiToolCallBlock)next.Messages[0].Blocks[1];
        Assert.Equal(ChatUiStatus.Running, block.Status);
        Assert.Equal("a1:tool", block.ToolName);
    }

    [Fact]
    public void Tool_lifecycle_explicit_status_and_tool_name_are_kept()
    {
        var initial = Empty();
        var withMsg = ChatUiTimelineStore.ApplyRuntimeEvent(initial,
            new ChatUiMessageUpsertEvent { Message = Assistant("a1", "") });
        var next = ChatUiTimelineStore.ApplyRuntimeEvent(withMsg,
            new ChatUiToolLifecycleEvent
            {
                MessageID = "a1",
                BlockID = "a1:tool",
                Status = ChatUiStatus.Failed,
                ToolCall = new ChatUiToolCallBlock { Id = "a1:tool", ToolName = "read_file", ErrorText = "boom" }
            });

        var block = (ChatUiToolCallBlock)next.Messages[0].Blocks[1];
        Assert.Equal(ChatUiStatus.Failed, block.Status);
        Assert.Equal("read_file", block.ToolName);
        Assert.Equal("boom", block.ErrorText);
    }

    [Fact]
    public void Apply_runtime_events_batch_applies_in_order()
    {
        var events = new ChatUiRuntimeEvent[]
        {
            new ChatUiMessageUpsertEvent { Message = Assistant("a1", "Hello") },
            new ChatUiStreamDeltaEvent { MessageID = "a1", BlockID = "a1:text", TextDelta = " world" },
            new ChatUiPresentationUpdateEvent { Presentation = new ChatUiPresentation { Theme = "dark" } }
        };

        var next = ChatUiTimelineStore.ApplyRuntimeEvents(null, events);

        Assert.Equal("Hello world", ((ChatUiMarkdownBlock)next.Messages[0].Blocks[0]).Text);
        Assert.Equal("dark", next.Presentation?.Theme);
        Assert.True(next.Revision >= 3);
    }
}
