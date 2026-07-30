using System.Collections.Immutable;

namespace ReadOS.App.Models.ChatUi;

// Platform-neutral render operation planner.
//
// Mirrors Projection/runtime/render-planner.js. Given a previous timeline
// snapshot and the next one, it decides the cheapest render operation:
//
//   fullRender            — first render, or structural change beyond a patch
//   scrollSync            — nothing changed
//   presentationOnly     — only theme/collapse/selection/window changed
//   directStreamingUpdate — a single assistant markdown block appended text
//   payloadPatch          — block/message added/removed/patched
//
// The planner never inspects renderer payload shapes. It compares canonical
// timeline payloads (messages + blocks) and presentation state only. Comparisons
// are structural rather than JSON-based so polymorphic block subtypes compare
// their full fields, not just the abstract base.

public static class ChatUiRenderPlanner
{
    public static ChatUiRenderOperation Plan(ChatUiTimeline? previous, ChatUiTimeline next)
    {
        if (previous is null)
        {
            return new ChatUiFullRenderOperation { Payload = next, Presentation = next.Presentation };
        }

        if (SamePayload(previous, next) && SamePresentation(previous, next))
        {
            return new ChatUiScrollSyncOperation();
        }

        if (SamePayload(previous, next) && !SamePresentation(previous, next))
        {
            return new ChatUiPresentationOnlyUpdateOperation { Presentation = next.Presentation! };
        }

        var streamingUpdate = DirectStreamingUpdate(previous, next);
        if (streamingUpdate is not null)
        {
            return new ChatUiDirectStreamingUpdateOperation { Update = streamingUpdate, Presentation = next.Presentation };
        }

        var patch = ChatUiPayloadDiff.BuildPatch(previous, next);
        if (ChatUiPayloadDiff.HasChanges(patch))
        {
            return new ChatUiPayloadPatchOperation { Patch = patch, Presentation = next.Presentation };
        }

        return new ChatUiScrollSyncOperation();
    }

    private static bool SamePayload(ChatUiTimeline a, ChatUiTimeline b)
    {
        if (a.Messages.Count != b.Messages.Count) return false;
        for (var i = 0; i < a.Messages.Count; i++)
        {
            if (!MessageEqual(a.Messages[i], b.Messages[i])) return false;
        }
        return true;
    }

    private static bool SamePresentation(ChatUiTimeline a, ChatUiTimeline b)
    {
        return PresentationEqual(a.Presentation, b.Presentation);
    }

    private static bool MessageEqual(ChatUiMessage a, ChatUiMessage b)
    {
        if (a.Id != b.Id) return false;
        if (a.Role != b.Role) return false;
        if (a.Blocks.Count != b.Blocks.Count) return false;
        for (var i = 0; i < a.Blocks.Count; i++)
        {
            if (!BlockEqual(a.Blocks[i], b.Blocks[i])) return false;
        }
        return true;
    }

    private static object? DirectStreamingUpdate(ChatUiTimeline previous, ChatUiTimeline next)
    {
        var prevMessages = previous.Messages;
        var nextMessages = next.Messages;
        if (prevMessages.Count != nextMessages.Count) return null;

        ChatUiMessage? changedBefore = null;
        ChatUiMessage? changedAfter = null;
        ChatUiBlock? changedBeforeBlock = null;
        ChatUiBlock? changedAfterBlock = null;
        int changedIndex = -1;

        for (var i = 0; i < nextMessages.Count; i++)
        {
            var before = prevMessages[i];
            var after = nextMessages[i];
            if (ChatUiIdentity.MessageKey(before, i) != ChatUiIdentity.MessageKey(after, i)) return null;
            if (!ComparableEqual(before, after)) return null;

            var beforeCatalog = Catalog(before);
            foreach (var afterBlock in after.Blocks)
            {
                if (!beforeCatalog.TryGetValue(afterBlock.Id, out var beforeBlock)) return null;
                if (!BlockEqual(beforeBlock, afterBlock))
                {
                    if (changedBefore is not null) return null;
                    changedBefore = before;
                    changedAfter = after;
                    changedBeforeBlock = beforeBlock;
                    changedAfterBlock = afterBlock;
                    changedIndex = i;
                }
            }
        }

        if (changedAfter is null || changedAfter.Role != ChatUiRole.Assistant) return null;
        if (changedBefore!.IsStreaming != true && changedAfter.IsStreaming != true) return null;
        if (changedBeforeBlock!.Id != changedAfterBlock!.Id) return null;
        if (changedAfterBlock is not ChatUiMarkdownBlock afterMarkdown) return null;
        var beforeMarkdown = (ChatUiMarkdownBlock)changedBeforeBlock;
        if (!afterMarkdown.Text.StartsWith(beforeMarkdown.Text, StringComparison.Ordinal)) return null;

        return new
        {
            updates = new[]
            {
                new
                {
                    kind = "main_text",
                    messageKey = ChatUiIdentity.MessageKey(changedAfter, changedIndex),
                    blockID = afterMarkdown.Id,
                    block = afterMarkdown,
                    messageState = changedAfter,
                    syncMessageChrome = true
                }
            }
        };
    }

    private static bool ComparableEqual(ChatUiMessage a, ChatUiMessage b)
    {
        // status and streaming are excluded — only identity + block count matter
        // for the streaming-update fast path. Block bodies are compared below.
        if (a.Id != b.Id) return false;
        if (a.Role != b.Role) return false;
        if (a.Blocks.Count != b.Blocks.Count) return false;
        return true;
    }

    private static bool PresentationEqual(ChatUiPresentation? a, ChatUiPresentation? b)
    {
        if (a is null && b is null) return true;
        if (a is null || b is null) return false;
        return a.Theme == b.Theme &&
               a.MarkdownProfile == b.MarkdownProfile &&
               a.CodeTheme == b.CodeTheme &&
               a.BottomSlackPx == b.BottomSlackPx &&
               a.BottomSafeAreaInsetPx == b.BottomSafeAreaInsetPx &&
               a.IsConversationGenerating == b.IsConversationGenerating &&
               BoolMapEqual(a.CollapsedBlocks, b.CollapsedBlocks) &&
               BoolMapEqual(a.ExpandedBlocks, b.ExpandedBlocks);
    }

    private static bool BoolMapEqual(IReadOnlyDictionary<string, bool>? a, IReadOnlyDictionary<string, bool>? b)
    {
        if (a is null && b is null) return true;
        if (a is null || b is null) return false;
        if (a.Count != b.Count) return false;
        foreach (var (key, value) in a)
        {
            if (!b.TryGetValue(key, out var other) || other != value) return false;
        }
        return true;
    }

    private static bool BlockEqual(ChatUiBlock a, ChatUiBlock b)
    {
        if (a.Id != b.Id) return false;
        if (a.Type != b.Type) return false;
        return (a, b) switch
        {
            (ChatUiMarkdownBlock ma, ChatUiMarkdownBlock mb) =>
                ma.Text == mb.Text && ma.Streaming == mb.Streaming && ma.Status == mb.Status,
            (ChatUiToolCallBlock ta, ChatUiToolCallBlock tb) =>
                ta.ToolName == tb.ToolName && ta.Title == tb.Title && ta.DetailText == tb.DetailText &&
                ta.OutputText == tb.OutputText && ta.ErrorText == tb.ErrorText && ta.DurationMs == tb.DurationMs && ta.Status == tb.Status,
            (ChatUiProgressBlock pa, ChatUiProgressBlock pb) =>
                pa.Title == pb.Title && pa.DetailText == pb.DetailText && pa.Progress == pb.Progress && pa.Status == pb.Status,
            (ChatUiNoticeBlock na, ChatUiNoticeBlock nb) =>
                na.Text == nb.Text && na.Status == nb.Status,
            (ChatUiAttachmentBlock aa, ChatUiAttachmentBlock ab) =>
                aa.Attachments.Count == ab.Attachments.Count,
            (ChatUiFooterBlock fa, ChatUiFooterBlock fb) =>
                fa.Text == fb.Text,
            _ => a.Status == b.Status
        };
    }

    internal static bool BlocksEqual(ChatUiBlock a, ChatUiBlock b) => BlockEqual(a, b);

    private static Dictionary<string, ChatUiBlock> Catalog(ChatUiMessage m)
    {
        var dict = new Dictionary<string, ChatUiBlock>(m.Blocks.Count);
        foreach (var b in m.Blocks)
        {
            dict[b.Id] = b;
        }
        return dict;
    }
}

// Simplified payload diff.
//
// The reference payload-diff.js builds a structured patch (blockCatalog, message
// additions/removals, block patches). ReadOS only needs to know *whether* a
// patch exists so the planner can choose between payloadPatch and scrollSync.
// A future slice can expand this into a structured patch the XAML view consumes
// for fine-grained virtualization; for now HasChanges is the contract.

public static class ChatUiPayloadDiff
{
    public sealed class Patch
    {
        public bool HasMessageChanges { get; init; }
        public bool HasBlockChanges { get; init; }
    }

    public static Patch BuildPatch(ChatUiTimeline previous, ChatUiTimeline next)
    {
        var hasMessageChanges = previous.Messages.Count != next.Messages.Count
            || previous.Messages.Zip(next.Messages).Any(pair => pair.First.Id != pair.Second.Id);

        var hasBlockChanges = false;
        if (!hasMessageChanges)
        {
            for (var i = 0; i < next.Messages.Count; i++)
            {
                var a = previous.Messages[i];
                var b = next.Messages[i];
                if (a.Blocks.Count != b.Blocks.Count) { hasBlockChanges = true; break; }
                for (var j = 0; j < b.Blocks.Count; j++)
                {
                    if (a.Blocks[j].Id != b.Blocks[j].Id || !ChatUiRenderPlanner.BlocksEqual(a.Blocks[j], b.Blocks[j]))
                    {
                        hasBlockChanges = true;
                        break;
                    }
                }
                if (hasBlockChanges) break;
            }
        }

        return new Patch { HasMessageChanges = hasMessageChanges, HasBlockChanges = hasBlockChanges };
    }

    public static bool HasChanges(Patch patch) => patch.HasMessageChanges || patch.HasBlockChanges;
}
