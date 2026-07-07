using ReadOS.App.Models;

namespace ReadOS.App.Services.Msp;

internal readonly record struct ReadOsConversationEnsureResult(
    bool ShouldRefresh,
    ChatConversation? CreatedConversation);

internal sealed class ReadOsConversationService
{
    public IReadOnlyList<ChatConversation> GetVisibleConversations(
        LibraryItem? selectedDocument,
        ProjectItem? selectedProject)
    {
        if (selectedDocument is not null)
        {
            return selectedDocument.Conversations
                .OrderByDescending(item => item.UpdatedAt)
                .ToArray();
        }

        if (selectedProject is not null)
        {
            return selectedProject.StandaloneConversations
                .OrderByDescending(item => item.UpdatedAt)
                .ToArray();
        }

        return Array.Empty<ChatConversation>();
    }

    public IReadOnlyList<ChatMessage> GetMessages(ChatConversation? conversation)
    {
        return conversation?.Messages.ToArray() ?? Array.Empty<ChatMessage>();
    }

    public ReadOsConversationEnsureResult EnsureConversation(
        LibraryItem? selectedDocument,
        ProjectItem? selectedProject,
        ChatConversation? selectedConversation,
        bool forceNew,
        DateTimeOffset createdAt)
    {
        if (!forceNew && selectedConversation is not null)
        {
            return new ReadOsConversationEnsureResult(false, null);
        }

        if (selectedDocument is not null)
        {
            ChatConversation? created = null;
            if (forceNew || selectedDocument.Conversations.Count == 0)
            {
                created = new ChatConversation
                {
                    DocumentId = selectedDocument.Id,
                    Title = $"阅读问答 {selectedDocument.Conversations.Count + 1}",
                    UpdatedAt = createdAt
                };
                selectedDocument.Conversations.Insert(0, created);
            }

            return new ReadOsConversationEnsureResult(true, created);
        }

        if (selectedProject is not null)
        {
            ChatConversation? created = null;
            if (forceNew || selectedProject.StandaloneConversations.Count == 0)
            {
                created = new ChatConversation
                {
                    Title = $"项目问答 {selectedProject.StandaloneConversations.Count + 1}",
                    UpdatedAt = createdAt
                };
                selectedProject.StandaloneConversations.Insert(0, created);
            }

            return new ReadOsConversationEnsureResult(true, created);
        }

        return new ReadOsConversationEnsureResult(false, null);
    }
}
