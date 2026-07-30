using ReadOS.App.Models.ChatUi;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ChatUiTimelineStoreTests
{
    [Fact]
    public void Timeline_replace_replaces_state_and_normalizes()
    {
        var initial = new ChatUiTimeline { Revision = 5, Messages = new[] { UserMessage("old", "old") } };
        var replacement = new ChatUiTimeline { Messages = new[] { UserMessage("new", "new") } };

        var next = ChatUiTimelineStore.ApplyRuntimeEvent(initial, new ChatUiTimelineReplaceEvent { Timeline = replacement });

        Assert.Single(next.Messages);
        Assert.Equal("new", next.Messages[0].Id);
        Assert.NotEqual(5, next.Revision);
    }

    [Fact]
    public void Message_upsert_appends_when_absent_and_replaces_when_present()
    {
        var timeline = ChatUiTimelineStore.Normalize(new ChatUiTimeline { Messages = new[] { UserMessage("u1", "first") } });

        var appended = ChatUiTimelineStore.ApplyRuntimeEvent(timeline,
            new ChatUiMessageUpsertEvent { Message = UserMessage("u2", "second") });
        Assert.Equal(2, appended.Messages.Count);

        var replaced = ChatUiTimelineStore.ApplyRuntimeEvent(appended,
            new ChatUiMessageUpsertEvent { Message = UserMessage("u1", "updated") });
        Assert.Equal(2, replaced.Messages.Count);
        Assert.Equal("updated", ((ChatUiMarkdownBlock)replaced.Messages[0].Blocks[0]).Text);
    }

    [Fact]
    public void Message_remove_drops_the_named_message()
    {
        var timeline = new ChatUiTimeline { Messages = new[] { UserMessage("keep", "k"), UserMessage("drop", "d") } };

        var next = ChatUiTimelineStore.ApplyRuntimeEvent(timeline,
            new ChatUiMessageRemoveEvent { MessageID = "drop" });

        Assert.Single(next.Messages);
        Assert.Equal("keep", next.Messages[0].Id);
    }

    [Fact]
    public void Block_upsert_inserts_block_into_target_message()
    {
        var timeline = new ChatUiTimeline { Messages = new[] { AssistantMessage("a1", "hello") } };

        var next = ChatUiTimelineStore.ApplyRuntimeEvent(timeline,
            new ChatUiBlockUpsertEvent
            {
                MessageID = "a1",
                BlockID = "a1:tool",
                Block = new ChatUiToolCallBlock { Id = "a1:tool", ToolName = "pdf search", Status = ChatUiStatus.Running }
            });

        Assert.Equal(2, next.Messages[0].Blocks.Count);
        Assert.Equal(ChatUiBlockType.ToolCall, next.Messages[0].Blocks[1].Type);
    }

    [Fact]
    public void Block_status_updates_only_the_named_block()
    {
        var timeline = new ChatUiTimeline
        {
            Messages = new[]
            {
                new ChatUiMessage
                {
                    Id = "m1", Role = ChatUiRole.Tool,
                    Blocks = new ChatUiBlock[]
                    {
                        new ChatUiToolCallBlock { Id = "b1", ToolName = "echo", Status = ChatUiStatus.Running },
                        new ChatUiToolCallBlock { Id = "b2", ToolName = "pwd", Status = ChatUiStatus.Running }
                    }
                }
            }
        };

        var next = ChatUiTimelineStore.ApplyRuntimeEvent(timeline,
            new ChatUiBlockStatusEvent { MessageID = "m1", BlockID = "b1", Status = ChatUiStatus.Success });

        Assert.Equal(ChatUiStatus.Success, next.Messages[0].Blocks[0].Status);
        Assert.Equal(ChatUiStatus.Running, next.Messages[0].Blocks[1].Status);
    }

    [Fact]
    public void Stream_delta_appends_text_to_markdown_block_and_keeps_streaming()
    {
        var timeline = new ChatUiTimeline
        {
            Messages = new[]
            {
                new ChatUiMessage
                {
                    Id = "a1", Role = ChatUiRole.Assistant, IsStreaming = true,
                    Blocks = new ChatUiBlock[] { new ChatUiMarkdownBlock { Id = "a1:text", Text = "Hel", Streaming = true } }
                }
            }
        };

        var next = ChatUiTimelineStore.ApplyRuntimeEvent(timeline,
            new ChatUiStreamDeltaEvent { MessageID = "a1", BlockID = "a1:text", TextDelta = "lo" });

        Assert.Equal("Hello", ((ChatUiMarkdownBlock)next.Messages[0].Blocks[0]).Text);
        Assert.True(((ChatUiMarkdownBlock)next.Messages[0].Blocks[0]).Streaming);
    }

    [Fact]
    public void Stream_delta_with_terminal_status_clears_streaming_flag()
    {
        var timeline = new ChatUiTimeline
        {
            Messages = new[]
            {
                new ChatUiMessage
                {
                    Id = "a1", Role = ChatUiRole.Assistant, IsStreaming = true,
                    Blocks = new ChatUiBlock[] { new ChatUiMarkdownBlock { Id = "a1:text", Text = "Hel", Streaming = true } }
                }
            }
        };

        var next = ChatUiTimelineStore.ApplyRuntimeEvent(timeline,
            new ChatUiStreamDeltaEvent { MessageID = "a1", BlockID = "a1:text", TextDelta = "lo", Status = ChatUiStatus.Success });

        Assert.Equal("Hello", ((ChatUiMarkdownBlock)next.Messages[0].Blocks[0]).Text);
        Assert.False(((ChatUiMarkdownBlock)next.Messages[0].Blocks[0]).Streaming);
    }

    [Fact]
    public void Tool_lifecycle_upserts_tool_call_block_with_status()
    {
        var timeline = new ChatUiTimeline
        {
            Messages = new[]
            {
                new ChatUiMessage { Id = "a1", Role = ChatUiRole.Assistant, Blocks = Array.Empty<ChatUiBlock>() }
            }
        };

        var next = ChatUiTimelineStore.ApplyRuntimeEvent(timeline,
            new ChatUiToolLifecycleEvent
            {
                MessageID = "a1", BlockID = "a1:tool", Status = ChatUiStatus.Success,
                ToolCall = new ChatUiToolCallBlock { Id = "a1:tool", ToolName = "pdf text", OutputText = "extracted" }
            });

        Assert.Single(next.Messages[0].Blocks);
        var block = Assert.IsType<ChatUiToolCallBlock>(next.Messages[0].Blocks[0]);
        Assert.Equal("pdf text", block.ToolName);
        Assert.Equal("extracted", block.OutputText);
        Assert.Equal(ChatUiStatus.Success, block.Status);
    }

    [Fact]
    public void Interaction_collapse_records_collapsed_block_in_presentation()
    {
        var timeline = ChatUiTimelineStore.Normalize(new ChatUiTimeline
        {
            Messages = new[] { AssistantMessage("a1", "hello") }
        });

        var next = ChatUiTimelineStore.ApplyRuntimeEvent(timeline,
            new ChatUiInteractionCollapseEvent { MessageID = "a1", BlockID = "a1:text", Collapsed = true });

        Assert.NotNull(next.Presentation);
        Assert.True(next.Presentation!.CollapsedBlocks?["a1:text"]);
    }

    [Fact]
    public void Selection_update_records_selection_in_presentation()
    {
        var timeline = ChatUiTimelineStore.Normalize(new ChatUiTimeline
        {
            Messages = new[] { AssistantMessage("a1", "hello") }
        });

        var next = ChatUiTimelineStore.ApplyRuntimeEvent(timeline,
            new ChatUiSelectionUpdateEvent { Selection = new Dictionary<string, object?> { ["start"] = 0 } });

        Assert.NotNull(next.Presentation?.Style);
    }

    [Fact]
    public void Scroll_sync_only_bumps_revision()
    {
        var timeline = ChatUiTimelineStore.Normalize(new ChatUiTimeline { Revision = 3, Messages = new[] { UserMessage("u1", "hi") } });

        var next = ChatUiTimelineStore.ApplyRuntimeEvent(timeline, new ChatUiScrollSyncEvent());

        Assert.Equal(4, next.Revision);
        Assert.Same(timeline.Messages, next.Messages);
    }

    [Fact]
    public void ApplyRuntime_events_reduces_a_sequence()
    {
        var timeline = ChatUiTimelineStore.ApplyRuntimeEvents(null, new ChatUiRuntimeEvent[]
        {
            new ChatUiTimelineReplaceEvent { Timeline = new ChatUiTimeline { Messages = new[] { UserMessage("u1", "q") } } },
            new ChatUiMessageUpsertEvent { Message = AssistantMessage("a1", "") },
            new ChatUiBlockUpsertEvent { MessageID = "a1", BlockID = "a1:text", Block = new ChatUiMarkdownBlock { Id = "a1:text", Text = "" } },
            new ChatUiStreamDeltaEvent { MessageID = "a1", BlockID = "a1:text", TextDelta = "Hello" }
        });

        Assert.Equal(2, timeline.Messages.Count);
        Assert.Equal("Hello", ((ChatUiMarkdownBlock)timeline.Messages[1].Blocks[0]).Text);
    }

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
        Blocks = string.IsNullOrEmpty(text)
            ? Array.Empty<ChatUiBlock>()
            : new ChatUiBlock[] { new ChatUiMarkdownBlock { Id = id + ":text", Text = text, Streaming = true } }
    };
}
