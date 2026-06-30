using ReadOS.App.Models;

namespace ReadOS.App.Services;

public interface IAiChatService
{
    Task<string> SendAsync(
        WorkspaceSettings settings,
        LibraryItem? document,
        IEnumerable<ChatMessage> history,
        string userPrompt,
        IEnumerable<ChatAttachment> attachments,
        Func<ChatAttachment, Task<string>> attachmentTextProvider,
        string? mspInstruction = null,
        string? mspExecutionContext = null,
        bool allowMspCommandRequests = true,
        CancellationToken cancellationToken = default);
}
