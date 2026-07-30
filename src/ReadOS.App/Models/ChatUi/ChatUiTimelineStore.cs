using System.Collections.Immutable;

namespace ReadOS.App.Models.ChatUi;

// Canonical runtime event reducer.
//
// Mirrors Projection/runtime/timeline-store.js. applyRuntimeEvent takes an
// immutable timeline state plus a single runtime event and returns the next
// immutable timeline. Every event bumps the revision so the render planner can
// detect change. Validation fails closed: an unknown event type leaves the
// timeline unchanged except for the revision bump, matching the reference
// fallback branch.
//
// This is a pure model layer. It must not reference WinUI, DOM, or any
// renderer. The reducer is the single source of truth for timeline state;
// renderers only read what it produces.

public static class ChatUiTimelineStore
{
    public static ChatUiTimeline ApplyRuntimeEvent(ChatUiTimeline? current, ChatUiRuntimeEvent runtimeEvent)
    {
        if (runtimeEvent is ChatUiTimelineReplaceEvent replace)
        {
            return Normalize(replace.Timeline);
        }

        var normalized = Normalize(current);
        return runtimeEvent switch
        {
            ChatUiPresentationUpdateEvent e => UpdatePresentation(normalized, e),
            ChatUiMessageUpsertEvent e => UpsertMessage(normalized, e),
            ChatUiMessageRemoveEvent e => RemoveMessage(normalized, e),
            ChatUiMessageStatusEvent e => UpdateMessageStatus(normalized, e),
            ChatUiMessagePatchEvent e => PatchMessage(normalized, e),
            ChatUiBlockUpsertEvent e => UpsertBlock(normalized, e),
            ChatUiBlockRemoveEvent e => RemoveBlock(normalized, e),
            ChatUiBlockPatchEvent e => PatchBlock(normalized, e),
            ChatUiBlockStatusEvent e => UpdateBlockStatus(normalized, e),
            ChatUiStreamDeltaEvent e => AppendStreamDelta(normalized, e),
            ChatUiToolLifecycleEvent e => UpsertToolLifecycle(normalized, e),
            ChatUiInteractionCollapseEvent e => UpdateCollapseState(normalized, e),
            ChatUiSelectionUpdateEvent e => UpdateSelection(normalized, e),
            ChatUiScrollSyncEvent => NextRevision(normalized),
            _ => NextRevision(normalized)
        };
    }

    public static ChatUiTimeline ApplyRuntimeEvents(ChatUiTimeline? current, IEnumerable<ChatUiRuntimeEvent> events)
    {
        var state = Normalize(current);
        foreach (var e in events)
        {
            state = ApplyRuntimeEvent(state, e);
        }
        return state;
    }

    public static ChatUiTimeline Normalize(ChatUiTimeline? timeline)
    {
        if (timeline is null)
        {
            return new ChatUiTimeline { Revision = 0, Messages = Array.Empty<ChatUiMessage>() };
        }
        if (timeline.Messages.Count == 0)
        {
            return timeline;
        }
        return timeline;
    }

    private static ChatUiTimeline NextRevision(ChatUiTimeline t) =>
        new() { SchemaVersion = t.SchemaVersion, Id = t.Id, Title = t.Title, Revision = t.Revision + 1, Presentation = t.Presentation, Messages = t.Messages };

    private static ChatUiTimeline With(this ChatUiTimeline t, IReadOnlyList<ChatUiMessage> messages) =>
        new() { SchemaVersion = t.SchemaVersion, Id = t.Id, Title = t.Title, Revision = t.Revision + 1, Presentation = t.Presentation, Messages = messages };

    private static ChatUiTimeline With(this ChatUiTimeline t, ChatUiPresentation presentation) =>
        new() { SchemaVersion = t.SchemaVersion, Id = t.Id, Title = t.Title, Revision = t.Revision + 1, Presentation = presentation, Messages = t.Messages };

    private static ChatUiMessage MapMessage(IReadOnlyList<ChatUiMessage> messages, string messageID, Func<ChatUiMessage, ChatUiMessage> transform)
    {
        for (var i = 0; i < messages.Count; i++)
        {
            if (messages[i].Id == messageID)
            {
                return transform(messages[i]);
            }
        }
        return messages[0];
    }

    private static IReadOnlyList<ChatUiMessage> ReplaceMessage(IReadOnlyList<ChatUiMessage> messages, string id, ChatUiMessage next)
    {
        var list = messages.ToList();
        for (var i = 0; i < list.Count; i++)
        {
            if (list[i].Id == id)
            {
                list[i] = next;
                return list.ToImmutableArray();
            }
        }
        list.Add(next);
        return list.ToImmutableArray();
    }

    private static IReadOnlyList<ChatUiBlock> ReplaceBlock(IReadOnlyList<ChatUiBlock> blocks, string id, ChatUiBlock next)
    {
        var list = blocks.ToList();
        for (var i = 0; i < list.Count; i++)
        {
            if (list[i].Id == id)
            {
                list[i] = next;
                return list.ToImmutableArray();
            }
        }
        list.Add(next);
        return list.ToImmutableArray();
    }

    private static ChatUiTimeline UpdatePresentation(ChatUiTimeline t, ChatUiPresentationUpdateEvent e)
    {
        var merged = MergePresentation(t.Presentation, e.Presentation);
        return t.With(merged);
    }

    private static ChatUiTimeline UpsertMessage(ChatUiTimeline t, ChatUiMessageUpsertEvent e)
    {
        var message = e.Message;
        if (string.IsNullOrEmpty(message.Id) && !string.IsNullOrEmpty(e.MessageID))
        {
            message = new ChatUiMessage
            {
                Id = e.MessageID,
                Role = message.Role,
                Status = message.Status,
                ModelName = message.ModelName,
                CreatedAt = message.CreatedAt,
                UpdatedAt = message.UpdatedAt,
                TimeText = message.TimeText,
                CompletedGoalDurationMs = message.CompletedGoalDurationMs,
                MemoryCitation = message.MemoryCitation,
                HasRenderPatches = message.HasRenderPatches,
                HasEnabledRenderPatches = message.HasEnabledRenderPatches,
                Blocks = message.Blocks,
                IsStreaming = message.IsStreaming
            };
        }
        return t.With(ReplaceMessage(t.Messages, message.Id, message));
    }

    private static ChatUiTimeline RemoveMessage(ChatUiTimeline t, ChatUiMessageRemoveEvent e)
    {
        var next = t.Messages.Where(m => m.Id != e.MessageID).ToImmutableArray();
        return t.With(next);
    }

    private static ChatUiTimeline UpdateMessageStatus(ChatUiTimeline t, ChatUiMessageStatusEvent e)
    {
        var next = t.Messages.Select(m => m.Id == e.MessageID
            ? new ChatUiMessage
            {
                Id = m.Id, Role = m.Role, Status = e.Status, ModelName = m.ModelName,
                CreatedAt = m.CreatedAt, UpdatedAt = m.UpdatedAt, TimeText = m.TimeText,
                CompletedGoalDurationMs = m.CompletedGoalDurationMs, MemoryCitation = m.MemoryCitation,
                HasRenderPatches = m.HasRenderPatches, HasEnabledRenderPatches = m.HasEnabledRenderPatches,
                Blocks = m.Blocks, IsStreaming = m.IsStreaming
            }
            : m).ToImmutableArray();
        return t.With(next);
    }

    private static ChatUiTimeline PatchMessage(ChatUiTimeline t, ChatUiMessagePatchEvent e)
    {
        var next = t.Messages.Select(m => m.Id == e.MessageID ? ApplyMessagePatch(m, e.Patch) : m).ToImmutableArray();
        return t.With(next);
    }

    private static ChatUiMessage ApplyMessagePatch(ChatUiMessage m, IReadOnlyDictionary<string, object?> patch)
    {
        // Message patches only touch scalar metadata; block edits use block events.
        return new ChatUiMessage
        {
            Id = m.Id,
            Role = m.Role,
            Status = patch.TryGetValue("status", out var s) && s is ChatUiStatus st ? st : m.Status,
            ModelName = patch.TryGetValue("modelName", out var mn) && mn is string ms ? ms : m.ModelName,
            CreatedAt = m.CreatedAt,
            UpdatedAt = patch.TryGetValue("updatedAt", out var u) && u is string us ? us : m.UpdatedAt,
            TimeText = patch.TryGetValue("timeText", out var tt) && tt is string ts ? ts : m.TimeText,
            CompletedGoalDurationMs = m.CompletedGoalDurationMs,
            MemoryCitation = m.MemoryCitation,
            HasRenderPatches = m.HasRenderPatches,
            HasEnabledRenderPatches = m.HasEnabledRenderPatches,
            Blocks = m.Blocks,
            IsStreaming = patch.TryGetValue("isStreaming", out var iss) && iss is bool b ? b : m.IsStreaming
        };
    }

    private static ChatUiTimeline UpsertBlock(ChatUiTimeline t, ChatUiBlockUpsertEvent e)
    {
        var block = e.Block;
        var next = t.Messages.Select(m =>
        {
            if (m.Id != e.MessageID) return m;
            return new ChatUiMessage
            {
                Id = m.Id, Role = m.Role, Status = m.Status, ModelName = m.ModelName,
                CreatedAt = m.CreatedAt, UpdatedAt = m.UpdatedAt, TimeText = m.TimeText,
                CompletedGoalDurationMs = m.CompletedGoalDurationMs, MemoryCitation = m.MemoryCitation,
                HasRenderPatches = m.HasRenderPatches, HasEnabledRenderPatches = m.HasEnabledRenderPatches,
                IsStreaming = m.IsStreaming,
                Blocks = ReplaceBlock(m.Blocks, block.Id, block)
            };
        }).ToImmutableArray();
        return t.With(next);
    }

    private static ChatUiTimeline RemoveBlock(ChatUiTimeline t, ChatUiBlockRemoveEvent e)
    {
        var next = t.Messages.Select(m =>
        {
            if (m.Id != e.MessageID) return m;
            return new ChatUiMessage
            {
                Id = m.Id, Role = m.Role, Status = m.Status, ModelName = m.ModelName,
                CreatedAt = m.CreatedAt, UpdatedAt = m.UpdatedAt, TimeText = m.TimeText,
                CompletedGoalDurationMs = m.CompletedGoalDurationMs, MemoryCitation = m.MemoryCitation,
                HasRenderPatches = m.HasRenderPatches, HasEnabledRenderPatches = m.HasEnabledRenderPatches,
                IsStreaming = m.IsStreaming,
                Blocks = m.Blocks.Where(b => b.Id != e.BlockID).ToImmutableArray()
            };
        }).ToImmutableArray();
        return t.With(next);
    }

    private static ChatUiTimeline PatchBlock(ChatUiTimeline t, ChatUiBlockPatchEvent e)
    {
        var next = t.Messages.Select(m =>
        {
            if (m.Id != e.MessageID) return m;
            return new ChatUiMessage
            {
                Id = m.Id, Role = m.Role, Status = m.Status, ModelName = m.ModelName,
                CreatedAt = m.CreatedAt, UpdatedAt = m.UpdatedAt, TimeText = m.TimeText,
                CompletedGoalDurationMs = m.CompletedGoalDurationMs, MemoryCitation = m.MemoryCitation,
                HasRenderPatches = m.HasRenderPatches, HasEnabledRenderPatches = m.HasEnabledRenderPatches,
                IsStreaming = m.IsStreaming,
                Blocks = m.Blocks.Select(b => b.Id == e.BlockID ? ApplyBlockPatch(b, e.Patch) : b).ToImmutableArray()
            };
        }).ToImmutableArray();
        return t.With(next);
    }

    private static ChatUiBlock ApplyBlockPatch(ChatUiBlock block, IReadOnlyDictionary<string, object?> patch)
    {
        var status = patch.TryGetValue("status", out var s) && s is ChatUiStatus st ? st : block.Status;
        return block switch
        {
            ChatUiMarkdownBlock md => new ChatUiMarkdownBlock
            {
                Id = md.Id,
                Status = status,
                Text = patch.TryGetValue("text", out var t) && t is string ts ? ts : md.Text,
                Streaming = patch.TryGetValue("streaming", out var ss) && ss is bool b ? b : md.Streaming
            },
            ChatUiToolCallBlock tc => new ChatUiToolCallBlock
            {
                Id = tc.Id, Status = status, ToolName = tc.ToolName,
                Title = patch.TryGetValue("title", out var ti) && ti is string tis ? tis : tc.Title,
                DetailText = patch.TryGetValue("detailText", out var d) && d is string ds ? ds : tc.DetailText,
                DurationMs = tc.DurationMs,
                OutputText = patch.TryGetValue("outputText", out var o) && o is string os ? os : tc.OutputText,
                ErrorText = patch.TryGetValue("errorText", out var er) && er is string ers ? ers : tc.ErrorText
            },
            _ => block
        };
    }

    private static ChatUiTimeline UpdateBlockStatus(ChatUiTimeline t, ChatUiBlockStatusEvent e)
    {
        var next = t.Messages.Select(m =>
        {
            if (m.Id != e.MessageID) return m;
            return new ChatUiMessage
            {
                Id = m.Id, Role = m.Role, Status = m.Status, ModelName = m.ModelName,
                CreatedAt = m.CreatedAt, UpdatedAt = m.UpdatedAt, TimeText = m.TimeText,
                CompletedGoalDurationMs = m.CompletedGoalDurationMs, MemoryCitation = m.MemoryCitation,
                HasRenderPatches = m.HasRenderPatches, HasEnabledRenderPatches = m.HasEnabledRenderPatches,
                IsStreaming = m.IsStreaming,
                Blocks = m.Blocks.Select(b => b.Id == e.BlockID ? WithStatus(b, e.Status) : b).ToImmutableArray()
            };
        }).ToImmutableArray();
        return t.With(next);
    }

    private static ChatUiBlock WithStatus(ChatUiBlock block, ChatUiStatus status)
    {
        return block switch
        {
            ChatUiMarkdownBlock md => new ChatUiMarkdownBlock { Id = md.Id, Status = status, Text = md.Text, Streaming = md.Streaming },
            ChatUiToolCallBlock tc => new ChatUiToolCallBlock { Id = tc.Id, Status = status, ToolName = tc.ToolName, Title = tc.Title, DetailText = tc.DetailText, DurationMs = tc.DurationMs, OutputText = tc.OutputText, ErrorText = tc.ErrorText },
            ChatUiProgressBlock p => new ChatUiProgressBlock { Id = p.Id, Status = status, Title = p.Title, DetailText = p.DetailText, Progress = p.Progress },
            ChatUiNoticeBlock n => new ChatUiNoticeBlock { Id = n.Id, Status = status, Text = n.Text },
            _ => block
        };
    }

    private static ChatUiTimeline AppendStreamDelta(ChatUiTimeline t, ChatUiStreamDeltaEvent e)
    {
        var next = t.Messages.Select(m =>
        {
            if (m.Id != e.MessageID) return m;
            return new ChatUiMessage
            {
                Id = m.Id, Role = m.Role, Status = m.Status, ModelName = m.ModelName,
                CreatedAt = m.CreatedAt, UpdatedAt = m.UpdatedAt, TimeText = m.TimeText,
                CompletedGoalDurationMs = m.CompletedGoalDurationMs, MemoryCitation = m.MemoryCitation,
                HasRenderPatches = m.HasRenderPatches, HasEnabledRenderPatches = m.HasEnabledRenderPatches,
                IsStreaming = m.IsStreaming,
                Blocks = m.Blocks.Select(b => b.Id == e.BlockID ? ChatUiStreamDelta.AppendText(b, e) : b).ToImmutableArray()
            };
        }).ToImmutableArray();
        return t.With(next);
    }

    private static ChatUiTimeline UpsertToolLifecycle(ChatUiTimeline t, ChatUiToolLifecycleEvent e)
    {
        var source = e.ToolCall;
        var block = new ChatUiToolCallBlock
        {
            Id = e.BlockID,
            Status = e.Status != ChatUiStatus.Pending ? e.Status : (source.Status ?? ChatUiStatus.Running),
            ToolName = !string.IsNullOrEmpty(source.ToolName) ? source.ToolName : e.BlockID,
            Title = source.Title,
            DetailText = source.DetailText,
            OutputText = source.OutputText,
            ErrorText = source.ErrorText,
            DurationMs = source.DurationMs
        };
        return UpsertBlock(t, new ChatUiBlockUpsertEvent { MessageID = e.MessageID, BlockID = e.BlockID, Block = block });
    }

    private static ChatUiTimeline UpdateCollapseState(ChatUiTimeline t, ChatUiInteractionCollapseEvent e)
    {
        var existing = t.Presentation?.CollapsedBlocks ?? new Dictionary<string, bool>();
        var collapsed = new Dictionary<string, bool>(existing) { [e.BlockID] = e.Collapsed };
        var presentation = MergePresentation(t.Presentation, new ChatUiPresentation { CollapsedBlocks = collapsed });
        return t.With(presentation);
    }

    private static ChatUiTimeline UpdateSelection(ChatUiTimeline t, ChatUiSelectionUpdateEvent e)
    {
        var style = t.Presentation?.Style;
        var presentation = MergePresentation(t.Presentation, new ChatUiPresentation
        {
            Style = e.Selection is null ? style : new Dictionary<string, object?> { ["selection"] = e.Selection }
        });
        return t.With(presentation);
    }

    private static ChatUiPresentation MergePresentation(ChatUiPresentation? current, ChatUiPresentation update)
    {
        return new ChatUiPresentation
        {
            Theme = update.Theme ?? current?.Theme,
            MarkdownProfile = update.MarkdownProfile ?? current?.MarkdownProfile,
            CodeTheme = update.CodeTheme ?? current?.CodeTheme,
            CollapsedBlocks = update.CollapsedBlocks ?? current?.CollapsedBlocks,
            ExpandedBlocks = update.ExpandedBlocks ?? current?.ExpandedBlocks,
            Style = update.Style ?? current?.Style,
            DisplayWindow = update.DisplayWindow ?? current?.DisplayWindow,
            BottomSlackPx = update.BottomSlackPx ?? current?.BottomSlackPx,
            BottomSafeAreaInsetPx = update.BottomSafeAreaInsetPx ?? current?.BottomSafeAreaInsetPx,
            MessageActions = update.MessageActions ?? current?.MessageActions,
            AssistantModelOptions = update.AssistantModelOptions ?? current?.AssistantModelOptions,
            IsConversationGenerating = update.IsConversationGenerating ?? current?.IsConversationGenerating
        };
    }
}
