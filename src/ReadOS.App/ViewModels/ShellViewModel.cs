using System.Collections.ObjectModel;
using System;
using System.Linq;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using ReadOS.App.Models;
using ReadOS.App.Services;

namespace ReadOS.App.ViewModels;

public partial class ShellViewModel : ObservableObject
{
    private readonly WorkspaceSeed workspace;

    [ObservableProperty]
    public partial bool IsLibraryPaneOpen { get; set; } = true;

    [ObservableProperty]
    public partial bool IsChatPaneOpen { get; set; } = true;

    [ObservableProperty]
    public partial string LibrarySearchQuery { get; set; } = string.Empty;

    [ObservableProperty]
    public partial LibraryItem? SelectedLibraryItem { get; set; }

    [ObservableProperty]
    public partial DocumentTab? SelectedTab { get; set; }

    [ObservableProperty]
    public partial ChatConversation? CurrentConversation { get; set; }

    [ObservableProperty]
    public partial OutlineItem? SelectedOutlineItem { get; set; }

    [ObservableProperty]
    public partial string PromptDraft { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string PageRangeDraft { get; set; } = "144-145";

    [ObservableProperty]
    public partial string StatusMessage { get; set; } = "Ready. Mock workspace loaded.";

    [ObservableProperty]
    public partial int CurrentPageNumber { get; set; } = 1;

    public ShellViewModel(IWorkspaceService workspaceService)
    {
        workspace = workspaceService.CreateSeed();

        foreach (var item in workspace.LibraryItems)
        {
            LibraryItems.Add(item);
        }

        foreach (var conversation in workspace.Conversations)
        {
            Conversations.Add(conversation);
        }

        var firstDocument = workspace.Documents.First();
        OpenDocument(firstDocument.DocumentId);
    }

    public ObservableCollection<LibraryItem> LibraryItems { get; } = new();

    public ObservableCollection<DocumentTab> OpenTabs { get; } = new();

    public ObservableCollection<string> PageChips { get; } = new();

    public ObservableCollection<OutlineItem> CurrentOutline { get; } = new();

    public ObservableCollection<ChatConversation> Conversations { get; } = new();

    public ObservableCollection<ChatConversation> DocumentConversations { get; } = new();

    public ObservableCollection<ChatMessage> CurrentMessages { get; } = new();

    public ObservableCollection<AttachmentItem> Attachments { get; } = new();

    public string SelectedDocumentTitle => SelectedTab?.Title ?? "No document selected";

    public string SelectedDocumentSubtitle => SelectedTab?.Subtitle ?? "Choose a document from the library";

    public string ReaderHeading => SelectedTab is null ? "Reader" : "Reader preview";

    public string ReaderSubheading => SelectedTab is null ? "No PDF loaded" : "PDF engine placeholder";

    public string CurrentPageLabel => SelectedTab is null ? "No page" : $"{SelectedTab.PageLabelPrefix} {CurrentPageNumber}";

    public string PagePreviewText => SelectedTab?.PagePreviewText ?? "Open a PDF to begin.";

    public int CurrentPageCount => SelectedTab?.PageCount ?? 1;

    public string CurrentConversationScope => CurrentConversation?.Scope ?? "No PDF selected";

    public string ActiveModelLabel => "Model: mock provider";

    public string AttachmentSummary => Attachments.Count == 0 ? "No attachments" : $"{Attachments.Count} attachment(s) ready";

    public string LibrarySummary => $"{LibraryItems.Count} item(s)";

    partial void OnLibrarySearchQueryChanged(string value)
    {
        RefreshLibraryItems();
    }

    partial void OnSelectedLibraryItemChanged(LibraryItem? value)
    {
        if (value?.Kind == LibraryItemKind.Document)
        {
            OpenDocument(value.Id);
            return;
        }

        if (value?.Kind == LibraryItemKind.Folder)
        {
            StatusMessage = $"Selected folder \"{value.DisplayName}\". Nested navigation lands with the library persistence milestone.";
        }
    }

    partial void OnSelectedTabChanged(DocumentTab? value)
    {
        if (value is null)
        {
            return;
        }

        CurrentPageNumber = value.PageNumber;
        RefreshDocumentState(value);
        StatusMessage = $"Opened {value.Title}.";
    }

    partial void OnCurrentConversationChanged(ChatConversation? value)
    {
        RefreshMessages(value);
    }

    partial void OnSelectedOutlineItemChanged(OutlineItem? value)
    {
        if (value is null)
        {
            return;
        }

        CurrentPageNumber = Math.Clamp(value.PageNumber, 1, CurrentPageCount);
        StatusMessage = $"Jumped to outline item: {value.Title}.";
    }

    partial void OnCurrentPageNumberChanged(int value)
    {
        if (SelectedTab is null)
        {
            return;
        }

        OnPropertyChanged(nameof(CurrentPageLabel));
        StatusMessage = $"Preview page moved to {CurrentPageLabel}. Real PDF navigation will be wired in the PDF engine milestone.";
    }

    [RelayCommand]
    private void ToggleLibraryPane()
    {
        IsLibraryPaneOpen = !IsLibraryPaneOpen;
    }

    [RelayCommand]
    private void ToggleChatPane()
    {
        IsChatPaneOpen = !IsChatPaneOpen;
    }

    [RelayCommand]
    private void OpenSettings()
    {
        StatusMessage = "Settings placeholder: providers, prompts, shortcuts, and MinorU configuration will live here.";
    }

    [RelayCommand]
    private void ImportDocument()
    {
        var number = workspace.Documents.Count + 1;
        var documentId = $"imported-{number}";
        var title = $"Imported Draft {number}.pdf";

        var libraryItem = new LibraryItem
        {
            Id = documentId,
            DisplayName = title,
            Kind = LibraryItemKind.Document,
            Detail = "Mock Imports / Inbox",
            ProgressText = "Newly imported"
        };

        var document = new DocumentTab
        {
            DocumentId = documentId,
            Title = title,
            Subtitle = "Mock import created from the toolbar",
            PageLabel = "PDF page 1",
            PageLabelPrefix = "PDF page",
            PageNumber = 1,
            PageCount = 24,
            PagePreviewText = "This document was created by the MVP import mock. Real file pickers and persistence come next."
        };

        document.PageChips.Add("1");
        document.PageChips.Add("2");
        document.PageChips.Add("3");
        document.Outline.Add(new OutlineItem { Title = "Imported overview", PageLabel = "PDF page 1", PageNumber = 1 });
        document.Outline.Add(new OutlineItem { Title = "Notes to review", PageLabel = "PDF page 6", PageNumber = 6 });

        var conversation = new ChatConversation
        {
            Id = $"chat-{documentId}-1",
            DocumentId = documentId,
            Title = "Import notes",
            Scope = title
        };
        conversation.Messages.Add(new ChatMessage
        {
            Author = "ReadOS",
            TimeLabel = "now",
            Content = "Mock import is ready. The real importer will attach a PDF file and persist it in the library."
        });

        workspace.LibraryItems.Add(libraryItem);
        workspace.Documents.Add(document);
        workspace.Conversations.Add(conversation);
        Conversations.Add(conversation);

        LibrarySearchQuery = string.Empty;
        RefreshLibraryItems();
        OpenDocument(documentId);
        SelectedLibraryItem = LibraryItems.FirstOrDefault(item => item.Id == documentId);
        StatusMessage = $"Imported mock document: {title}.";
    }

    [RelayCommand]
    private void ConfigurePageMapping()
    {
        if (SelectedTab is null)
        {
            return;
        }

        if (!SelectedTab.PageChips.Contains("mapped"))
        {
            SelectedTab.PageChips.Add("mapped");
            PageChips.Add("mapped");
        }

        StatusMessage = "Page mapping mock complete: cover, roman numerals, and book pages are represented by chips.";
    }

    [RelayCommand]
    private void GenerateOutline()
    {
        if (SelectedTab is null)
        {
            return;
        }

        if (SelectedTab.Outline.All(item => item.Title != "AI generated study checkpoint"))
        {
            var checkpoint = new OutlineItem
            {
                Title = "AI generated study checkpoint",
                PageLabel = $"{SelectedTab.PageLabelPrefix} {Math.Min(CurrentPageNumber + 2, CurrentPageCount)}",
                PageNumber = Math.Min(CurrentPageNumber + 2, CurrentPageCount)
            };

            SelectedTab.Outline.Add(checkpoint);
            CurrentOutline.Add(checkpoint);
        }

        StatusMessage = "Outline mock updated. Metadata export will write these entries into the PDF later.";
    }

    [RelayCommand]
    private void AttachCurrentPage()
    {
        if (SelectedTab is null)
        {
            return;
        }

        Attachments.Add(new AttachmentItem
        {
            Name = SelectedTab.PageLabel,
            Detail = $"Attached from {SelectedTab.Title}"
        });

        PromptDraft = "Please explain the attached page in the order used by the book.";
        StatusMessage = $"Attached {SelectedTab.PageLabel}.";
        OnPropertyChanged(nameof(AttachmentSummary));
    }

    [RelayCommand]
    private void AttachPageRange()
    {
        if (SelectedTab is null)
        {
            return;
        }

        var range = string.IsNullOrWhiteSpace(PageRangeDraft) ? CurrentPageLabel : PageRangeDraft.Trim();
        Attachments.Add(new AttachmentItem
        {
            Name = $"Pages {range}",
            Detail = $"Range attachment from {SelectedTab.Title}"
        });

        PromptDraft = $"Please explain pages {range} and keep the explanation aligned with the book.";
        StatusMessage = $"Attached page range: {range}.";
        OnPropertyChanged(nameof(AttachmentSummary));
    }

    [RelayCommand]
    private void StartRegionExplain()
    {
        if (SelectedTab is null)
        {
            return;
        }

        Attachments.Add(new AttachmentItem
        {
            Name = "Red-box region + context",
            Detail = $"{SelectedTab.PageLabel} with surrounding pages"
        });

        PromptDraft = "Please use the surrounding context and focus on the red-boxed content.";
        StatusMessage = "Region explain mock prepared.";
        OnPropertyChanged(nameof(AttachmentSummary));
    }

    [RelayCommand]
    private void ClearAttachments()
    {
        Attachments.Clear();
        StatusMessage = "Cleared pending attachments.";
        OnPropertyChanged(nameof(AttachmentSummary));
    }

    [RelayCommand]
    private void CreateConversation()
    {
        if (SelectedTab is null)
        {
            return;
        }

        var conversation = new ChatConversation
        {
            Id = $"chat-{SelectedTab.DocumentId}-{Conversations.Count + 1}",
            DocumentId = SelectedTab.DocumentId,
            Title = "New study note",
            Scope = SelectedTab.Title
        };

        conversation.Messages.Add(new ChatMessage
        {
            Author = "ReadOS",
            TimeLabel = "now",
            Content = "New mock conversation created. Real persistence lands with the chat warehouse milestone."
        });

        Conversations.Add(conversation);
        DocumentConversations.Add(conversation);
        CurrentConversation = conversation;
        StatusMessage = "Created a new per-PDF conversation.";
    }

    [RelayCommand]
    private void SendPrompt()
    {
        if (SelectedTab is null)
        {
            return;
        }

        if (CurrentConversation is null || CurrentConversation.DocumentId != SelectedTab.DocumentId)
        {
            CreateConversation();
        }

        if (CurrentConversation is null)
        {
            return;
        }

        var prompt = string.IsNullOrWhiteSpace(PromptDraft)
            ? "Please explain the attached material."
            : PromptDraft.Trim();
        var attachmentNote = Attachments.Count == 0
            ? "No attachments"
            : string.Join(", ", Attachments.Select(item => item.Name));

        CurrentConversation.Messages.Add(new ChatMessage
        {
            Author = "You",
            TimeLabel = "now",
            Content = $"{prompt}\n\nAttachments: {attachmentNote}"
        });
        CurrentConversation.Messages.Add(new ChatMessage
        {
            Author = "ReadOS",
            TimeLabel = "mock",
            Content = $"Mock answer for {SelectedTab.Title}. Real model calls will use the selected provider, PDF page images, and per-document system prompt."
        });

        RefreshMessages(CurrentConversation);
        PromptDraft = string.Empty;
        Attachments.Clear();
        OnPropertyChanged(nameof(AttachmentSummary));
        StatusMessage = "Sent mock prompt and stored it in the per-PDF chat warehouse.";
    }

    [RelayCommand]
    private void CloseCurrentTab()
    {
        if (SelectedTab is null)
        {
            return;
        }

        var tab = SelectedTab;
        var index = OpenTabs.IndexOf(tab);
        OpenTabs.Remove(tab);

        SelectedTab = OpenTabs.Count == 0
            ? null
            : OpenTabs[Math.Clamp(index - 1, 0, OpenTabs.Count - 1)];

        if (SelectedTab is null)
        {
            ClearDocumentState();
            StatusMessage = $"Closed {tab.Title}.";
        }
    }

    [RelayCommand]
    private void PreviousPage()
    {
        if (CurrentPageNumber > 1)
        {
            CurrentPageNumber--;
        }
    }

    [RelayCommand]
    private void NextPage()
    {
        if (CurrentPageNumber < CurrentPageCount)
        {
            CurrentPageNumber++;
        }
    }

    private void OpenDocument(string documentId)
    {
        var document = workspace.Documents.FirstOrDefault(item => item.DocumentId == documentId);
        if (document is null)
        {
            StatusMessage = "Folder selected. Nested navigation lands with the library milestone.";
            return;
        }

        var existing = OpenTabs.FirstOrDefault(item => item.DocumentId == document.DocumentId);
        if (existing is null)
        {
            OpenTabs.Add(document);
            existing = document;
        }

        SelectedTab = existing;
    }

    private void RefreshDocumentState(DocumentTab document)
    {
        PageChips.Clear();
        foreach (var chip in document.PageChips)
        {
            PageChips.Add(chip);
        }

        CurrentOutline.Clear();
        foreach (var item in document.Outline)
        {
            CurrentOutline.Add(item);
        }
        SelectedOutlineItem = null;

        Attachments.Clear();
        RefreshDocumentConversations(document.DocumentId);
        OnPropertyChanged(nameof(SelectedDocumentTitle));
        OnPropertyChanged(nameof(SelectedDocumentSubtitle));
        OnPropertyChanged(nameof(ReaderHeading));
        OnPropertyChanged(nameof(ReaderSubheading));
        OnPropertyChanged(nameof(CurrentPageLabel));
        OnPropertyChanged(nameof(PagePreviewText));
        OnPropertyChanged(nameof(CurrentPageCount));
        OnPropertyChanged(nameof(AttachmentSummary));

        CurrentConversation = DocumentConversations.FirstOrDefault();
        OnPropertyChanged(nameof(CurrentConversationScope));
    }

    private void RefreshMessages(ChatConversation? conversation)
    {
        CurrentMessages.Clear();

        if (conversation is not null)
        {
            foreach (var message in conversation.Messages)
            {
                CurrentMessages.Add(message);
            }
        }

        OnPropertyChanged(nameof(CurrentConversationScope));
    }

    private void RefreshLibraryItems()
    {
        var query = LibrarySearchQuery.Trim();
        var items = string.IsNullOrWhiteSpace(query)
            ? workspace.LibraryItems
            : new ObservableCollection<LibraryItem>(workspace.LibraryItems.Where(item =>
                item.DisplayName.Contains(query, StringComparison.OrdinalIgnoreCase) ||
                item.Detail.Contains(query, StringComparison.OrdinalIgnoreCase) ||
                item.ProgressText.Contains(query, StringComparison.OrdinalIgnoreCase)));

        LibraryItems.Clear();
        foreach (var item in items)
        {
            LibraryItems.Add(item);
        }

        OnPropertyChanged(nameof(LibrarySummary));
    }

    private void RefreshDocumentConversations(string documentId)
    {
        DocumentConversations.Clear();
        foreach (var conversation in Conversations.Where(item => item.DocumentId == documentId))
        {
            DocumentConversations.Add(conversation);
        }
    }

    private void ClearDocumentState()
    {
        PageChips.Clear();
        CurrentOutline.Clear();
        DocumentConversations.Clear();
        CurrentMessages.Clear();
        Attachments.Clear();
        CurrentConversation = null;
        OnPropertyChanged(nameof(SelectedDocumentTitle));
        OnPropertyChanged(nameof(SelectedDocumentSubtitle));
        OnPropertyChanged(nameof(ReaderHeading));
        OnPropertyChanged(nameof(ReaderSubheading));
        OnPropertyChanged(nameof(CurrentPageLabel));
        OnPropertyChanged(nameof(PagePreviewText));
        OnPropertyChanged(nameof(CurrentPageCount));
        OnPropertyChanged(nameof(CurrentConversationScope));
        OnPropertyChanged(nameof(AttachmentSummary));
    }
}
