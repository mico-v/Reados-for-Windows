using System.Collections.ObjectModel;

namespace ReadOS.App.Models;

public enum LibraryItemKind
{
    Folder,
    Document
}

public sealed class LibraryItem
{
    public required string Id { get; init; }

    public required string DisplayName { get; init; }

    public required LibraryItemKind Kind { get; init; }

    public required string Detail { get; init; }

    public string ProgressText { get; init; } = string.Empty;

    public string KindLabel => Kind == LibraryItemKind.Folder ? "DIR" : "PDF";
}

public sealed class DocumentTab
{
    public required string DocumentId { get; init; }

    public required string Title { get; init; }

    public required string Subtitle { get; init; }

    public required string PageLabel { get; init; }

    public required string PagePreviewText { get; init; }

    public int PageNumber { get; init; }

    public int PageCount { get; init; }

    public ObservableCollection<string> PageChips { get; } = new();

    public ObservableCollection<OutlineItem> Outline { get; } = new();
}

public sealed class OutlineItem
{
    public required string Title { get; init; }

    public required string PageLabel { get; init; }
}

public sealed class ChatConversation
{
    public required string Id { get; init; }

    public required string DocumentId { get; init; }

    public required string Title { get; init; }

    public required string Scope { get; init; }

    public ObservableCollection<ChatMessage> Messages { get; } = new();
}

public sealed class ChatMessage
{
    public required string Author { get; init; }

    public required string Content { get; init; }

    public required string TimeLabel { get; init; }
}

public sealed class AttachmentItem
{
    public required string Name { get; init; }

    public required string Detail { get; init; }
}

public sealed class WorkspaceSeed
{
    public ObservableCollection<LibraryItem> LibraryItems { get; } = new();

    public ObservableCollection<DocumentTab> Documents { get; } = new();

    public ObservableCollection<ChatConversation> Conversations { get; } = new();
}
