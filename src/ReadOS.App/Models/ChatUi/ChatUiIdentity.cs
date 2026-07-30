namespace ReadOS.App.Models.ChatUi;

// Stable identity keys for messages and blocks.
//
// Projection uses these keys to decide whether a message moved, whether a
// streaming update is a simple append, or whether a full re-render is required.
// Keys are stable across revisions so the render planner can compare payloads
// without depending on renderer-specific payload shapes.

internal static class ChatUiIdentity
{
    public static string MessageKey(ChatUiMessage? message, int index)
    {
        if (message is null) return $"_:{index}";
        return string.IsNullOrEmpty(message.Id) ? $"_:{index}" : message.Id;
    }

    public static string BlockKey(ChatUiBlock? block, int index, string messageId)
    {
        if (block is null) return $"{messageId}:_:{index}";
        return string.IsNullOrEmpty(block.Id) ? $"{messageId}:_:{index}" : block.Id;
    }
}
