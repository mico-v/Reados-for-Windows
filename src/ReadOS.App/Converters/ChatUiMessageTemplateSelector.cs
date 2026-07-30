using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using ReadOS.App.Models.ChatUi;

namespace ReadOS.App.Converters;

// Selects the message-level container template per the Default renderer
// manifest (Renderers/Default/renderer.manifest.json):
//
//   showsUserBubbleBackground: true      -> user messages render inside a
//                                           gray bubble (UserMessageTemplate)
//   showsAssistantBubbleBackground: false -> assistant/system/tool messages
//                                           render as open markdown, no bubble
//                                           (AssistantMessageTemplate)
//
// This is the safe alternative to the dual-Border Grid that previously
// triggered a WMC9999 internal compiler error: only ONE template is ever
// selected per message, so there is never a nested pair of mutually-visible
// Borders inside the ItemsRepeater DataTemplate.
public class ChatUiMessageTemplateSelector : DataTemplateSelector
{
    public DataTemplate? UserMessageTemplate { get; set; }
    public DataTemplate? AssistantMessageTemplate { get; set; }

    protected override DataTemplate? SelectTemplateCore(object item, DependencyObject container)
    {
        if (item is ChatUiMessage message)
        {
            return message.IsUserBubble ? UserMessageTemplate : AssistantMessageTemplate;
        }

        return AssistantMessageTemplate;
    }
}
