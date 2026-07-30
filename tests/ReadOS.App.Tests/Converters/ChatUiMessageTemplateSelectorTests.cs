using ReadOS.App.Models.ChatUi;

namespace ReadOS.App.Tests.Converters;

// Guards the routing decision used by ChatUiMessageTemplateSelector.
//
// The selector itself returns WinUI DataTemplate instances, which cannot be
// instantiated in this logic-only xUnit suite (WinRT types require a UI-thread
// COM context). The decision input — ChatUiMessage.IsUserBubble /
// IsAssistantOpen — is plain C# and is what maps a role to the gray bubble
// (user) vs open markdown (assistant/system/tool) container, per
// renderer.manifest.json (showsUserBubbleBackground / showsAssistantBubbleBackground).
public sealed class ChatUiMessageTemplateSelectorTests
{
    private static ChatUiMessage Message(ChatUiRole role) => new()
    {
        Id = "m",
        Role = role
    };

    [Fact]
    public void User_role_is_bubble_and_not_open()
    {
        var message = Message(ChatUiRole.User);

        Assert.True(message.IsUserBubble);
        Assert.False(message.IsAssistantOpen);
    }

    [Theory]
    [InlineData(ChatUiRole.Assistant)]
    [InlineData(ChatUiRole.System)]
    [InlineData(ChatUiRole.Tool)]
    public void Non_user_roles_are_open_and_not_bubble(ChatUiRole role)
    {
        var message = Message(role);

        Assert.False(message.IsUserBubble);
        Assert.True(message.IsAssistantOpen);
    }
}
