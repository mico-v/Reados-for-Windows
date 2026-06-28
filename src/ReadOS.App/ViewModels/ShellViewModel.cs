using System.Collections.ObjectModel;
using System.Diagnostics;
using System.Text.RegularExpressions;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media.Imaging;
using ReadOS.App.Models;
using ReadOS.App.Services;
using ReadOS.App.Services.Msp;
using ReadOS.Msp.Models;

namespace ReadOS.App.ViewModels;

public enum ShellRoute
{
    Home,
    Reader,
    Settings
}

public enum SettingsRoute
{
    General,
    Provider,
    Prompts,
    Workspace
}

public sealed partial class ShellViewModel : ObservableObject
{
    private readonly IWorkspaceStore workspaceStore;
    private readonly IPdfDocumentService pdfService;
    private readonly IFileDialogService fileDialogService;
    private readonly IAiChatService aiChatService;
    private readonly ReadOsMspHost mspHost;
    private WorkspaceState? workspace;
    private Window? hostWindow;
    private bool suppressNavigationSelection;
    private bool suppressThumbnailSelection;

    public ShellViewModel(
        IWorkspaceStore workspaceStore,
        IPdfDocumentService pdfService,
        IFileDialogService fileDialogService,
        IAiChatService aiChatService)
    {
        this.workspaceStore = workspaceStore;
        this.pdfService = pdfService;
        this.fileDialogService = fileDialogService;
        this.aiChatService = aiChatService;
        mspHost = new ReadOsMspHost(
            workspaceStore,
            pdfService,
            () => workspace,
            () => SelectedDocument);

        LanguageOptions.Add(new LanguageOption { Code = "zh-CN", DisplayName = "中文" });
        LanguageOptions.Add(new LanguageOption { Code = "en-US", DisplayName = "English" });
    }

    public ObservableCollection<LanguageOption> LanguageOptions { get; } = new();

    public ObservableCollection<ProjectItem> Projects { get; } = new();

    public ObservableCollection<NavigationEntry> NavigationEntries { get; } = new();

    public ObservableCollection<LibraryItem> LibraryItems { get; } = new();

    public ObservableCollection<PageImageItem> Thumbnails { get; } = new();

    public ObservableCollection<OutlineItem> Outline { get; } = new();

    public ObservableCollection<PdfTextHit> DocumentSearchResults { get; } = new();

    public ObservableCollection<ChatConversation> Conversations { get; } = new();

    public ObservableCollection<ChatMessage> ChatMessages { get; } = new();

    public ObservableCollection<ChatAttachment> PendingAttachments { get; } = new();

    [ObservableProperty]
    public partial AppStrings Strings { get; set; } = LocalizationCatalog.GetStrings("zh-CN");

    [ObservableProperty]
    public partial LanguageOption? SelectedLanguageOption { get; set; }

    [ObservableProperty]
    public partial NavigationEntry? SelectedNavigationEntry { get; set; }

    [ObservableProperty]
    public partial ProjectItem? SelectedProject { get; set; }

    [ObservableProperty]
    public partial LibraryItem? SelectedDocument { get; set; }

    [ObservableProperty]
    public partial ChatConversation? SelectedConversation { get; set; }

    [ObservableProperty]
    public partial PageImageItem? SelectedThumbnail { get; set; }

    [ObservableProperty]
    public partial OutlineItem? SelectedOutlineItem { get; set; }

    [ObservableProperty]
    public partial PdfTextHit? SelectedSearchResult { get; set; }

    [ObservableProperty]
    public partial string SearchQuery { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string DocumentSearchQuery { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string ComposerDraft { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string PageJumpText { get; set; } = "1";

    [ObservableProperty]
    public partial string PageRangeDraft { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string CurrentPageLabelDraft { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string OutlineTitleDraft { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string DocumentNameDraft { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string StatusMessage { get; set; } = "正在启动 ReadOS...";

    [ObservableProperty]
    public partial bool IsBusy { get; set; }

    [ObservableProperty]
    public partial ShellRoute CurrentRoute { get; set; } = ShellRoute.Home;

    [ObservableProperty]
    public partial SettingsRoute SelectedSettingsRoute { get; set; } = SettingsRoute.Provider;

    [ObservableProperty]
    public partial bool IsSettingsOpen { get; set; }

    [ObservableProperty]
    public partial bool IsLibraryVisible { get; set; } = true;

    [ObservableProperty]
    public partial bool IsChatVisible { get; set; } = true;

    [ObservableProperty]
    public partial bool IsOutlineVisible { get; set; } = true;

    [ObservableProperty]
    public partial bool IsThumbnailsVisible { get; set; } = true;

    [ObservableProperty]
    public partial bool IsRegionModeActive { get; set; }

    [ObservableProperty]
    public partial double SidebarWidth { get; set; } = 280;

    [ObservableProperty]
    public partial double ChatWidth { get; set; } = 324;

    [ObservableProperty]
    public partial double ReaderImageWidth { get; set; } = 760;

    [ObservableProperty]
    public partial int CurrentPageNumber { get; set; }

    [ObservableProperty]
    public partial BitmapImage? CurrentPageImage { get; set; }

    [ObservableProperty]
    public partial string ProviderName { get; set; } = "OpenAI Compatible";

    [ObservableProperty]
    public partial string ProviderBaseUrl { get; set; } = "https://api.openai.com/v1";

    [ObservableProperty]
    public partial string ProviderApiKey { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string ModelName { get; set; } = "gpt-4.1-mini";

    [ObservableProperty]
    public partial bool UseOfflineResponses { get; set; } = true;

    [ObservableProperty]
    public partial bool IsDarkTheme { get; set; }

    [ObservableProperty]
    public partial string AttachmentDefaultPrompt { get; set; } = "请按书本顺序解释我附加的页面。";

    [ObservableProperty]
    public partial string RegionExplainPrompt { get; set; } = "请结合上下文，重点讲解红框内容。";

    [ObservableProperty]
    public partial string ChapterExplainPrompt { get; set; } = "请围绕我附加的这一整节内容进行系统讲解。";

    [ObservableProperty]
    public partial string MinorUEndpoint { get; set; } = "https://mineru.net/api";

    public string ActiveTitle => SelectedDocument?.Name ?? SelectedProject?.Name ?? "ReadOS";

    public string ActiveSubtitle => SelectedDocument is null
        ? SelectedProject?.Description ?? "导入 PDF 后开始阅读"
        : $"{SelectedDocument.Detail} · {workspaceStore.GetAbsolutePath(SelectedDocument)}";

    public string CurrentPageIndicator => SelectedDocument is null || SelectedDocument.PageCount == 0
        ? "0 / 0"
        : $"{CurrentPageNumber} / {SelectedDocument.PageCount}";

    public bool HasDocument => SelectedDocument is not null;

    public bool HasPdfDocument => SelectedDocument?.Kind == LibraryItemKind.Pdf;

    public bool HasPageImage => CurrentPageImage is not null;

    public string LibrarySummary => SelectedProject is null
        ? "0 个文件"
        : $"{SelectedProject.LibraryItems.Count} 个文件";

    public string PendingAttachmentSummary => PendingAttachments.Count == 0
        ? "暂无附件"
        : $"{PendingAttachments.Count} 个附件待发送";

    public string ActiveModelLabel => UseOfflineResponses
        ? "离线阅读模式"
        : $"{ProviderName} / {ModelName}";

    public string ThemeLabel => IsDarkTheme ? "深色" : "浅色";

    public string ThemeToggleLabel => IsDarkTheme ? "切换浅色" : "切换深色";

    public bool IsHomeRoute => CurrentRoute == ShellRoute.Home;

    public bool IsReaderRoute => CurrentRoute == ShellRoute.Reader;

    public bool IsSettingsRoute => CurrentRoute == ShellRoute.Settings;

    public bool IsGeneralSettingsSelected => SelectedSettingsRoute == SettingsRoute.General;

    public bool IsProviderSettingsSelected => SelectedSettingsRoute == SettingsRoute.Provider;

    public bool IsPromptSettingsSelected => SelectedSettingsRoute == SettingsRoute.Prompts;

    public bool IsWorkspaceSettingsSelected => SelectedSettingsRoute == SettingsRoute.Workspace;

    public void SetHostWindow(Window window)
    {
        hostWindow = window;
    }

    public ValueTask<MspCommandResult> ExecuteMspCommandAsync(
        string commandText,
        string actor = "reados-agent",
        CancellationToken cancellationToken = default)
    {
        return mspHost.ExecuteAsync(commandText, actor, cancellationToken);
    }

    public async Task InitializeAsync()
    {
        IsBusy = true;
        try
        {
            workspace = await workspaceStore.LoadAsync();
            LoadSettings(workspace.Settings);
            RefreshProjects();
            var firstProject = workspace.Projects.FirstOrDefault();
            if (firstProject is not null)
            {
                await SelectProjectAsync(firstProject, firstProject.LibraryItems.FirstOrDefault(item => item.Kind == LibraryItemKind.Pdf));
            }

            StatusMessage = $"工作区已加载：{workspaceStore.WorkspaceRoot}";
        }
        catch (Exception ex)
        {
            StatusMessage = $"启动失败：{ex.Message}";
        }
        finally
        {
            IsBusy = false;
        }
    }

    partial void OnSelectedLanguageOptionChanged(LanguageOption? value)
    {
        if (value is null)
        {
            return;
        }

        Strings = LocalizationCatalog.GetStrings(value.Code);
        if (workspace is not null)
        {
            workspace.Settings.LanguageCode = value.Code;
            _ = SaveWorkspaceAsync();
        }
    }

    async partial void OnSelectedNavigationEntryChanged(NavigationEntry? value)
    {
        if (suppressNavigationSelection || value is null || workspace is null)
        {
            return;
        }

        var project = workspace.Projects.FirstOrDefault(item => item.Id == value.ProjectId);
        if (project is null)
        {
            return;
        }

        var document = value.DocumentId is null
            ? project.LibraryItems.FirstOrDefault(item => item.Kind == LibraryItemKind.Pdf)
            : project.LibraryItems.FirstOrDefault(item => item.Id == value.DocumentId);
        await SelectProjectAsync(project, document);
    }

    async partial void OnSelectedDocumentChanged(LibraryItem? value)
    {
        if (value is null)
        {
            CurrentPageImage = null;
            CurrentPageNumber = 0;
            RefreshDocumentCollections();
            NotifyActiveContext();
            return;
        }

        DocumentNameDraft = value.Name;
        CurrentPageNumber = Math.Clamp(value.CurrentPage, value.PageCount > 0 ? 1 : 0, Math.Max(1, value.PageCount));
        PageJumpText = CurrentPageNumber.ToString();
        CurrentPageLabelDraft = value.CurrentPageLabel;
        RefreshDocumentCollections();
        NotifyActiveContext();
        await LoadCurrentPageAsync();
        _ = LoadThumbnailsAsync();
    }

    async partial void OnSelectedThumbnailChanged(PageImageItem? value)
    {
        if (suppressThumbnailSelection || value is null)
        {
            return;
        }

        await GoToPageAsync(value.PageNumber);
    }

    async partial void OnSelectedOutlineItemChanged(OutlineItem? value)
    {
        if (value is null || value.Page <= 0)
        {
            return;
        }

        await GoToPageAsync(value.Page);
    }

    async partial void OnSelectedSearchResultChanged(PdfTextHit? value)
    {
        if (value is null)
        {
            return;
        }

        await GoToPageAsync(value.PageNumber);
    }

    partial void OnSearchQueryChanged(string value)
    {
        RefreshNavigation();
        RefreshLibrary();
    }

    async partial void OnReaderImageWidthChanged(double value)
    {
        if (HasPdfDocument)
        {
            await LoadCurrentPageAsync();
        }
    }

    partial void OnUseOfflineResponsesChanged(bool value)
    {
        OnPropertyChanged(nameof(ActiveModelLabel));
    }

    partial void OnIsDarkThemeChanged(bool value)
    {
        OnPropertyChanged(nameof(ThemeLabel));
        OnPropertyChanged(nameof(ThemeToggleLabel));
        if (workspace is not null)
        {
            workspace.Settings.UseDarkTheme = value;
            _ = SaveWorkspaceAsync();
        }
    }

    partial void OnProviderNameChanged(string value) => OnPropertyChanged(nameof(ActiveModelLabel));

    partial void OnModelNameChanged(string value) => OnPropertyChanged(nameof(ActiveModelLabel));

    partial void OnCurrentRouteChanged(ShellRoute value)
    {
        IsSettingsOpen = value == ShellRoute.Settings;
        OnPropertyChanged(nameof(IsHomeRoute));
        OnPropertyChanged(nameof(IsReaderRoute));
        OnPropertyChanged(nameof(IsSettingsRoute));
    }

    partial void OnSelectedSettingsRouteChanged(SettingsRoute value)
    {
        OnPropertyChanged(nameof(IsGeneralSettingsSelected));
        OnPropertyChanged(nameof(IsProviderSettingsSelected));
        OnPropertyChanged(nameof(IsPromptSettingsSelected));
        OnPropertyChanged(nameof(IsWorkspaceSettingsSelected));
    }

    [RelayCommand]
    private void NavigateHome()
    {
        CurrentRoute = ShellRoute.Home;
    }

    [RelayCommand]
    private void NavigateReader()
    {
        CurrentRoute = ShellRoute.Reader;
    }

    [RelayCommand]
    private void NavigateSettings()
    {
        CurrentRoute = ShellRoute.Settings;
    }

    [RelayCommand]
    private void SelectSettingsCategory(string category)
    {
        if (Enum.TryParse<SettingsRoute>(category, ignoreCase: true, out var route))
        {
            SelectedSettingsRoute = route;
        }
    }

    [RelayCommand]
    private void ToggleTheme()
    {
        IsDarkTheme = !IsDarkTheme;
    }

    [RelayCommand]
    private async Task CreateProjectAsync()
    {
        if (workspace is null)
        {
            return;
        }

        var project = await workspaceStore.CreateProjectAsync(workspace, $"阅读项目 {workspace.Projects.Count + 1}");
        RefreshProjects();
        await SelectProjectAsync(project, null);
        StatusMessage = $"已创建项目：{project.Name}";
    }

    [RelayCommand]
    private async Task ImportMaterialAsync()
    {
        if (workspace is null)
        {
            return;
        }

        if (SelectedProject is null)
        {
            await CreateProjectAsync();
        }

        if (SelectedProject is null || hostWindow is null)
        {
            return;
        }

        var paths = await fileDialogService.PickPdfFilesAsync(hostWindow);
        if (paths.Count == 0)
        {
            return;
        }

        IsBusy = true;
        try
        {
            LibraryItem? firstImported = null;
            foreach (var path in paths)
            {
                var imported = await workspaceStore.ImportDocumentAsync(workspace, SelectedProject, path);
                firstImported ??= imported;
            }

            RefreshNavigation();
            RefreshLibrary();
            if (firstImported is not null)
            {
                await SelectProjectAsync(SelectedProject, firstImported);
            }

            StatusMessage = $"已导入 {paths.Count} 个文件。";
        }
        catch (Exception ex)
        {
            StatusMessage = $"导入失败：{ex.Message}";
        }
        finally
        {
            IsBusy = false;
        }
    }

    [RelayCommand]
    private async Task RenameDocumentAsync()
    {
        if (workspace is null || SelectedDocument is null)
        {
            return;
        }

        await workspaceStore.RenameDocumentAsync(workspace, SelectedDocument, DocumentNameDraft);
        RefreshNavigation();
        RefreshLibrary();
        NotifyActiveContext();
        StatusMessage = $"已重命名为：{SelectedDocument.Name}";
    }

    [RelayCommand]
    private async Task DeleteDocumentAsync()
    {
        if (workspace is null || SelectedProject is null || SelectedDocument is null)
        {
            return;
        }

        var deleted = SelectedDocument;
        await workspaceStore.DeleteDocumentAsync(workspace, SelectedProject, deleted);
        SelectedDocument = SelectedProject.LibraryItems.FirstOrDefault(item => item.Kind == LibraryItemKind.Pdf);
        RefreshNavigation();
        RefreshLibrary();
        StatusMessage = $"已移入 ReadOS 回收站：{deleted.Name}";
    }

    [RelayCommand]
    private async Task PreviousPageAsync()
    {
        await GoToPageAsync(CurrentPageNumber - 1);
    }

    [RelayCommand]
    private async Task NextPageAsync()
    {
        await GoToPageAsync(CurrentPageNumber + 1);
    }

    [RelayCommand]
    private async Task JumpToPageAsync()
    {
        if (SelectedDocument is null)
        {
            return;
        }

        var page = ResolvePage(PageJumpText);
        if (page <= 0)
        {
            StatusMessage = $"找不到页码：{PageJumpText}";
            return;
        }

        await GoToPageAsync(page);
    }

    [RelayCommand]
    private async Task SavePageLabelAsync()
    {
        if (workspace is null || SelectedDocument is null || CurrentPageNumber <= 0)
        {
            return;
        }

        var label = SelectedDocument.PageLabels.FirstOrDefault(item => item.PdfPage == CurrentPageNumber);
        if (label is null)
        {
            label = new PageLabelRule { PdfPage = CurrentPageNumber };
            SelectedDocument.PageLabels.Add(label);
        }

        label.Label = string.IsNullOrWhiteSpace(CurrentPageLabelDraft) ? CurrentPageNumber.ToString() : CurrentPageLabelDraft.Trim();
        await SaveWorkspaceAsync();
        StatusMessage = $"已保存第 {CurrentPageNumber} 页标签：{label.Label}";
    }

    [RelayCommand]
    private async Task AutoMapPagesAsync()
    {
        if (workspace is null || SelectedDocument is null || SelectedDocument.PageCount == 0)
        {
            return;
        }

        SelectedDocument.PageLabels.Clear();
        for (var page = 1; page <= SelectedDocument.PageCount; page++)
        {
            var label = page switch
            {
                1 => "cover",
                2 => "i",
                3 => "ii",
                4 => "iii",
                _ => (page - 4).ToString()
            };
            SelectedDocument.PageLabels.Add(new PageLabelRule { PdfPage = page, Label = label });
        }

        CurrentPageLabelDraft = SelectedDocument.CurrentPageLabel;
        await SaveWorkspaceAsync();
        StatusMessage = "已生成可编辑页码映射。";
    }

    [RelayCommand]
    private async Task GenerateOutlineAsync()
    {
        if (workspace is null || SelectedDocument is null || !HasPdfDocument)
        {
            return;
        }

        IsBusy = true;
        try
        {
            var path = workspaceStore.GetAbsolutePath(SelectedDocument);
            var maxPage = Math.Min(SelectedDocument.PageCount, 12);
            var text = await pdfService.ExtractPageTextAsync(path, 1, maxPage);
            var generated = ExtractOutlineFromText(text);
            if (generated.Count == 0)
            {
                for (var page = 1; page <= SelectedDocument.PageCount; page += 10)
                {
                    generated.Add(new OutlineItem { Title = $"第 {page} 页起", Page = page, Level = 1 });
                }
            }

            SelectedDocument.Outline.Clear();
            foreach (var item in generated.Take(80))
            {
                SelectedDocument.Outline.Add(item);
            }

            RefreshOutline();
            await SaveWorkspaceAsync();
            StatusMessage = $"已生成 {SelectedDocument.Outline.Count} 条目录项，可继续手动编辑。";
        }
        catch (Exception ex)
        {
            StatusMessage = $"目录生成失败：{ex.Message}";
        }
        finally
        {
            IsBusy = false;
        }
    }

    [RelayCommand]
    private async Task AddOutlineItemAsync()
    {
        if (workspace is null || SelectedDocument is null)
        {
            return;
        }

        var title = string.IsNullOrWhiteSpace(OutlineTitleDraft)
            ? $"第 {CurrentPageNumber} 页"
            : OutlineTitleDraft.Trim();
        var item = new OutlineItem
        {
            Title = title,
            Page = Math.Max(1, CurrentPageNumber),
            Level = 1
        };
        SelectedDocument.Outline.Add(item);
        RefreshOutline();
        OutlineTitleDraft = string.Empty;
        await SaveWorkspaceAsync();
    }

    [RelayCommand]
    private async Task DeleteOutlineItemAsync()
    {
        if (workspace is null || SelectedDocument is null || SelectedOutlineItem is null)
        {
            return;
        }

        SelectedDocument.Outline.Remove(SelectedOutlineItem);
        RefreshOutline();
        await SaveWorkspaceAsync();
    }

    [RelayCommand]
    private async Task SearchInDocumentAsync()
    {
        DocumentSearchResults.Clear();
        if (SelectedDocument is null || !HasPdfDocument || string.IsNullOrWhiteSpace(DocumentSearchQuery))
        {
            return;
        }

        IsBusy = true;
        try
        {
            var hits = await pdfService.SearchAsync(workspaceStore.GetAbsolutePath(SelectedDocument), DocumentSearchQuery);
            foreach (var hit in hits)
            {
                DocumentSearchResults.Add(hit);
            }

            StatusMessage = hits.Count == 0 ? "未找到匹配内容。" : $"找到 {hits.Count} 个匹配项。";
        }
        catch (Exception ex)
        {
            StatusMessage = $"搜索失败：{ex.Message}";
        }
        finally
        {
            IsBusy = false;
        }
    }

    [RelayCommand]
    private void AttachCurrentPage()
    {
        if (SelectedDocument is null || CurrentPageNumber <= 0)
        {
            return;
        }

        PendingAttachments.Add(new ChatAttachment
        {
            Kind = AttachmentKind.Page,
            DocumentId = SelectedDocument.Id,
            Title = $"{SelectedDocument.Name} · 第 {CurrentPageNumber} 页",
            StartPage = CurrentPageNumber,
            EndPage = CurrentPageNumber
        });
        NotifyAttachmentState();
    }

    [RelayCommand]
    private void AttachRange()
    {
        if (SelectedDocument is null)
        {
            return;
        }

        var ranges = ParseRanges(PageRangeDraft);
        if (ranges.Count == 0)
        {
            StatusMessage = "请输入页码范围，例如 3-5, 8。";
            return;
        }

        foreach (var range in ranges)
        {
            PendingAttachments.Add(new ChatAttachment
            {
                Kind = range.Start == range.End ? AttachmentKind.Page : AttachmentKind.PageRange,
                DocumentId = SelectedDocument.Id,
                Title = $"{SelectedDocument.Name} · 第 {range.Start}-{range.End} 页",
                StartPage = range.Start,
                EndPage = range.End
            });
        }

        PageRangeDraft = string.Empty;
        NotifyAttachmentState();
    }

    [RelayCommand]
    private void StartRegionSelection()
    {
        if (!HasPdfDocument)
        {
            StatusMessage = "请先打开一份 PDF。";
            return;
        }

        IsRegionModeActive = true;
        StatusMessage = "在页面上拖动鼠标框选需要讲解的区域。";
    }

    public void AttachRegionSelection(double x, double y, double width, double height)
    {
        if (SelectedDocument is null || CurrentPageNumber <= 0)
        {
            return;
        }

        PendingAttachments.Add(new ChatAttachment
        {
            Kind = AttachmentKind.Region,
            DocumentId = SelectedDocument.Id,
            Title = $"{SelectedDocument.Name} · 第 {CurrentPageNumber} 页红框区域",
            StartPage = Math.Max(1, CurrentPageNumber - 1),
            EndPage = Math.Min(SelectedDocument.PageCount, CurrentPageNumber + 1),
            RegionX = x,
            RegionY = y,
            RegionWidth = width,
            RegionHeight = height
        });

        IsRegionModeActive = false;
        ComposerDraft = RegionExplainPrompt;
        NotifyAttachmentState();
        StatusMessage = "已添加红框区域和上下文页面。";
    }

    [RelayCommand]
    private void ClearAttachments()
    {
        PendingAttachments.Clear();
        NotifyAttachmentState();
    }

    [RelayCommand]
    private async Task SendPromptAsync()
    {
        if (workspace is null)
        {
            return;
        }

        EnsureConversation();
        if (SelectedConversation is null)
        {
            return;
        }

        var attachments = PendingAttachments.Select(CloneAttachment).ToList();
        var prompt = string.IsNullOrWhiteSpace(ComposerDraft)
            ? attachments.Count > 0 ? AttachmentDefaultPrompt : "请总结当前文档。"
            : ComposerDraft.Trim();

        var userMessage = new ChatMessage
        {
            Role = ChatRole.User,
            Author = "你",
            Content = prompt,
            CreatedAt = DateTimeOffset.Now
        };
        foreach (var attachment in attachments)
        {
            userMessage.Attachments.Add(attachment);
        }

        SelectedConversation.Messages.Add(userMessage);
        PendingAttachments.Clear();
        ComposerDraft = string.Empty;
        RefreshChatMessages();
        NotifyAttachmentState();

        IsBusy = true;
        try
        {
            var answer = await aiChatService.SendAsync(
                BuildSettingsFromInputs(),
                SelectedDocument,
                SelectedConversation.Messages.Where(message => message.Id != userMessage.Id),
                prompt,
                attachments,
                GetAttachmentTextAsync);

            SelectedConversation.Messages.Add(new ChatMessage
            {
                Role = ChatRole.Assistant,
                Author = "ReadOS",
                Content = answer,
                CreatedAt = DateTimeOffset.Now
            });
            SelectedConversation.UpdatedAt = DateTimeOffset.Now;
            if (SelectedDocument is not null)
            {
                SelectedDocument.UpdatedAt = DateTimeOffset.Now;
            }

            await SaveWorkspaceAsync();
            RefreshChatMessages();
            StatusMessage = "回答已保存到当前文档对话仓库。";
        }
        catch (Exception ex)
        {
            SelectedConversation.Messages.Add(new ChatMessage
            {
                Role = ChatRole.Assistant,
                Author = "ReadOS",
                Content = $"请求失败：{ex.Message}",
                CreatedAt = DateTimeOffset.Now
            });
            RefreshChatMessages();
            StatusMessage = $"请求失败：{ex.Message}";
        }
        finally
        {
            IsBusy = false;
        }
    }

    [RelayCommand]
    private void NewConversation()
    {
        EnsureConversation(forceNew: true);
        RefreshConversations();
        SelectedConversation = Conversations.FirstOrDefault();
        RefreshChatMessages();
    }

    [RelayCommand]
    private async Task ClearConversationAsync()
    {
        if (workspace is null || SelectedConversation is null)
        {
            return;
        }

        SelectedConversation.Messages.Clear();
        RefreshChatMessages();
        await SaveWorkspaceAsync();
    }

    [RelayCommand]
    private async Task SaveSettingsAsync()
    {
        if (workspace is null)
        {
            return;
        }

        workspace.Settings = BuildSettingsFromInputs();
        Strings = LocalizationCatalog.GetStrings(workspace.Settings.LanguageCode);
        await SaveWorkspaceAsync();
        StatusMessage = "设置已保存。";
    }

    [RelayCommand]
    private void OpenSettings()
    {
        CurrentRoute = ShellRoute.Settings;
    }

    [RelayCommand]
    private void CloseSettings()
    {
        CurrentRoute = ShellRoute.Home;
    }

    [RelayCommand]
    private void ToggleLibrary()
    {
        IsLibraryVisible = !IsLibraryVisible;
    }

    [RelayCommand]
    private void ToggleChat()
    {
        IsChatVisible = !IsChatVisible;
    }

    [RelayCommand]
    private void ToggleOutline()
    {
        IsOutlineVisible = !IsOutlineVisible;
    }

    [RelayCommand]
    private void ToggleThumbnails()
    {
        IsThumbnailsVisible = !IsThumbnailsVisible;
    }

    [RelayCommand]
    private void OpenLocation()
    {
        var path = SelectedDocument is null ? workspaceStore.LibraryRoot : workspaceStore.GetAbsolutePath(SelectedDocument);
        if (!Directory.Exists(path) && !File.Exists(path))
        {
            path = workspaceStore.WorkspaceRoot;
        }

        var args = File.Exists(path) ? $"/select,\"{path}\"" : $"\"{path}\"";
        Process.Start(new ProcessStartInfo("explorer.exe", args) { UseShellExecute = true });
    }

    [RelayCommand]
    private async Task ExportWorkspaceAsync()
    {
        if (workspace is null || hostWindow is null)
        {
            return;
        }

        var path = await fileDialogService.PickWorkspaceExportAsync(hostWindow, $"ReadOS-Workspace-{DateTimeOffset.Now:yyyyMMdd-HHmm}.zip");
        if (string.IsNullOrWhiteSpace(path))
        {
            return;
        }

        await workspaceStore.ExportWorkspaceAsync(workspace, path);
        StatusMessage = $"工作区已导出：{path}";
    }

    [RelayCommand]
    private async Task ImportWorkspaceAsync()
    {
        if (hostWindow is null)
        {
            return;
        }

        var path = await fileDialogService.PickWorkspaceImportAsync(hostWindow);
        if (string.IsNullOrWhiteSpace(path))
        {
            return;
        }

        workspace = await workspaceStore.ImportWorkspaceAsync(path);
        LoadSettings(workspace.Settings);
        RefreshProjects();
        await SelectProjectAsync(workspace.Projects.First(), workspace.Projects.First().LibraryItems.FirstOrDefault());
        StatusMessage = "工作区已导入并重新加载。";
    }

    private async Task SelectProjectAsync(ProjectItem project, LibraryItem? document)
    {
        SelectedProject = project;
        RefreshLibrary();
        SelectedDocument = document;
        RefreshConversations();
        UpdateSelectedNavigationEntry();
        NotifyActiveContext();
        await Task.CompletedTask;
    }

    private async Task GoToPageAsync(int page)
    {
        if (workspace is null || SelectedDocument is null || SelectedDocument.PageCount == 0)
        {
            return;
        }

        var target = Math.Clamp(page, 1, SelectedDocument.PageCount);
        CurrentPageNumber = target;
        SelectedDocument.CurrentPage = target;
        PageJumpText = target.ToString();
        CurrentPageLabelDraft = SelectedDocument.CurrentPageLabel;
        await LoadCurrentPageAsync();
        await SaveWorkspaceAsync();
    }

    private async Task LoadCurrentPageAsync()
    {
        if (SelectedDocument is null || SelectedDocument.Kind != LibraryItemKind.Pdf || SelectedDocument.PageCount == 0)
        {
            CurrentPageImage = null;
            NotifyActiveContext();
            return;
        }

        try
        {
            CurrentPageImage = await pdfService.RenderPageAsync(
                workspaceStore.GetAbsolutePath(SelectedDocument),
                Math.Max(1, CurrentPageNumber),
                ReaderImageWidth);
            CurrentPageLabelDraft = SelectedDocument.CurrentPageLabel;
        }
        catch (Exception ex)
        {
            CurrentPageImage = null;
            StatusMessage = $"页面渲染失败：{ex.Message}";
        }

        OnPropertyChanged(nameof(HasPageImage));
        OnPropertyChanged(nameof(CurrentPageIndicator));
    }

    private async Task LoadThumbnailsAsync()
    {
        if (SelectedDocument is null || SelectedDocument.Kind != LibraryItemKind.Pdf)
        {
            Thumbnails.Clear();
            return;
        }

        try
        {
            var thumbnails = await pdfService.RenderThumbnailsAsync(
                workspaceStore.GetAbsolutePath(SelectedDocument),
                SelectedDocument.PageCount,
                24);
            Thumbnails.Clear();
            foreach (var thumbnail in thumbnails)
            {
                var label = SelectedDocument.PageLabels.FirstOrDefault(item => item.PdfPage == thumbnail.PageNumber)?.Label;
                thumbnail.Label = string.IsNullOrWhiteSpace(label) ? thumbnail.PageNumber.ToString() : label;
                Thumbnails.Add(thumbnail);
            }

            suppressThumbnailSelection = true;
            SelectedThumbnail = Thumbnails.FirstOrDefault(item => item.PageNumber == CurrentPageNumber);
            suppressThumbnailSelection = false;
        }
        catch (Exception ex)
        {
            StatusMessage = $"缩略图加载失败：{ex.Message}";
        }
    }

    private void RefreshProjects()
    {
        Projects.Clear();
        if (workspace is null)
        {
            return;
        }

        foreach (var project in workspace.Projects)
        {
            Projects.Add(project);
        }

        RefreshNavigation();
    }

    private void RefreshNavigation()
    {
        NavigationEntries.Clear();
        if (workspace is null)
        {
            return;
        }

        var query = SearchQuery.Trim();
        foreach (var project in workspace.Projects)
        {
            var projectMatches = string.IsNullOrWhiteSpace(query) ||
                project.Name.Contains(query, StringComparison.OrdinalIgnoreCase) ||
                project.Description.Contains(query, StringComparison.OrdinalIgnoreCase);
            var documents = project.LibraryItems
                .Where(item => item.Kind != LibraryItemKind.Folder)
                .Where(item => string.IsNullOrWhiteSpace(query) ||
                    projectMatches ||
                    item.SearchText.Contains(query, StringComparison.OrdinalIgnoreCase))
                .OrderBy(item => item.Order)
                .ThenBy(item => item.Name)
                .ToList();

            if (!projectMatches && documents.Count == 0)
            {
                continue;
            }

            NavigationEntries.Add(new NavigationEntry
            {
                Id = project.Id,
                ProjectId = project.Id,
                IsProject = true,
                Title = project.Name,
                Detail = project.UpdatedLabel,
                Prefix = "▸"
            });

            foreach (var document in documents)
            {
                NavigationEntries.Add(new NavigationEntry
                {
                    Id = document.Id,
                    ProjectId = project.Id,
                    DocumentId = document.Id,
                    Title = document.Name,
                    Detail = document.KindLabel,
                    Prefix = "  "
                });
            }
        }

        UpdateSelectedNavigationEntry();
    }

    private void RefreshLibrary()
    {
        LibraryItems.Clear();
        if (SelectedProject is null)
        {
            return;
        }

        var query = SearchQuery.Trim();
        foreach (var item in SelectedProject.LibraryItems
            .Where(item => string.IsNullOrWhiteSpace(query) || item.SearchText.Contains(query, StringComparison.OrdinalIgnoreCase))
            .OrderBy(item => item.Order)
            .ThenBy(item => item.Name))
        {
            LibraryItems.Add(item);
        }

        OnPropertyChanged(nameof(LibrarySummary));
    }

    private void RefreshDocumentCollections()
    {
        RefreshOutline();
        RefreshConversations();
        OnPropertyChanged(nameof(HasDocument));
        OnPropertyChanged(nameof(HasPdfDocument));
        OnPropertyChanged(nameof(CurrentPageIndicator));
    }

    private void RefreshOutline()
    {
        Outline.Clear();
        if (SelectedDocument is null)
        {
            return;
        }

        foreach (var item in SelectedDocument.Outline.OrderBy(item => item.Page).ThenBy(item => item.Level))
        {
            Outline.Add(item);
        }
    }

    private void RefreshConversations()
    {
        Conversations.Clear();
        if (SelectedDocument is not null)
        {
            foreach (var conversation in SelectedDocument.Conversations.OrderByDescending(item => item.UpdatedAt))
            {
                Conversations.Add(conversation);
            }
        }
        else if (SelectedProject is not null)
        {
            foreach (var conversation in SelectedProject.StandaloneConversations.OrderByDescending(item => item.UpdatedAt))
            {
                Conversations.Add(conversation);
            }
        }

        SelectedConversation = Conversations.FirstOrDefault();
        RefreshChatMessages();
    }

    private void RefreshChatMessages()
    {
        ChatMessages.Clear();
        if (SelectedConversation is null)
        {
            return;
        }

        foreach (var message in SelectedConversation.Messages)
        {
            ChatMessages.Add(message);
        }
    }

    private void UpdateSelectedNavigationEntry()
    {
        suppressNavigationSelection = true;
        SelectedNavigationEntry = NavigationEntries.FirstOrDefault(entry =>
            entry.ProjectId == SelectedProject?.Id &&
            entry.DocumentId == SelectedDocument?.Id) ??
            NavigationEntries.FirstOrDefault(entry => entry.ProjectId == SelectedProject?.Id && entry.IsProject);
        suppressNavigationSelection = false;
    }

    private void EnsureConversation(bool forceNew = false)
    {
        ChatConversation? created = null;
        if (!forceNew && SelectedConversation is not null)
        {
            return;
        }

        if (SelectedDocument is not null)
        {
            if (forceNew || SelectedDocument.Conversations.Count == 0)
            {
                created = new ChatConversation
                {
                    DocumentId = SelectedDocument.Id,
                    Title = $"阅读问答 {SelectedDocument.Conversations.Count + 1}",
                    UpdatedAt = DateTimeOffset.Now
                };
                SelectedDocument.Conversations.Insert(0, created);
            }
        }
        else if (SelectedProject is not null)
        {
            if (forceNew || SelectedProject.StandaloneConversations.Count == 0)
            {
                created = new ChatConversation
                {
                    Title = $"项目问答 {SelectedProject.StandaloneConversations.Count + 1}",
                    UpdatedAt = DateTimeOffset.Now
                };
                SelectedProject.StandaloneConversations.Insert(0, created);
            }
        }

        RefreshConversations();
        if (created is not null)
        {
            SelectedConversation = created;
            RefreshChatMessages();
        }
    }

    private async Task<string> GetAttachmentTextAsync(ChatAttachment attachment)
    {
        var document = FindDocument(attachment.DocumentId);
        if (document is null || document.Kind != LibraryItemKind.Pdf)
        {
            return string.Empty;
        }

        return await pdfService.ExtractPageTextAsync(
            workspaceStore.GetAbsolutePath(document),
            Math.Max(1, attachment.StartPage),
            Math.Max(attachment.StartPage, attachment.EndPage));
    }

    private LibraryItem? FindDocument(string id)
    {
        return workspace?.Projects
            .SelectMany(project => project.LibraryItems)
            .FirstOrDefault(item => item.Id == id);
    }

    private int ResolvePage(string text)
    {
        if (SelectedDocument is null || string.IsNullOrWhiteSpace(text))
        {
            return 0;
        }

        if (int.TryParse(text.Trim(), out var page))
        {
            return Math.Clamp(page, 1, SelectedDocument.PageCount);
        }

        var label = SelectedDocument.PageLabels.FirstOrDefault(item =>
            string.Equals(item.Label, text.Trim(), StringComparison.OrdinalIgnoreCase));
        return label?.PdfPage ?? 0;
    }

    private List<(int Start, int End)> ParseRanges(string text)
    {
        var ranges = new List<(int Start, int End)>();
        if (SelectedDocument is null || string.IsNullOrWhiteSpace(text))
        {
            return ranges;
        }

        foreach (var part in text.Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries))
        {
            var edges = part.Split('-', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
            var start = ResolvePage(edges[0]);
            var end = edges.Length > 1 ? ResolvePage(edges[1]) : start;
            if (start <= 0 || end <= 0)
            {
                continue;
            }

            ranges.Add((Math.Min(start, end), Math.Max(start, end)));
        }

        return ranges;
    }

    private static List<OutlineItem> ExtractOutlineFromText(string text)
    {
        var items = new List<OutlineItem>();
        var currentPage = 1;
        foreach (var rawLine in text.Split('\n'))
        {
            var line = rawLine.Trim();
            var pageMatch = Regex.Match(line, @"^\[PDF 第 (?<page>\d+) 页\]");
            if (pageMatch.Success && int.TryParse(pageMatch.Groups["page"].Value, out var parsedPage))
            {
                currentPage = parsedPage;
                continue;
            }

            if (line.Length is < 4 or > 90)
            {
                continue;
            }

            var isHeading =
                Regex.IsMatch(line, @"^(chapter|section)\s+\d+", RegexOptions.IgnoreCase) ||
                Regex.IsMatch(line, @"^第.{1,12}[章节篇]") ||
                Regex.IsMatch(line, @"^\d+(\.\d+){0,3}\s+\S+");
            if (!isHeading)
            {
                continue;
            }

            var level = line.Count(character => character == '.') + 1;
            items.Add(new OutlineItem
            {
                Title = line,
                Page = currentPage,
                Level = Math.Clamp(level, 1, 4)
            });
        }

        return items;
    }

    private WorkspaceSettings BuildSettingsFromInputs()
    {
        return new WorkspaceSettings
        {
            LanguageCode = SelectedLanguageOption?.Code ?? "zh-CN",
            ProviderName = ProviderName,
            ProviderBaseUrl = ProviderBaseUrl,
            ProviderApiKey = ProviderApiKey,
            ModelName = ModelName,
            UseOfflineResponses = UseOfflineResponses,
            UseDarkTheme = IsDarkTheme,
            AttachmentDefaultPrompt = AttachmentDefaultPrompt,
            RegionExplainPrompt = RegionExplainPrompt,
            ChapterExplainPrompt = ChapterExplainPrompt,
            MinorUEndpoint = MinorUEndpoint
        };
    }

    private void LoadSettings(WorkspaceSettings settings)
    {
        ProviderName = settings.ProviderName;
        ProviderBaseUrl = settings.ProviderBaseUrl;
        ProviderApiKey = settings.ProviderApiKey;
        ModelName = settings.ModelName;
        UseOfflineResponses = settings.UseOfflineResponses;
        IsDarkTheme = settings.UseDarkTheme;
        AttachmentDefaultPrompt = settings.AttachmentDefaultPrompt;
        RegionExplainPrompt = settings.RegionExplainPrompt;
        ChapterExplainPrompt = settings.ChapterExplainPrompt;
        MinorUEndpoint = settings.MinorUEndpoint;

        SelectedLanguageOption = LanguageOptions.FirstOrDefault(item => item.Code == settings.LanguageCode) ?? LanguageOptions.First();
        Strings = LocalizationCatalog.GetStrings(SelectedLanguageOption.Code);
        OnPropertyChanged(nameof(ActiveModelLabel));
    }

    private async Task SaveWorkspaceAsync()
    {
        if (workspace is not null)
        {
            await workspaceStore.SaveAsync(workspace);
        }
    }

    private void NotifyActiveContext()
    {
        OnPropertyChanged(nameof(ActiveTitle));
        OnPropertyChanged(nameof(ActiveSubtitle));
        OnPropertyChanged(nameof(HasDocument));
        OnPropertyChanged(nameof(HasPdfDocument));
        OnPropertyChanged(nameof(HasPageImage));
        OnPropertyChanged(nameof(CurrentPageIndicator));
    }

    private void NotifyAttachmentState()
    {
        OnPropertyChanged(nameof(PendingAttachmentSummary));
    }

    private static ChatAttachment CloneAttachment(ChatAttachment source)
    {
        return new ChatAttachment
        {
            Kind = source.Kind,
            DocumentId = source.DocumentId,
            Title = source.Title,
            StartPage = source.StartPage,
            EndPage = source.EndPage,
            FilePath = source.FilePath,
            RegionX = source.RegionX,
            RegionY = source.RegionY,
            RegionWidth = source.RegionWidth,
            RegionHeight = source.RegionHeight
        };
    }
}
