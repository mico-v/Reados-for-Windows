using ReadOS.App.Models.ChatUi;

namespace ReadOS.App.Tests.Models.ChatUi;

// Guards contract-level invariants that the renderers and selectors depend on:
// every ChatUiBlockType has exactly one concrete block class reporting it, and
// every ChatUiRenderOperationKind is represented by an operation type. Catches
// accidental gaps when a new block type or operation is added upstream.

public sealed class ChatUiContractsTests
{
    public static IEnumerable<object[]> BlockTypes()
    {
        foreach (ChatUiBlockType type in Enum.GetValues(typeof(ChatUiBlockType)))
        {
            yield return new object[] { type };
        }
    }

    [Theory]
    [MemberData(nameof(BlockTypes))]
    public void Every_block_type_maps_to_a_concrete_block_with_matching_type(ChatUiBlockType type)
    {
        ChatUiBlock? block = type switch
        {
            ChatUiBlockType.Markdown => new ChatUiMarkdownBlock(),
            ChatUiBlockType.ToolCall => new ChatUiToolCallBlock(),
            ChatUiBlockType.ToolGroup => new ChatUiToolGroupBlock(),
            ChatUiBlockType.Processing => new ChatUiProcessingBlock(),
            ChatUiBlockType.Reasoning => new ChatUiReasoningBlock(),
            ChatUiBlockType.Progress => new ChatUiProgressBlock(),
            ChatUiBlockType.VideoProgress => new ChatUiVideoProgressBlock(),
            ChatUiBlockType.ProposedPlan => new ChatUiProposedPlanBlock(),
            ChatUiBlockType.Notice => new ChatUiNoticeBlock(),
            ChatUiBlockType.Attachment => new ChatUiAttachmentBlock(),
            ChatUiBlockType.Image => new ChatUiImageBlock(),
            ChatUiBlockType.SearchResults => new ChatUiSearchResultsBlock(),
            ChatUiBlockType.SearchProgress => new ChatUiSearchProgressBlock(),
            ChatUiBlockType.Sources => new ChatUiSourcesBlock(),
            ChatUiBlockType.TextSelection => new ChatUiTextSelectionBlock(),
            ChatUiBlockType.Footer => new ChatUiFooterBlock(),
            _ => null
        };

        Assert.NotNull(block);
        Assert.Equal(type, block!.Type);
    }

    [Fact]
    public void There_are_sixteen_block_types()
    {
        Assert.Equal(16, Enum.GetValues(typeof(ChatUiBlockType)).Length);
    }

    [Fact]
    public void Every_render_operation_kind_has_a_concrete_operation()
    {
        Assert.IsType<ChatUiFullRenderOperation>(OperationFor(ChatUiRenderOperationKind.FullRender));
        Assert.IsType<ChatUiPayloadPatchOperation>(OperationFor(ChatUiRenderOperationKind.PayloadPatch));
        Assert.IsType<ChatUiDirectStreamingUpdateOperation>(OperationFor(ChatUiRenderOperationKind.DirectStreamingUpdate));
        Assert.IsType<ChatUiPresentationOnlyUpdateOperation>(OperationFor(ChatUiRenderOperationKind.PresentationOnlyUpdate));
        Assert.IsType<ChatUiScrollSyncOperation>(OperationFor(ChatUiRenderOperationKind.ScrollSync));
    }

    [Fact]
    public void Schema_constants_are_stable()
    {
        Assert.Equal("msp.chat-ui.timeline.v1", ChatUiTimeline.Schema);
        Assert.Equal("msp.chat-ui.event.v1", ChatUiRuntimeEvent.EventSchema);
    }

    private static ChatUiRenderOperation OperationFor(ChatUiRenderOperationKind kind) => kind switch
    {
        ChatUiRenderOperationKind.FullRender => new ChatUiFullRenderOperation(),
        ChatUiRenderOperationKind.PayloadPatch => new ChatUiPayloadPatchOperation(),
        ChatUiRenderOperationKind.DirectStreamingUpdate => new ChatUiDirectStreamingUpdateOperation(),
        ChatUiRenderOperationKind.PresentationOnlyUpdate => new ChatUiPresentationOnlyUpdateOperation(),
        ChatUiRenderOperationKind.ScrollSync => new ChatUiScrollSyncOperation(),
        _ => throw new ArgumentOutOfRangeException(nameof(kind))
    };
}
