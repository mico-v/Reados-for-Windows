using ReadOS.App.Models;

namespace ReadOS.App.Services.Msp;

internal readonly record struct ReadOsDocumentCollectionRefresh(
    IReadOnlyList<OutlineItem> OutlineItems,
    IReadOnlyList<ChatConversation> Conversations);

internal sealed class ReadOsDocumentCollectionRefreshService
{
    private readonly ReadOsConversationService conversationService;

    public ReadOsDocumentCollectionRefreshService(ReadOsConversationService conversationService)
    {
        this.conversationService = conversationService;
    }

    public ReadOsDocumentCollectionRefresh Build(
        LibraryItem? selectedDocument,
        ProjectItem? selectedProject)
    {
        return new ReadOsDocumentCollectionRefresh(
            GetOutlineItems(selectedDocument),
            conversationService.GetVisibleConversations(selectedDocument, selectedProject));
    }

    public IReadOnlyList<OutlineItem> GetOutlineItems(LibraryItem? selectedDocument)
    {
        return selectedDocument?.Outline
            .OrderBy(item => item.Page)
            .ThenBy(item => item.Level)
            .ToArray() ?? Array.Empty<OutlineItem>();
    }
}
