using System;
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
    public partial bool IsSettingsOpen { get; set; }

    [ObservableProperty]
    public partial double LibraryPaneWidth { get; set; } = 304;

    [ObservableProperty]
    public partial double ChatPaneWidth { get; set; } = 392;

    [ObservableProperty]
    public partial double OutlinePaneWidth { get; set; } = 272;

    [ObservableProperty]
    public partial AppStrings Strings { get; set; } = LocalizationCatalog.GetStrings("zh-CN");

    [ObservableProperty]
    public partial LanguageOption? SelectedLanguageOption { get; set; }

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
    public partial string StatusMessage { get; set; } = "就绪。已加载模拟工作区。";

    [ObservableProperty]
    public partial int CurrentPageNumber { get; set; } = 1;

    [ObservableProperty]
    public partial string ProviderName { get; set; } = "OpenAI Compatible";

    [ObservableProperty]
    public partial string ProviderBaseUrl { get; set; } = "https://api.openai.com/v1";

    [ObservableProperty]
    public partial string ProviderApiKey { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string ModelName { get; set; } = "gpt-4.1";

    [ObservableProperty]
    public partial string AttachmentDefaultPrompt { get; set; } = "请按书本顺序解释我附加的页面。";

    [ObservableProperty]
    public partial string RegionExplainPrompt { get; set; } = "请结合上下文，重点讲解红框内容。";

    [ObservableProperty]
    public partial string ChapterExplainPrompt { get; set; } = "请围绕我附加的这一整节内容进行系统讲解。";

    [ObservableProperty]
    public partial string MinorUEndpoint { get; set; } = "https://mineru.net/api";

    [ObservableProperty]
    public partial bool UseMockResponses { get; set; } = true;

    public ShellViewModel(IWorkspaceService workspaceService)
    {
        workspace = workspaceService.CreateSeed();

        LanguageOptions.Add(new LanguageOption { Code = "zh-CN", DisplayName = "中文" });
        LanguageOptions.Add(new LanguageOption { Code = "en-US", DisplayName = "English" });
        SelectedLanguageOption = LanguageOptions.First();

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

    public ObservableCollection<LanguageOption> LanguageOptions { get; } = new();

    public ObservableCollection<LibraryItem> LibraryItems { get; } = new();

    public ObservableCollection<DocumentTab> OpenTabs { get; } = new();

    public ObservableCollection<string> PageChips { get; } = new();

    public ObservableCollection<OutlineItem> CurrentOutline { get; } = new();

    public ObservableCollection<ChatConversation> Conversations { get; } = new();

    public ObservableCollection<ChatConversation> DocumentConversations { get; } = new();

    public ObservableCollection<ChatMessage> CurrentMessages { get; } = new();

    public ObservableCollection<AttachmentItem> Attachments { get; } = new();

    public string SelectedDocumentTitle => SelectedTab?.Title ?? Strings.NoDocumentSelected;

    public string SelectedDocumentSubtitle => SelectedTab?.Subtitle ?? Strings.ChooseDocument;

    public string ReaderHeading => SelectedTab is null ? Strings.Reader : Strings.ReaderPreview;

    public string ReaderSubheading => SelectedTab is null ? Strings.NoPdfLoaded : Strings.PdfEnginePlaceholder;

    public string CurrentPageLabel => SelectedTab is null ? Strings.NoPage : $"{CurrentPagePrefix} {CurrentPageNumber}";

    public string PagePreviewText => SelectedTab?.PagePreviewText ?? Strings.OpenPdfToBegin;

    public int CurrentPageCount => SelectedTab?.PageCount ?? 1;

    public string CurrentConversationScope => CurrentConversation?.Scope ?? Strings.NoPdfSelected;

    public string ActiveModelLabel => string.Format(Strings.ModelLabelFormat, UseMockResponses ? "mock provider" : ModelName);

    public string AttachmentSummary => Attachments.Count == 0 ? Strings.NoAttachments : string.Format(Strings.AttachmentsReadyFormat, Attachments.Count);

    public string LibrarySummary => string.Format(Strings.LibrarySummaryFormat, LibraryItems.Count);

    private string CurrentPagePrefix => SelectedTab?.PageLabelPrefix == "PDF page" ? Strings.PdfPage : Strings.BookPage;

    partial void OnSelectedLanguageOptionChanged(LanguageOption? value)
    {
        if (value is null)
        {
            return;
        }

        Strings = LocalizationCatalog.GetStrings(value.Code);
        NotifyLocalizedProperties();
        StatusMessage = value.Code == "zh-CN" ? "已切换到中文界面。" : "Switched to English.";
    }

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
            StatusMessage = $"已选择文件夹“{value.DisplayName}”。嵌套导航会在资料库持久化阶段实现。";
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
        StatusMessage = $"已打开 {value.Title}。";
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
        StatusMessage = $"已跳转到目录项：{value.Title}。";
    }

    partial void OnCurrentPageNumberChanged(int value)
    {
        if (SelectedTab is null)
        {
            return;
        }

        OnPropertyChanged(nameof(CurrentPageLabel));
        StatusMessage = $"预览页已移动到 {CurrentPageLabel}。真实 PDF 导航会在 PDF 引擎阶段接入。";
    }

    partial void OnUseMockResponsesChanged(bool value)
    {
        OnPropertyChanged(nameof(ActiveModelLabel));
    }

    partial void OnModelNameChanged(string value)
    {
        OnPropertyChanged(nameof(ActiveModelLabel));
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
        IsSettingsOpen = true;
        StatusMessage = "已打开设置。";
    }

    [RelayCommand]
    private void CloseSettings()
    {
        IsSettingsOpen = false;
        StatusMessage = "已关闭设置。";
    }

    [RelayCommand]
    private void SaveSettings()
    {
        IsSettingsOpen = false;
        OnPropertyChanged(nameof(ActiveModelLabel));
        StatusMessage = "设置已保存到当前会话。持久化会在数据层里实现。";
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
        StatusMessage = $"已导入模拟文档：{title}。";
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

        StatusMessage = "页码映射模拟完成：封面、罗马数字页和书本页会显示为标签。";
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
            var pageNumber = Math.Min(CurrentPageNumber + 2, CurrentPageCount);
            var checkpoint = new OutlineItem
            {
                Title = "AI generated study checkpoint",
                PageLabel = $"{CurrentPagePrefix} {pageNumber}",
                PageNumber = pageNumber
            };

            SelectedTab.Outline.Add(checkpoint);
            CurrentOutline.Add(checkpoint);
        }

        StatusMessage = "目录模拟已更新。后续导出时会写入 PDF 元数据。";
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
            Name = CurrentPageLabel,
            Detail = $"Attached from {SelectedTab.Title}"
        });

        PromptDraft = AttachmentDefaultPrompt;
        StatusMessage = $"已附加 {CurrentPageLabel}。";
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

        PromptDraft = $"{AttachmentDefaultPrompt} 范围：{range}";
        StatusMessage = $"已附加页码范围：{range}。";
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
            Detail = $"{CurrentPageLabel} with surrounding pages"
        });

        PromptDraft = RegionExplainPrompt;
        StatusMessage = "框选讲解附件已准备。";
        OnPropertyChanged(nameof(AttachmentSummary));
    }

    [RelayCommand]
    private void ClearAttachments()
    {
        Attachments.Clear();
        StatusMessage = "已清空待发送附件。";
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
            Title = "新的学习记录",
            Scope = SelectedTab.Title
        };

        conversation.Messages.Add(new ChatMessage
        {
            Author = "ReadOS",
            TimeLabel = "now",
            Content = "已创建新的模拟对话。真实持久化会在对话仓库里实现。"
        });

        Conversations.Add(conversation);
        DocumentConversations.Add(conversation);
        CurrentConversation = conversation;
        StatusMessage = "已创建当前 PDF 的新对话。";
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
            ? AttachmentDefaultPrompt
            : PromptDraft.Trim();
        var attachmentNote = Attachments.Count == 0
            ? Strings.NoAttachments
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
            TimeLabel = UseMockResponses ? "mock" : "queued",
            Content = UseMockResponses
                ? $"这是 {SelectedTab.Title} 的模拟回答。真实模型调用会使用设置中的服务商、PDF 页面图像和文档提示词。"
                : $"将使用 {ProviderName} / {ModelName} 调用真实模型。当前 MVP 还没有接入网络请求。"
        });

        RefreshMessages(CurrentConversation);
        PromptDraft = string.Empty;
        Attachments.Clear();
        OnPropertyChanged(nameof(AttachmentSummary));
        StatusMessage = "已发送模拟提问，并保存到当前 PDF 的对话仓库。";
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
            StatusMessage = $"已关闭 {tab.Title}。";
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
            StatusMessage = "已选择文件夹。嵌套导航会在资料库持久化阶段实现。";
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

    private void NotifyLocalizedProperties()
    {
        OnPropertyChanged(nameof(SelectedDocumentTitle));
        OnPropertyChanged(nameof(SelectedDocumentSubtitle));
        OnPropertyChanged(nameof(ReaderHeading));
        OnPropertyChanged(nameof(ReaderSubheading));
        OnPropertyChanged(nameof(CurrentPageLabel));
        OnPropertyChanged(nameof(PagePreviewText));
        OnPropertyChanged(nameof(CurrentConversationScope));
        OnPropertyChanged(nameof(ActiveModelLabel));
        OnPropertyChanged(nameof(AttachmentSummary));
        OnPropertyChanged(nameof(LibrarySummary));
    }
}
