using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsDocumentCollectionRefreshServiceTests
{
    [Fact]
    public void GetOutlineItems_returns_empty_outline_when_no_document_is_selected()
    {
        var service = CreateService();

        var outline = service.GetOutlineItems(null);

        Assert.Empty(outline);
    }

    [Fact]
    public void GetOutlineItems_sorts_by_page_then_level()
    {
        var service = CreateService();
        var document = new LibraryItem();
        document.Outline.Add(new OutlineItem
        {
            Id = "page-3",
            Page = 3,
            Level = 1
        });
        document.Outline.Add(new OutlineItem
        {
            Id = "page-1-level-2",
            Page = 1,
            Level = 2
        });
        document.Outline.Add(new OutlineItem
        {
            Id = "page-1-level-1",
            Page = 1,
            Level = 1
        });

        var outline = service.GetOutlineItems(document);

        Assert.Equal(new[] { "page-1-level-1", "page-1-level-2", "page-3" }, outline.Select(item => item.Id));
    }

    [Fact]
    public void Build_prefers_document_conversations_and_sorted_outline()
    {
        var service = CreateService();
        var project = new ProjectItem();
        project.StandaloneConversations.Add(new ChatConversation
        {
            Id = "project"
        });
        var document = new LibraryItem();
        document.Outline.Add(new OutlineItem
        {
            Id = "second",
            Page = 2,
            Level = 1
        });
        document.Outline.Add(new OutlineItem
        {
            Id = "first",
            Page = 1,
            Level = 1
        });
        document.Conversations.Add(new ChatConversation
        {
            Id = "older",
            UpdatedAt = new DateTimeOffset(2026, 7, 7, 10, 0, 0, TimeSpan.Zero)
        });
        document.Conversations.Add(new ChatConversation
        {
            Id = "newer",
            UpdatedAt = new DateTimeOffset(2026, 7, 7, 10, 5, 0, TimeSpan.Zero)
        });

        var refresh = service.Build(document, project);

        Assert.Equal(new[] { "first", "second" }, refresh.OutlineItems.Select(item => item.Id));
        Assert.Equal(new[] { "newer", "older" }, refresh.Conversations.Select(item => item.Id));
    }

    [Fact]
    public void Build_uses_project_conversations_when_no_document_is_selected()
    {
        var service = CreateService();
        var project = new ProjectItem();
        project.StandaloneConversations.Add(new ChatConversation
        {
            Id = "older",
            UpdatedAt = new DateTimeOffset(2026, 7, 7, 9, 0, 0, TimeSpan.Zero)
        });
        project.StandaloneConversations.Add(new ChatConversation
        {
            Id = "newer",
            UpdatedAt = new DateTimeOffset(2026, 7, 7, 11, 0, 0, TimeSpan.Zero)
        });

        var refresh = service.Build(null, project);

        Assert.Empty(refresh.OutlineItems);
        Assert.Equal(new[] { "newer", "older" }, refresh.Conversations.Select(item => item.Id));
    }

    private static ReadOsDocumentCollectionRefreshService CreateService()
    {
        return new ReadOsDocumentCollectionRefreshService(new ReadOsConversationService());
    }
}
