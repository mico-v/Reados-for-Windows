namespace ReadOS.App.Models.ChatUi;

// MSP Chat UI canonical contract types.
//
// These mirror the product Web package schemas under src/ReadOS.Web/MSPChatUI.
// (msp.chat-ui.timeline.v1 and msp.chat-ui.event.v1). They are platform-neutral
// payload models: they must not reference WinUI, DOM, WebView, or any specific
// renderer. Renderers consume Timeline objects; hosts forward RuntimeEvents.
//
// The contract layer is the only public model. Projection, render planning, and
// renderer adaptation build on top of it but do not leak private payload shapes
// back into these types.

public enum ChatUiRole
{
    User,
    Assistant,
    System,
    Tool
}

public enum ChatUiStatus
{
    Pending,
    Running,
    Success,
    Failed,
    Cancelled
}

public enum ChatUiBlockType
{
    Markdown,
    ToolCall,
    ToolGroup,
    Processing,
    Reasoning,
    Progress,
    VideoProgress,
    ProposedPlan,
    Notice,
    Attachment,
    Image,
    SearchResults,
    SearchProgress,
    Sources,
    TextSelection,
    Footer
}

public enum ChatUiRenderOperationKind
{
    FullRender,
    PayloadPatch,
    DirectStreamingUpdate,
    PresentationOnlyUpdate,
    ScrollSync
}

public sealed class ChatUiTimeline
{
    public const string Schema = "msp.chat-ui.timeline.v1";

    public string? SchemaVersion { get; init; } = Schema;
    public string? Id { get; init; }
    public string? Title { get; init; }
    public int Revision { get; init; }
    public ChatUiPresentation? Presentation { get; init; }
    public IReadOnlyList<ChatUiMessage> Messages { get; init; } = Array.Empty<ChatUiMessage>();
}

public sealed class ChatUiPresentation
{
    public string? Theme { get; init; }
    public string? MarkdownProfile { get; init; }
    public string? CodeTheme { get; init; }
    public IReadOnlyDictionary<string, bool>? CollapsedBlocks { get; init; }
    public IReadOnlyDictionary<string, bool>? ExpandedBlocks { get; init; }
    public IReadOnlyDictionary<string, object?>? Style { get; init; }
    public ChatUiDisplayWindow? DisplayWindow { get; init; }
    public double? BottomSlackPx { get; init; }
    public double? BottomSafeAreaInsetPx { get; init; }
    public ChatUiMessageActions? MessageActions { get; init; }
    public IReadOnlyList<object>? AssistantModelOptions { get; init; }
    public bool? IsConversationGenerating { get; init; }
}

public sealed class ChatUiDisplayWindow
{
    public int StartIndex { get; init; }
    public int DisplayCount { get; init; }
}

public sealed class ChatUiMessageActions
{
    public bool? Enabled { get; init; }
    public string? AssistantPlacement { get; init; }
    public IReadOnlyList<string>? Assistant { get; init; }
    public IReadOnlyList<string>? User { get; init; }
}

public sealed class ChatUiMessage
{
    public string Id { get; init; } = string.Empty;
    public ChatUiRole Role { get; init; }
    public ChatUiStatus? Status { get; init; }
    public string? ModelName { get; init; }
    public string? CreatedAt { get; init; }
    public string? UpdatedAt { get; init; }
    public string? TimeText { get; init; }
    public double? CompletedGoalDurationMs { get; init; }
    public IReadOnlyDictionary<string, object?>? MemoryCitation { get; init; }
    public bool? HasRenderPatches { get; init; }
    public bool? HasEnabledRenderPatches { get; init; }
    public IReadOnlyList<ChatUiBlock> Blocks { get; init; } = Array.Empty<ChatUiBlock>();
    public bool? IsStreaming { get; init; }

    // Convenience flags for XAML bindings. The Default renderer manifest
    // distinguishes "user" messages (gray bubble) from "assistant/system/tool"
    // messages (open markdown, no bubble). Exposing these as computed properties
    // keeps the XAML declarative and avoids an in-line converter.
    public bool IsUserBubble => Role == ChatUiRole.User;
    public bool IsAssistantOpen => Role != ChatUiRole.User;
}

public abstract class ChatUiBlock
{
    public string Id { get; init; } = string.Empty;
    public abstract ChatUiBlockType Type { get; }
    public ChatUiStatus? Status { get; init; }
}

public sealed class ChatUiMarkdownBlock : ChatUiBlock
{
    public override ChatUiBlockType Type => ChatUiBlockType.Markdown;
    public string Text { get; init; } = string.Empty;
    public bool? Streaming { get; init; }
}

public sealed class ChatUiToolCallBlock : ChatUiBlock
{
    public override ChatUiBlockType Type => ChatUiBlockType.ToolCall;
    public string ToolName { get; init; } = string.Empty;
    public string? Title { get; init; }
    public string? DetailText { get; init; }
    public double? DurationMs { get; init; }
    public string? OutputText { get; init; }
    public string? ErrorText { get; init; }
}

public sealed class ChatUiToolGroupBlock : ChatUiBlock
{
    public override ChatUiBlockType Type => ChatUiBlockType.ToolGroup;
    public string? Title { get; init; }
    public IReadOnlyList<ChatUiToolCallBlock> ToolCalls { get; init; } = Array.Empty<ChatUiToolCallBlock>();
}

public enum ChatUiActivityItemType
{
    Tool,
    WebSearch,
    Progress,
    MainText,
    VideoProgress,
    OperationSummary,
    Subagent
}

public sealed class ChatUiActivityItem
{
    public string? Id { get; init; }
    public ChatUiActivityItemType? Type { get; init; }
    public ChatUiStatus? Status { get; init; }
    public string? Text { get; init; }
    public string? Title { get; init; }
    public string? DetailText { get; init; }
    public string? ToolName { get; init; }
    public string? AgentName { get; init; }
    public string? ThreadID { get; init; }
    public double? DurationMs { get; init; }
    public double? Progress { get; init; }
    public object? Result { get; init; }
    public object? Arguments { get; init; }
    public IReadOnlyList<object>? PreviewItems { get; init; }
    public IReadOnlyList<ChatUiActivityItem>? ChildItems { get; init; }
    public IReadOnlyList<string>? SearchQueries { get; init; }
    public IReadOnlyList<object>? SearchReferences { get; init; }
    public IReadOnlyList<object>? WebSearchActions { get; init; }
}

public sealed class ChatUiProcessingBlock : ChatUiBlock
{
    public override ChatUiBlockType Type => ChatUiBlockType.Processing;
    public string? Title { get; init; }
    public bool? Active { get; init; }
    public string? GroupID { get; init; }
    public string? ChromeRole { get; init; }
    public double? StartedAtMs { get; init; }
    public double? DurationMs { get; init; }
    public IReadOnlyList<ChatUiActivityItem> Items { get; init; } = Array.Empty<ChatUiActivityItem>();
}

public sealed class ChatUiReasoningBlock : ChatUiBlock
{
    public override ChatUiBlockType Type => ChatUiBlockType.Reasoning;
    public string Text { get; init; } = string.Empty;
}

public sealed class ChatUiProgressBlock : ChatUiBlock
{
    public override ChatUiBlockType Type => ChatUiBlockType.Progress;
    public string Title { get; init; } = string.Empty;
    public string? DetailText { get; init; }
    public double? Progress { get; init; }
}

public sealed class ChatUiVideoProgressBlock : ChatUiBlock
{
    public override ChatUiBlockType Type => ChatUiBlockType.VideoProgress;
    public string? Title { get; init; }
    public string? DetailText { get; init; }
    public double? Progress { get; init; }
    public IReadOnlyList<object>? Items { get; init; }
}

public sealed class ChatUiProposedPlanBlock : ChatUiBlock
{
    public override ChatUiBlockType Type => ChatUiBlockType.ProposedPlan;
    public string Text { get; init; } = string.Empty;
    public string? PhaseTitle { get; init; }
}

public sealed class ChatUiAttachmentBlock : ChatUiBlock
{
    public override ChatUiBlockType Type => ChatUiBlockType.Attachment;
    public IReadOnlyList<object> Attachments { get; init; } = Array.Empty<object>();
}

public sealed class ChatUiImageBlock : ChatUiBlock
{
    public override ChatUiBlockType Type => ChatUiBlockType.Image;
    public IReadOnlyList<object> Images { get; init; } = Array.Empty<object>();
}

public sealed class ChatUiNoticeBlock : ChatUiBlock
{
    public override ChatUiBlockType Type => ChatUiBlockType.Notice;
    public string Text { get; init; } = string.Empty;
}

public sealed class ChatUiSearchResultsBlock : ChatUiBlock
{
    public override ChatUiBlockType Type => ChatUiBlockType.SearchResults;
    public IReadOnlyList<string>? SearchQueries { get; init; }
    public IReadOnlyList<object>? SearchReferences { get; init; }
    public IReadOnlyList<object>? WebSearchActions { get; init; }
}

public sealed class ChatUiSearchProgressBlock : ChatUiBlock
{
    public override ChatUiBlockType Type => ChatUiBlockType.SearchProgress;
    public string? Title { get; init; }
    public string? DetailText { get; init; }
    public IReadOnlyList<string>? SearchQueries { get; init; }
    public IReadOnlyList<object>? WebSearchActions { get; init; }
}

public sealed class ChatUiSourcesBlock : ChatUiBlock
{
    public override ChatUiBlockType Type => ChatUiBlockType.Sources;
    public IReadOnlyList<object>? Sources { get; init; }
    public IReadOnlyList<object>? References { get; init; }
}

public sealed class ChatUiTextSelectionBlock : ChatUiBlock
{
    public override ChatUiBlockType Type => ChatUiBlockType.TextSelection;
    public IReadOnlyDictionary<string, object?>? TextSelection { get; init; }
}

public sealed class ChatUiFooterBlock : ChatUiBlock
{
    public override ChatUiBlockType Type => ChatUiBlockType.Footer;
    public string Text { get; init; } = string.Empty;
}

public abstract class ChatUiRuntimeEvent
{
    public const string EventSchema = "msp.chat-ui.event.v1";
    public string? SchemaVersion { get; init; } = EventSchema;
    public string? Id { get; init; }
    public abstract string Type { get; }
    public string? Timestamp { get; init; }
}

public sealed class ChatUiTimelineReplaceEvent : ChatUiRuntimeEvent
{
    public override string Type => "timeline.replace";
    public ChatUiTimeline Timeline { get; init; } = null!;
}

public sealed class ChatUiPresentationUpdateEvent : ChatUiRuntimeEvent
{
    public override string Type => "presentation.update";
    public ChatUiPresentation Presentation { get; init; } = null!;
}

public sealed class ChatUiMessageUpsertEvent : ChatUiRuntimeEvent
{
    public override string Type => "message.upsert";
    public ChatUiMessage Message { get; init; } = null!;
    public string? MessageID { get; init; }
}

public sealed class ChatUiMessageRemoveEvent : ChatUiRuntimeEvent
{
    public override string Type => "message.remove";
    public string MessageID { get; init; } = string.Empty;
}

public sealed class ChatUiMessageStatusEvent : ChatUiRuntimeEvent
{
    public override string Type => "message.status";
    public string MessageID { get; init; } = string.Empty;
    public ChatUiStatus Status { get; init; }
}

public sealed class ChatUiMessagePatchEvent : ChatUiRuntimeEvent
{
    public override string Type => "message.patch";
    public string MessageID { get; init; } = string.Empty;
    public IReadOnlyDictionary<string, object?> Patch { get; init; } = new Dictionary<string, object?>();
}

public sealed class ChatUiBlockUpsertEvent : ChatUiRuntimeEvent
{
    public override string Type => "block.upsert";
    public string MessageID { get; init; } = string.Empty;
    public ChatUiBlock Block { get; init; } = null!;
    public string? BlockID { get; init; }
}

public sealed class ChatUiBlockRemoveEvent : ChatUiRuntimeEvent
{
    public override string Type => "block.remove";
    public string MessageID { get; init; } = string.Empty;
    public string BlockID { get; init; } = string.Empty;
}

public sealed class ChatUiBlockStatusEvent : ChatUiRuntimeEvent
{
    public override string Type => "block.status";
    public string MessageID { get; init; } = string.Empty;
    public string BlockID { get; init; } = string.Empty;
    public ChatUiStatus Status { get; init; }
}

public sealed class ChatUiBlockPatchEvent : ChatUiRuntimeEvent
{
    public override string Type => "block.patch";
    public string MessageID { get; init; } = string.Empty;
    public string BlockID { get; init; } = string.Empty;
    public IReadOnlyDictionary<string, object?> Patch { get; init; } = new Dictionary<string, object?>();
}

public sealed class ChatUiStreamDeltaEvent : ChatUiRuntimeEvent
{
    public override string Type => "stream.delta";
    public string MessageID { get; init; } = string.Empty;
    public string BlockID { get; init; } = string.Empty;
    public string TextDelta { get; init; } = string.Empty;
    public ChatUiStatus? Status { get; init; }
}

public sealed class ChatUiToolLifecycleEvent : ChatUiRuntimeEvent
{
    public override string Type => "tool.lifecycle";
    public string MessageID { get; init; } = string.Empty;
    public string BlockID { get; init; } = string.Empty;
    public ChatUiStatus Status { get; init; }
    public ChatUiToolCallBlock ToolCall { get; init; } = null!;
}

public sealed class ChatUiInteractionCollapseEvent : ChatUiRuntimeEvent
{
    public override string Type => "interaction.collapse";
    public string MessageID { get; init; } = string.Empty;
    public string BlockID { get; init; } = string.Empty;
    public bool Collapsed { get; init; }
}

public sealed class ChatUiSelectionUpdateEvent : ChatUiRuntimeEvent
{
    public override string Type => "selection.update";
    public IReadOnlyDictionary<string, object?>? Selection { get; init; }
}

public sealed class ChatUiScrollSyncEvent : ChatUiRuntimeEvent
{
    public override string Type => "scroll.sync";
}

public abstract class ChatUiRenderOperation
{
    public abstract ChatUiRenderOperationKind Kind { get; }
}

public sealed class ChatUiFullRenderOperation : ChatUiRenderOperation
{
    public override ChatUiRenderOperationKind Kind => ChatUiRenderOperationKind.FullRender;
    public object Payload { get; init; } = null!;
    public ChatUiPresentation? Presentation { get; init; }
}

public sealed class ChatUiPayloadPatchOperation : ChatUiRenderOperation
{
    public override ChatUiRenderOperationKind Kind => ChatUiRenderOperationKind.PayloadPatch;
    public object Patch { get; init; } = null!;
    public ChatUiPresentation? Presentation { get; init; }
}

public sealed class ChatUiDirectStreamingUpdateOperation : ChatUiRenderOperation
{
    public override ChatUiRenderOperationKind Kind => ChatUiRenderOperationKind.DirectStreamingUpdate;
    public object Update { get; init; } = null!;
    public ChatUiPresentation? Presentation { get; init; }
}

public sealed class ChatUiPresentationOnlyUpdateOperation : ChatUiRenderOperation
{
    public override ChatUiRenderOperationKind Kind => ChatUiRenderOperationKind.PresentationOnlyUpdate;
    public ChatUiPresentation Presentation { get; init; } = null!;
}

public sealed class ChatUiScrollSyncOperation : ChatUiRenderOperation
{
    public override ChatUiRenderOperationKind Kind => ChatUiRenderOperationKind.ScrollSync;
}
