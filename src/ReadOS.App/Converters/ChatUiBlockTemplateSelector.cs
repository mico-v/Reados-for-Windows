using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using ReadOS.App.Models.ChatUi;

namespace ReadOS.App.Converters;

// Selects a XAML data template for a canonical MSP Chat UI block.
//
// The Default renderer manifest (Renderers/Default/renderer.manifest.json)
// enumerates 16 block types. This selector routes each block type to a template
// declared in ChatTimelineView.xaml. Unknown block types fall back to the
// markdown template so newly added contract blocks degrade gracefully instead
// of rendering blank.

public sealed class ChatUiBlockTemplateSelector : DataTemplateSelector
{
    public DataTemplate? MarkdownTemplate { get; set; }
    public DataTemplate? ToolCallTemplate { get; set; }
    public DataTemplate? ToolGroupTemplate { get; set; }
    public DataTemplate? ProcessingTemplate { get; set; }
    public DataTemplate? ReasoningTemplate { get; set; }
    public DataTemplate? ProgressTemplate { get; set; }
    public DataTemplate? VideoProgressTemplate { get; set; }
    public DataTemplate? ProposedPlanTemplate { get; set; }
    public DataTemplate? NoticeTemplate { get; set; }
    public DataTemplate? AttachmentTemplate { get; set; }
    public DataTemplate? ImageTemplate { get; set; }
    public DataTemplate? SearchResultsTemplate { get; set; }
    public DataTemplate? SearchProgressTemplate { get; set; }
    public DataTemplate? SourcesTemplate { get; set; }
    public DataTemplate? TextSelectionTemplate { get; set; }
    public DataTemplate? FooterTemplate { get; set; }

    protected override DataTemplate? SelectTemplateCore(object item, DependencyObject container)
    {
        return item switch
        {
            ChatUiMarkdownBlock => MarkdownTemplate,
            ChatUiToolCallBlock => ToolCallTemplate,
            ChatUiToolGroupBlock => ToolGroupTemplate,
            ChatUiProcessingBlock => ProcessingTemplate,
            ChatUiReasoningBlock => ReasoningTemplate,
            ChatUiProgressBlock => ProgressTemplate,
            ChatUiVideoProgressBlock => VideoProgressTemplate,
            ChatUiProposedPlanBlock => ProposedPlanTemplate,
            ChatUiNoticeBlock => NoticeTemplate,
            ChatUiAttachmentBlock => AttachmentTemplate,
            ChatUiImageBlock => ImageTemplate,
            ChatUiSearchResultsBlock => SearchResultsTemplate,
            ChatUiSearchProgressBlock => SearchProgressTemplate,
            ChatUiSourcesBlock => SourcesTemplate,
            ChatUiTextSelectionBlock => TextSelectionTemplate,
            ChatUiFooterBlock => FooterTemplate,
            _ => MarkdownTemplate
        };
    }
}
