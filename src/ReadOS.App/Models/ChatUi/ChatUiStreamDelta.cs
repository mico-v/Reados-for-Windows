namespace ReadOS.App.Models.ChatUi;

// Text delta helpers for live assistant output.
//
// Mirrors Projection/runtime/stream-delta.js: appends a stream delta to a
// markdown block's text and resolves terminal status. A streaming block keeps
// its streaming flag until a terminal status arrives. Non-markdown blocks are
// returned unchanged because the Default renderer only streams markdown text;
// tool/progress blocks use their own lifecycle events.

internal static class ChatUiStreamDelta
{
    public static ChatUiBlock AppendText(ChatUiBlock block, ChatUiStreamDeltaEvent delta)
    {
        if (block is ChatUiMarkdownBlock markdown)
        {
            var streaming = delta.Status switch
            {
                ChatUiStatus.Success or ChatUiStatus.Failed or ChatUiStatus.Cancelled => false,
                _ => markdown.Streaming ?? true
            };

            return new ChatUiMarkdownBlock
            {
                Id = markdown.Id,
                Status = delta.Status ?? markdown.Status,
                Text = markdown.Text + delta.TextDelta,
                Streaming = streaming
            };
        }

        return block;
    }
}
