using ReadOS.App.Models;

namespace ReadOS.App.Services;

public sealed class MockWorkspaceService : IWorkspaceService
{
    public WorkspaceSeed CreateSeed()
    {
        var seed = new WorkspaceSeed();

        seed.LibraryItems.Add(new LibraryItem
        {
            Id = "folder-textbooks",
            DisplayName = "Textbooks",
            Kind = LibraryItemKind.Folder,
            Detail = "12 documents",
            ProgressText = "Ordered manually"
        });
        seed.LibraryItems.Add(new LibraryItem
        {
            Id = "networks",
            DisplayName = "Computer Networking.pdf",
            Kind = LibraryItemKind.Document,
            Detail = "Textbooks / Networking",
            ProgressText = "Reading page 144"
        });
        seed.LibraryItems.Add(new LibraryItem
        {
            Id = "algebra",
            DisplayName = "Abstract Algebra.pdf",
            Kind = LibraryItemKind.Document,
            Detail = "Textbooks / Mathematics",
            ProgressText = "Mapped pages ready"
        });
        seed.LibraryItems.Add(new LibraryItem
        {
            Id = "transformers",
            DisplayName = "Transformer Notes.pdf",
            Kind = LibraryItemKind.Document,
            Detail = "Papers / Machine Learning",
            ProgressText = "Outline pending"
        });
        seed.LibraryItems.Add(new LibraryItem
        {
            Id = "folder-papers",
            DisplayName = "Papers",
            Kind = LibraryItemKind.Folder,
            Detail = "7 documents",
            ProgressText = "Synced locally"
        });

        seed.Documents.Add(CreateNetworkingDocument());
        seed.Documents.Add(CreateAlgebraDocument());
        seed.Documents.Add(CreateTransformerDocument());

        seed.Conversations.Add(CreateNetworkingConversation());
        seed.Conversations.Add(CreateAlgebraConversation());
        seed.Conversations.Add(CreateTransformerConversation());

        return seed;
    }

    private static DocumentTab CreateNetworkingDocument()
    {
        var document = new DocumentTab
        {
            DocumentId = "networks",
            Title = "Computer Networking.pdf",
            Subtitle = "AI page mapping and outline mock are active",
            PageLabel = "Book page 144",
            PageNumber = 144,
            PageCount = 328,
            PagePreviewText = "This placeholder represents the PDF surface. The first MVP proves the library, reader, outline, attachment, and chat workflow before the PDF engine is wired in."
        };

        document.PageChips.Add("cover");
        document.PageChips.Add("i");
        document.PageChips.Add("1");
        document.PageChips.Add("144");
        document.PageChips.Add("145");

        document.Outline.Add(new OutlineItem { Title = "6.4 Network security", PageLabel = "Book page 170" });
        document.Outline.Add(new OutlineItem { Title = "6.4.1 Cryptographic principles", PageLabel = "Book page 171" });
        document.Outline.Add(new OutlineItem { Title = "6.4.2 Symmetric key algorithms", PageLabel = "Book page 172" });
        document.Outline.Add(new OutlineItem { Title = "6.4.3 Public key cryptography", PageLabel = "Book page 176" });

        return document;
    }

    private static DocumentTab CreateAlgebraDocument()
    {
        var document = new DocumentTab
        {
            DocumentId = "algebra",
            Title = "Abstract Algebra.pdf",
            Subtitle = "Per-PDF system prompt mock is ready",
            PageLabel = "Book page 37",
            PageNumber = 37,
            PageCount = 412,
            PagePreviewText = "This mock document shows how page labels, outline entries, and chat history will stay bound to each PDF."
        };

        document.PageChips.Add("cover");
        document.PageChips.Add("iii");
        document.PageChips.Add("1");
        document.PageChips.Add("37");
        document.PageChips.Add("38");

        document.Outline.Add(new OutlineItem { Title = "2.1 Groups and symmetries", PageLabel = "Book page 31" });
        document.Outline.Add(new OutlineItem { Title = "2.2 Subgroups", PageLabel = "Book page 37" });
        document.Outline.Add(new OutlineItem { Title = "2.3 Cyclic groups", PageLabel = "Book page 45" });

        return document;
    }

    private static DocumentTab CreateTransformerDocument()
    {
        var document = new DocumentTab
        {
            DocumentId = "transformers",
            Title = "Transformer Notes.pdf",
            Subtitle = "Outline generation is waiting for a vision model",
            PageLabel = "PDF page 12",
            PageNumber = 12,
            PageCount = 64,
            PagePreviewText = "Later this area will render the real PDF. The MVP keeps the UX shape while services are still mocks."
        };

        document.PageChips.Add("1");
        document.PageChips.Add("2");
        document.PageChips.Add("12");
        document.PageChips.Add("13");

        document.Outline.Add(new OutlineItem { Title = "Attention overview", PageLabel = "PDF page 8" });
        document.Outline.Add(new OutlineItem { Title = "Multi-head attention", PageLabel = "PDF page 12" });
        document.Outline.Add(new OutlineItem { Title = "Position encodings", PageLabel = "PDF page 18" });

        return document;
    }

    private static ChatConversation CreateNetworkingConversation()
    {
        var conversation = new ChatConversation
        {
            Id = "chat-networks-1",
            DocumentId = "networks",
            Title = "Explain pages 144-145",
            Scope = "Computer Networking.pdf"
        };

        conversation.Messages.Add(new ChatMessage
        {
            Author = "You",
            TimeLabel = "mock",
            Content = "Explain these two pages in the order used by the book."
        });
        conversation.Messages.Add(new ChatMessage
        {
            Author = "ReadOS",
            TimeLabel = "mock",
            Content = "The answer will stay attached to the PDF, page labels, and original attachments so it can be reviewed later."
        });

        return conversation;
    }

    private static ChatConversation CreateAlgebraConversation()
    {
        var conversation = new ChatConversation
        {
            Id = "chat-algebra-1",
            DocumentId = "algebra",
            Title = "Subgroups intuition",
            Scope = "Abstract Algebra.pdf"
        };

        conversation.Messages.Add(new ChatMessage
        {
            Author = "You",
            TimeLabel = "mock",
            Content = "Use concrete examples to explain this section."
        });
        conversation.Messages.Add(new ChatMessage
        {
            Author = "ReadOS",
            TimeLabel = "mock",
            Content = "The per-document prompt will let this book use a different teaching style from other PDFs."
        });

        return conversation;
    }

    private static ChatConversation CreateTransformerConversation()
    {
        var conversation = new ChatConversation
        {
            Id = "chat-transformers-1",
            DocumentId = "transformers",
            Title = "Attention notes",
            Scope = "Transformer Notes.pdf"
        };

        conversation.Messages.Add(new ChatMessage
        {
            Author = "You",
            TimeLabel = "mock",
            Content = "What should I focus on before reading the attention equations?"
        });
        conversation.Messages.Add(new ChatMessage
        {
            Author = "ReadOS",
            TimeLabel = "mock",
            Content = "The first MVP keeps this as mock chat until provider settings and model calls are added."
        });

        return conversation;
    }
}
