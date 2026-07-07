using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsConversationServiceTests
{
    [Fact]
    public void GetVisibleConversations_prefers_selected_document_and_sorts_newest_first()
    {
        var service = new ReadOsConversationService();
        var document = new LibraryItem();
        var project = new ProjectItem();
        var older = new ChatConversation
        {
            Id = "older",
            UpdatedAt = new DateTimeOffset(2026, 7, 7, 10, 0, 0, TimeSpan.Zero)
        };
        var newer = new ChatConversation
        {
            Id = "newer",
            UpdatedAt = older.UpdatedAt.AddMinutes(10)
        };
        document.Conversations.Add(older);
        document.Conversations.Add(newer);
        project.StandaloneConversations.Add(new ChatConversation
        {
            Id = "project"
        });

        var conversations = service.GetVisibleConversations(document, project);

        Assert.Equal(new[] { "newer", "older" }, conversations.Select(item => item.Id));
    }

    [Fact]
    public void GetVisibleConversations_uses_project_when_no_document_is_selected()
    {
        var service = new ReadOsConversationService();
        var project = new ProjectItem();
        var older = new ChatConversation
        {
            Id = "older",
            UpdatedAt = new DateTimeOffset(2026, 7, 7, 10, 0, 0, TimeSpan.Zero)
        };
        var newer = new ChatConversation
        {
            Id = "newer",
            UpdatedAt = older.UpdatedAt.AddMinutes(5)
        };
        project.StandaloneConversations.Add(older);
        project.StandaloneConversations.Add(newer);

        var conversations = service.GetVisibleConversations(null, project);

        Assert.Equal(new[] { "newer", "older" }, conversations.Select(item => item.Id));
    }

    [Fact]
    public void GetMessages_returns_selected_conversation_messages_in_existing_order()
    {
        var service = new ReadOsConversationService();
        var conversation = new ChatConversation();
        conversation.Messages.Add(new ChatMessage
        {
            Id = "first",
            Content = "first"
        });
        conversation.Messages.Add(new ChatMessage
        {
            Id = "second",
            Content = "second"
        });

        var messages = service.GetMessages(conversation);
        var empty = service.GetMessages(null);

        Assert.Equal(new[] { "first", "second" }, messages.Select(item => item.Id));
        Assert.Empty(empty);
    }

    [Fact]
    public void EnsureConversation_noops_when_existing_selection_can_be_reused()
    {
        var service = new ReadOsConversationService();
        var document = new LibraryItem();
        var selected = new ChatConversation
        {
            Id = "selected"
        };
        document.Conversations.Add(selected);

        var result = service.EnsureConversation(
            document,
            null,
            selected,
            forceNew: false,
            DateTimeOffset.UtcNow);

        Assert.False(result.ShouldRefresh);
        Assert.Null(result.CreatedConversation);
        Assert.Single(document.Conversations);
    }

    [Fact]
    public void EnsureConversation_creates_document_conversation_when_needed()
    {
        var service = new ReadOsConversationService();
        var createdAt = new DateTimeOffset(2026, 7, 7, 11, 0, 0, TimeSpan.Zero);
        var document = new LibraryItem
        {
            Id = "doc"
        };

        var result = service.EnsureConversation(document, null, null, forceNew: false, createdAt);

        Assert.True(result.ShouldRefresh);
        Assert.NotNull(result.CreatedConversation);
        Assert.Same(result.CreatedConversation, document.Conversations[0]);
        Assert.Equal("doc", result.CreatedConversation?.DocumentId);
        Assert.Equal("阅读问答 1", result.CreatedConversation?.Title);
        Assert.Equal(createdAt, result.CreatedConversation?.UpdatedAt);
    }

    [Fact]
    public void EnsureConversation_refreshes_existing_document_conversations_without_creating()
    {
        var service = new ReadOsConversationService();
        var document = new LibraryItem();
        document.Conversations.Add(new ChatConversation
        {
            Id = "existing"
        });

        var result = service.EnsureConversation(
            document,
            null,
            selectedConversation: null,
            forceNew: false,
            DateTimeOffset.UtcNow);

        Assert.True(result.ShouldRefresh);
        Assert.Null(result.CreatedConversation);
        Assert.Single(document.Conversations);
    }

    [Fact]
    public void EnsureConversation_force_creates_project_conversation()
    {
        var service = new ReadOsConversationService();
        var createdAt = new DateTimeOffset(2026, 7, 7, 11, 30, 0, TimeSpan.Zero);
        var project = new ProjectItem();
        project.StandaloneConversations.Add(new ChatConversation
        {
            Id = "existing",
            Title = "项目问答 1"
        });

        var result = service.EnsureConversation(null, project, project.StandaloneConversations[0], forceNew: true, createdAt);

        Assert.True(result.ShouldRefresh);
        Assert.NotNull(result.CreatedConversation);
        Assert.Same(result.CreatedConversation, project.StandaloneConversations[0]);
        Assert.Equal("项目问答 2", result.CreatedConversation?.Title);
        Assert.Equal(createdAt, result.CreatedConversation?.UpdatedAt);
        Assert.Equal(2, project.StandaloneConversations.Count);
    }
}
