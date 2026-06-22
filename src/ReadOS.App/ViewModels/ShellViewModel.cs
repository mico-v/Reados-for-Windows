using System.Collections.ObjectModel;
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
    public partial string PromptDraft { get; set; } = string.Empty;

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

    public ObservableCollection<ChatMessage> CurrentMessages { get; } = new();

    public ObservableCollection<AttachmentItem> Attachments { get; } = new();

    public string SelectedDocumentTitle => SelectedTab?.Title ?? "No document selected";

    public string SelectedDocumentSubtitle => SelectedTab?.Subtitle ?? "Choose a document from the library";

    public string ReaderHeading => SelectedTab is null ? "Reader" : "Reader preview";

    public string ReaderSubheading => SelectedTab is null ? "No PDF loaded" : "PDF engine placeholder";

    public string CurrentPageLabel => SelectedTab?.PageLabel ?? "No page";

    public string PagePreviewText => SelectedTab?.PagePreviewText ?? "Open a PDF to begin.";

    public int CurrentPageCount => SelectedTab?.PageCount ?? 1;

    public string CurrentConversationScope => CurrentConversation?.Scope ?? "No PDF selected";

    public string ActiveModelLabel => "Model: mock provider";

    partial void OnSelectedLibraryItemChanged(LibraryItem? value)
    {
        if (value?.Kind == LibraryItemKind.Document)
        {
            OpenDocument(value.Id);
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

    partial void OnCurrentPageNumberChanged(int value)
    {
        if (SelectedTab is null)
        {
            return;
        }

        StatusMessage = $"Preview page slider moved to {value}. Real PDF navigation will be wired in the PDF engine milestone.";
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
        StatusMessage = "Import placeholder: real file import lands with the library persistence milestone.";
    }

    [RelayCommand]
    private void ConfigurePageMapping()
    {
        StatusMessage = "Page mapping placeholder: AI vision job and manual mapping editor are planned.";
    }

    [RelayCommand]
    private void GenerateOutline()
    {
        StatusMessage = "Outline placeholder: AI-generated outline will bind to PDF metadata export.";
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
        CurrentConversation = conversation;
        StatusMessage = "Created a new per-PDF conversation.";
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

        Attachments.Clear();
        OnPropertyChanged(nameof(SelectedDocumentTitle));
        OnPropertyChanged(nameof(SelectedDocumentSubtitle));
        OnPropertyChanged(nameof(ReaderHeading));
        OnPropertyChanged(nameof(ReaderSubheading));
        OnPropertyChanged(nameof(CurrentPageLabel));
        OnPropertyChanged(nameof(PagePreviewText));
        OnPropertyChanged(nameof(CurrentPageCount));

        CurrentConversation = Conversations.FirstOrDefault(item => item.DocumentId == document.DocumentId);
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
}
