using System.Collections.ObjectModel;
using System.Collections.Specialized;
using System.Diagnostics;
using System.Windows.Input;
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

public enum WorkspaceLayoutMode
{
    ChatPrimary,
    PresenterPrimary,
    FocusChat,
    FocusPresenter
}

public sealed partial class ShellViewModel : ObservableObject, IDisposable
{
    private const int MaxMspTranscriptEntries = 200;
    private const int MaxPreparedMspCommands = 8;
    private const string DefaultArtifactRefinementInstruction = "tighten caveats and keep citations";
    private const string ChatModelProviderFailureMessage = "请求失败：模型提供方调用失败。为保护请求与凭据数据，已隐藏提供方响应详情。";

    private readonly IWorkspaceStore workspaceStore;
    private readonly IPdfDocumentService pdfService;
    private readonly IFileDialogService fileDialogService;
    private readonly IAiChatService aiChatService;
    private readonly IClipboardService clipboardService;
    private readonly ReadOsMspTranscriptWorkspaceService mspTranscriptWorkspaceService;
    private readonly ReadOsMspCommandTranscriptService mspTranscriptService;
    private readonly ReadOsArtifactService artifactService;
    private readonly ReadOsArtifactLineageActionService artifactLineageActionService;
    private readonly ReadOsArtifactReuseService artifactReuseService;
    private readonly ReadOsPreparedMspCommandHistoryService preparedCommandHistoryService;
    private readonly ReadOsActiveMspCommandService activeMspCommandService;
    private readonly ReadOsAttachmentQueueService attachmentQueueService;
    private readonly ReadOsMspApprovalReviewService approvalReviewService;
    private readonly ReadOsMspPendingApprovalNavigationService pendingApprovalNavigationService;
    private readonly ReadOsMspAgentBridgeService mspAgentBridgeService;
    private readonly ReadOsChatTurnService chatTurnService;
    private readonly ReadOsConversationService conversationService;
    private readonly ReadOsMspSessionViewService mspSessionViewService;
    private readonly ReadOsTimelineService timelineService;
    private readonly ReadOsChatUiProjectionService chatUiProjectionService;
    private readonly ReadOsTimelineActionService timelineActionService;
    private readonly ReadOsActiveDocumentContextService activeDocumentContextService;
    private readonly ReadOsDocumentCollectionRefreshService documentCollectionRefreshService;
    private readonly ReadOsPresenterLoadPreparationService presenterLoadPreparationService;
    private readonly ReadOsThumbnailLoadPreparationService thumbnailLoadPreparationService;
    private readonly ReadOsPageNavigationService pageNavigationService;
    private readonly ReadOsPageLabelEditingService pageLabelEditingService;
    private readonly ReadOsOutlineEditingService outlineEditingService;
    private readonly ReadOsDocumentSearchService documentSearchService;
    private readonly ReadOsReaderAttachmentService readerAttachmentService;
    private readonly ReadOsReaderLayoutService readerLayoutService;
    private readonly ReadOsWorkflowDraftService workflowDraftService;
    private readonly ReadOsWorkflowPreparationService workflowPreparationService;
    private readonly ReadOsMspHost mspHost;
    private WorkspaceState? workspace;
    private Window? hostWindow;
    private bool suppressNavigationSelection;
    private bool suppressThumbnailSelection;
    private bool isArtifactPreviewActive;

    private long _pageLoadGeneration;
    private CancellationTokenSource? _pageCts;
    private long _presenterLoadGeneration;
    private CancellationTokenSource? _presenterCts;
    private long _thumbnailGeneration;
    private CancellationTokenSource? _thumbCts;
    private long _outlineGeneration;
    private CancellationTokenSource? _outlineCts;
    private long _searchGeneration;
    private CancellationTokenSource? _searchCts;

    public ShellViewModel(
        IWorkspaceStore workspaceStore,
        IPdfDocumentService pdfService,
        IFileDialogService fileDialogService,
        IAiChatService aiChatService,
        IClipboardService? clipboardService = null,
        ReadOsPackagedRuntimeFfiRegistration? runtimeFfiRegistration = null)
    {
        this.workspaceStore = workspaceStore;
        this.pdfService = pdfService;
        this.fileDialogService = fileDialogService;
        this.aiChatService = aiChatService;
        this.clipboardService = clipboardService ?? new ClipboardService();
        mspTranscriptWorkspaceService = new ReadOsMspTranscriptWorkspaceService(
            ReadOsMspHost.DefaultSessionId,
            MaxMspTranscriptEntries);
        mspTranscriptService = new ReadOsMspCommandTranscriptService(ReadOsMspHost.DefaultSessionId);
        artifactService = new ReadOsArtifactService();
        artifactLineageActionService = new ReadOsArtifactLineageActionService(artifactService);
        preparedCommandHistoryService = new ReadOsPreparedMspCommandHistoryService(MaxPreparedMspCommands);
        activeMspCommandService = new ReadOsActiveMspCommandService();
        attachmentQueueService = new ReadOsAttachmentQueueService();
        artifactReuseService = new ReadOsArtifactReuseService(artifactService, attachmentQueueService);
        approvalReviewService = new ReadOsMspApprovalReviewService();
        pendingApprovalNavigationService = new ReadOsMspPendingApprovalNavigationService();
        mspAgentBridgeService = new ReadOsMspAgentBridgeService();
        chatTurnService = new ReadOsChatTurnService();
        conversationService = new ReadOsConversationService();
        mspSessionViewService = new ReadOsMspSessionViewService();
        timelineService = new ReadOsTimelineService();
        chatUiProjectionService = new ReadOsChatUiProjectionService();
        ChatTimeline = new ChatTimelineViewModel();
        ChatUiHostBridge = new ReadOsChatUiHostBridge(
            this.clipboardService,
            fileDialogService);
        ChatUiHostBridge.MessageCopyRequested += OnChatUiMessageCopyRequested;
        timelineActionService = new ReadOsTimelineActionService();
        activeDocumentContextService = new ReadOsActiveDocumentContextService();
        documentCollectionRefreshService = new ReadOsDocumentCollectionRefreshService(conversationService);
        presenterLoadPreparationService = new ReadOsPresenterLoadPreparationService();
        thumbnailLoadPreparationService = new ReadOsThumbnailLoadPreparationService();
        pageNavigationService = new ReadOsPageNavigationService();
        pageLabelEditingService = new ReadOsPageLabelEditingService();
        outlineEditingService = new ReadOsOutlineEditingService();
        documentSearchService = new ReadOsDocumentSearchService();
        readerAttachmentService = new ReadOsReaderAttachmentService(pageNavigationService);
        readerLayoutService = new ReadOsReaderLayoutService();
        workflowDraftService = new ReadOsWorkflowDraftService();
        workflowPreparationService = new ReadOsWorkflowPreparationService(artifactService, workflowDraftService);
        mspHost = new ReadOsMspHost(
            new ReadOsMspHostDependencies(
                workspaceStore,
                pdfService,
                aiChatService,
                () => workspace,
                BuildSettingsFromInputs,
                () => SelectedDocument,
                () => attachmentQueueService.Snapshot(PendingAttachments),
                GetAttachmentTextAsync,
                QueueMspAttachment,
                ClearMspAttachments,
                ApplyMspChatResult,
                runtimeFfiRegistration?.Adapter),
            runtimeFfiEchoCommandAdapter: runtimeFfiRegistration?.Adapter,
            ownsRuntimeFfiEchoCommandAdapter: runtimeFfiRegistration?.Adapter is not null);

        LanguageOptions.Add(new LanguageOption { Code = "zh-CN", DisplayName = "中文" });
        LanguageOptions.Add(new LanguageOption { Code = "en-US", DisplayName = "English" });
        ApprovalModeOptions.Add(new ApprovalModeOption
        {
            Code = MspApprovalModeCodes.Policy,
            DisplayName = "策略",
            Detail = "写入和产物命令需要审批"
        });
        ApprovalModeOptions.Add(new ApprovalModeOption
        {
            Code = MspApprovalModeCodes.ConfirmAll,
            DisplayName = "每次确认",
            Detail = "所有非 dry-run 命令都先审批"
        });
        ApprovalModeOptions.Add(new ApprovalModeOption
        {
            Code = MspApprovalModeCodes.AllowWorkspace,
            DisplayName = "允许写入",
            Detail = "工作区写入直接执行，外部和删除仍需审批"
        });

        Sidebar = new SidebarViewModel(this);
        Thread = new ThreadViewModel(this);
        Inspector = new InspectorViewModel(this);
        Settings = new SettingsViewModel(this);
        MspTranscript.CollectionChanged += OnMspTranscriptCollectionChanged;
        PreparedMspCommands.CollectionChanged += OnPreparedMspCommandsCollectionChanged;
        QueuedComposerPrompts.CollectionChanged += OnQueuedComposerPromptsCollectionChanged;
    }

    // ── Child ViewModels ────────────────────────────────────────────────────

    public SidebarViewModel Sidebar { get; }
    public ThreadViewModel Thread { get; }
    public InspectorViewModel Inspector { get; }
    public SettingsViewModel Settings { get; }

    // ── Shared collections ──────────────────────────────────────────────────

    public ObservableCollection<LanguageOption> LanguageOptions { get; } = new();

    public ObservableCollection<ApprovalModeOption> ApprovalModeOptions { get; } = new();

    public ObservableCollection<ProjectItem> Projects { get; } = new();

    public ObservableCollection<NavigationEntry> NavigationEntries { get; } = new();

    public ObservableCollection<LibraryItem> LibraryItems { get; } = new();

    public ObservableCollection<PageImageItem> Thumbnails { get; } = new();

    public ObservableCollection<OutlineItem> Outline { get; } = new();

    public ObservableCollection<PdfTextHit> DocumentSearchResults { get; } = new();

    public ObservableCollection<ChatConversation> Conversations { get; } = new();

    public ObservableCollection<ChatMessage> ChatMessages { get; } = new();

    public ObservableCollection<ChatAttachment> PendingAttachments { get; } = new();

    public ObservableCollection<QueuedComposerPrompt> QueuedComposerPrompts { get; } = new();

    public ObservableCollection<MspTranscriptEntry> MspTranscript { get; } = new();

    public ObservableCollection<MspSessionEntry> MspSessions { get; } = new();

    public ObservableCollection<WorkspaceArtifact> Artifacts { get; } = new();

    public ObservableCollection<ArtifactLineageItem> SelectedArtifactLineage { get; } = new();

    public ObservableCollection<PreparedMspCommand> PreparedMspCommands { get; } = new();

    public ObservableCollection<ThreadTimelineItem> TimelineItems { get; } = new();

    public ChatTimelineViewModel ChatTimeline { get; }

    // Canonical MSP Chat UI host bridge. Forwarded by the XAML timeline view
    // when its ScrollViewer becomes available; exposes copy/open actions the
    // renderer can call back into. Mirrors Hosts/Windows/src/msp-chat-ui-webview2-host.ts.
    public ReadOsChatUiHostBridge ChatUiHostBridge { get; }

    [ObservableProperty]
    public partial AppStrings Strings { get; set; } = LocalizationCatalog.GetStrings("zh-CN");

    [ObservableProperty]
    public partial LanguageOption? SelectedLanguageOption { get; set; }

    [ObservableProperty]
    public partial ApprovalModeOption? SelectedApprovalModeOption { get; set; }

    [ObservableProperty]
    public partial NavigationEntry? SelectedNavigationEntry { get; set; }

    [ObservableProperty]
    public partial ProjectItem? SelectedProject { get; set; }

    [ObservableProperty]
    public partial LibraryItem? SelectedDocument { get; set; }

    [ObservableProperty]
    public partial ChatConversation? SelectedConversation { get; set; }

    [ObservableProperty]
    public partial MspSessionEntry? SelectedMspSession { get; set; }

    [ObservableProperty]
    public partial MspTranscriptEntry? SelectedMspTranscriptEntry { get; set; }

    [ObservableProperty]
    public partial WorkspaceArtifact? SelectedArtifact { get; set; }

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
    public partial string MspCommandDraft { get; set; } = "workspace info";

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
    public partial WorkspaceLayoutMode WorkspaceLayout { get; set; } = WorkspaceLayoutMode.FocusChat;

    [ObservableProperty]
    public partial WorkspaceSidebarMode SidebarMode { get; set; } = WorkspaceSidebarMode.Conversations;

    [ObservableProperty]
    public partial InspectorTab SelectedInspectorTab { get; set; } = InspectorTab.Evidence;

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

    /// <summary>
    /// Aliases IsLibraryVisible for the 3-column layout.  Sidebar == library panel.
    /// </summary>
    public bool IsSidebarVisible
    {
        get => IsLibraryVisible;
        set => IsLibraryVisible = value;
    }

    /// <summary>
    /// Controls the unified inspector column (replaces the old IsChatVisible-based
    /// visibility formula and the separate presenter column).
    /// </summary>
    [ObservableProperty]
    public partial bool IsInspectorVisible { get; set; } = true;

    [ObservableProperty]
    public partial bool IsRunDrawerOpen { get; set; }

    [ObservableProperty]
    public partial bool IsRunDrawerPinned { get; set; }

    [ObservableProperty]
    public partial bool IsCompactLayout { get; set; }

    [ObservableProperty]
    public partial double SidebarWidth { get; set; } = 280;

    [ObservableProperty]
    public partial double ChatWidth { get; set; } = 324;

    [ObservableProperty]
    public partial double PresenterWidth { get; set; } = 560;

    [ObservableProperty]
    public partial double InspectorWidth { get; set; } = 316;

    [ObservableProperty]
    public partial double RunDrawerHeight { get; set; } = 240;

    public string RunDrawerPinGlyph => IsRunDrawerPinned ? "\uE841" : "\uE718";

    // ── Conversation / overlay flyout state (pure-conversation shell) ──
    [ObservableProperty]
    public partial bool IsHistoryFlyoutOpen { get; set; }

    [ObservableProperty]
    public partial bool IsApprovalsFlyoutOpen { get; set; }

    [ObservableProperty]
    public partial double PresenterDrawerWidth { get; set; } = 148;

    [ObservableProperty]
    public partial double ReaderImageWidth { get; set; } = 760;

    [ObservableProperty]
    public partial int CurrentPageNumber { get; set; }

    [ObservableProperty]
    public partial BitmapImage? CurrentPageImage { get; set; }

    [ObservableProperty]
    public partial string PresenterTextContent { get; set; } = string.Empty;

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

    public bool HasPageImage => !isArtifactPreviewActive && CurrentPageImage is not null;

    public bool HasTextPresenter => !string.IsNullOrWhiteSpace(PresenterTextContent) &&
        CurrentPresenterKind is PresenterKind.Markdown or PresenterKind.Text;

    public bool IsPresenterPlaceholderVisible => !HasPageImage && !HasTextPresenter;

    public string LibrarySummary => SelectedProject is null
        ? "0 个文件"
        : $"{SelectedProject.LibraryItems.Count} 个文件";

    public string PendingAttachmentSummary => PendingAttachments.Count == 0
        ? "暂无附件"
        : $"{PendingAttachments.Count} 个附件待发送";

    public string ComposerQueueSummary => QueuedComposerPrompts.Count == 0
        ? "暂无队列"
        : $"{QueuedComposerPrompts.Count} 个队列项";

    public bool HasQueuedComposerPrompts => QueuedComposerPrompts.Count > 0;

    public string MspTranscriptSummary => MspTranscript.Count == 0
        ? "暂无 MSP 执行记录"
        : $"{MspTranscript.Count} 条 MSP 执行记录";

    public int PendingApprovalCount => MspTranscript.Count(entry => entry.IsApprovalRequired);

    public bool HasPendingApprovals => PendingApprovalCount > 0;

    public bool IsAnyMspRunning => MspTranscript.Any(entry => entry.IsRunning);

    public string PendingApprovalLabel => HasPendingApprovals
        ? $"{PendingApprovalCount} 个审批待处理"
        : "无待审批";

    public string MspActivitySummary
    {
        get
        {
            var running = MspTranscript.Count(entry => entry.IsRunning);
            var failed = MspTranscript.Count(entry => !entry.IsRunning && !entry.IsApprovalRequired && !entry.Succeeded);
            if (running > 0)
            {
                return $"{running} 个 MSP 命令运行中";
            }

            if (PendingApprovalCount > 0)
            {
                return $"{PendingApprovalCount} 个 MSP 命令待审批";
            }

            return failed > 0
                ? $"{failed} 个 MSP 命令需要复查"
                : MspTranscriptSummary;
        }
    }

    public string ArtifactSummary => Artifacts.Count == 0
        ? "暂无产物"
        : $"{Artifacts.Count} 个产物";

    public string RuntimeStatusLabel => IsAnyMspRunning
        ? "MSP 运行中"
        : HasPendingApprovals
            ? "等待审批"
            : IsBusy ? "处理中" : "就绪";

    public string ApprovalModeCode => MspApprovalModeCodes.Normalize(SelectedApprovalModeOption?.Code);

    public string ApprovalModeLabel => SelectedApprovalModeOption is null
        ? "策略审批"
        : SelectedApprovalModeOption.DisplayName;

    public string ApprovalModeDetail => SelectedApprovalModeOption?.Detail ?? "写入和产物命令需要审批";

    public bool IsPolicyApprovalModeSelected => ApprovalModeCode == MspApprovalModeCodes.Policy;

    public bool IsConfirmAllApprovalModeSelected => ApprovalModeCode == MspApprovalModeCodes.ConfirmAll;

    public bool IsAllowWorkspaceApprovalModeSelected => ApprovalModeCode == MspApprovalModeCodes.AllowWorkspace;

    public string SendButtonLabel => IsAnyMspRunning ? "停止" : "发送";

    public string ComposerPrimaryGlyph => IsAnyMspRunning ? "\uE711" : "\uE724";

    public string ComposerPrimaryToolTip => IsAnyMspRunning
        ? "停止当前 MSP 命令"
        : "发送当前提示";

    public ICommand ComposerPrimaryCommand => IsAnyMspRunning
        ? CancelActiveMspCommandCommand
        : SendPromptCommand;

    public string SelectedArtifactPreview => artifactService.GetArtifactPreviewText(SelectedArtifact);

    public bool HasSelectedArtifactLineage => SelectedArtifactLineage.Count > 0;

    public bool HasSelectedOutlineWorkflowTarget => HasPdfDocument && SelectedOutlineItem is not null;

    public bool HasSelectedArtifactWorkflowTarget => SelectedArtifact is not null;

    public bool HasComposerArtifactRefinementTarget =>
        SelectedArtifact is not null &&
        !string.IsNullOrWhiteSpace(ComposerDraft);

    public bool HasSelectedEvidenceArtifact => artifactService.IsEvidenceArtifact(SelectedArtifact);

    public bool HasSelectedFailureReviewTarget => SelectedMspTranscriptEntry is not null &&
        !SelectedMspTranscriptEntry.IsRunning &&
        !SelectedMspTranscriptEntry.IsApprovalRequired &&
        !SelectedMspTranscriptEntry.Succeeded;

    public bool HasPreparedMspCommands => PreparedMspCommands.Count > 0;

    public string ActiveModelLabel => UseOfflineResponses
        ? "离线阅读模式"
        : $"{ProviderName} / {ModelName}";

    public string ThemeLabel => IsDarkTheme ? "深色" : "浅色";

    public string ThemeToggleLabel => IsDarkTheme ? "切换浅色" : "切换深色";

    public bool IsWorkspaceRoute => CurrentRoute != ShellRoute.Settings;

    public bool IsHomeRoute => CurrentRoute == ShellRoute.Home;

    public bool IsReaderRoute => CurrentRoute == ShellRoute.Reader;

    public bool IsSettingsRoute => CurrentRoute == ShellRoute.Settings;

    public bool IsGeneralSettingsSelected => SelectedSettingsRoute == SettingsRoute.General;

    public bool IsProviderSettingsSelected => SelectedSettingsRoute == SettingsRoute.Provider;

    public bool IsPromptSettingsSelected => SelectedSettingsRoute == SettingsRoute.Prompts;

    public bool IsWorkspaceSettingsSelected => SelectedSettingsRoute == SettingsRoute.Workspace;

    public bool IsConversationsSidebarSelected => SidebarMode == WorkspaceSidebarMode.Conversations;

    public bool IsMaterialsSidebarSelected => SidebarMode == WorkspaceSidebarMode.Materials;

    public bool IsSessionsSidebarSelected => SidebarMode == WorkspaceSidebarMode.Sessions;

    public bool IsArtifactsSidebarSelected => SidebarMode == WorkspaceSidebarMode.Artifacts;

    public bool IsContextInspectorSelected => SelectedInspectorTab == InspectorTab.Context;

    public bool IsActionsInspectorSelected => SelectedInspectorTab == InspectorTab.Actions;

    public bool IsRunInspectorSelected => SelectedInspectorTab is InspectorTab.Run or InspectorTab.Actions;

    public bool IsArtifactsInspectorSelected => SelectedInspectorTab == InspectorTab.Artifacts;

    public bool IsPolicyInspectorSelected => SelectedInspectorTab == InspectorTab.Policy;

    public bool IsEvidenceInspectorSelected => SelectedInspectorTab == InspectorTab.Evidence;

    public bool IsAttachmentsInspectorSelected => SelectedInspectorTab == InspectorTab.Attachments;

    public bool IsOutlineInspectorSelected => SelectedInspectorTab == InspectorTab.Outline;

    public bool IsSearchInspectorSelected => SelectedInspectorTab == InspectorTab.Search;

    public bool IsPreviewInspectorSelected => SelectedInspectorTab == InspectorTab.Preview;

    public bool IsPresenterVisible => IsInspectorVisible;

    public bool IsChatWorkspaceVisible => true;

    public string WorkspaceModeLabel => IsInspectorVisible ? "审查面板已打开" : "对话模式";

    public PresenterKind CurrentPresenterKind => isArtifactPreviewActive
        ? SelectedArtifact?.MediaType.Contains("markdown", StringComparison.OrdinalIgnoreCase) == true ||
            SelectedArtifact?.Path.EndsWith(".md", StringComparison.OrdinalIgnoreCase) == true
            ? PresenterKind.Markdown
            : PresenterKind.Text
        : SelectedDocument?.Kind switch
        {
            LibraryItemKind.Pdf => PresenterKind.Pdf,
            LibraryItemKind.Markdown => PresenterKind.Markdown,
            LibraryItemKind.Note => PresenterKind.Text,
            null => PresenterKind.None,
            _ => PresenterKind.DocumentPlaceholder
        };

    public string PresenterKindLabel => CurrentPresenterKind switch
    {
        PresenterKind.Pdf => "PDF 演示器",
        PresenterKind.Markdown => "Markdown 预览",
        PresenterKind.Text => "文本预览",
        PresenterKind.ImagePlaceholder => "图片预览",
        PresenterKind.VideoPlaceholder => "视频预览",
        PresenterKind.DocumentPlaceholder => "文档预览",
        _ => "资料演示器"
    };

    public string ActiveWorkspaceScope => SelectedDocument is null
        ? SelectedProject?.Name ?? "本地阅读工作台"
        : $"{SelectedDocument.KindLabel} · {CurrentPageIndicator}";

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

    public ValueTask<MspCommandResult> ExecuteApprovedMspCommandAsync(
        string commandText,
        string actor = "reados-agent",
        CancellationToken cancellationToken = default)
    {
        return mspHost.ExecuteApprovedAsync(commandText, actor, cancellationToken);
    }

    public IAsyncEnumerable<MspCommandEvent> ExecuteMspCommandStreamingAsync(
        string commandText,
        string actor = "reados-agent",
        CancellationToken cancellationToken = default)
    {
        return mspHost.ExecuteStreamingAsync(commandText, actor, cancellationToken);
    }

    public IAsyncEnumerable<MspCommandEvent> ExecuteApprovedMspCommandStreamingAsync(
        string commandText,
        string actor = "reados-agent",
        CancellationToken cancellationToken = default)
    {
        return mspHost.ExecuteApprovedStreamingAsync(commandText, actor, cancellationToken);
    }

    [RelayCommand]
    private async Task RunMspCommandAsync()
    {
        if (string.IsNullOrWhiteSpace(MspCommandDraft))
        {
            StatusMessage = "请输入 MSP 命令。";
            return;
        }

        IsBusy = true;
        try
        {
            var entry = await ExecuteAndRecordMspCommandAsync(MspCommandDraft.Trim(), "user");
            StatusMessage = entry.WasCanceled
                ? $"MSP 命令已取消：{entry.CommandText}"
                : entry.Succeeded
                ? $"MSP 命令完成：{entry.CommandText}"
                : $"MSP 命令失败：{entry.CommandText}";
        }
        finally
        {
            IsBusy = false;
        }
    }

    [RelayCommand]
    private async Task ApproveMspCommandAsync(MspTranscriptEntry? entry)
    {
        if (!approvalReviewService.CanReview(entry) || entry is null)
        {
            return;
        }

        IsBusy = true;
        try
        {
            approvalReviewService.RemovePendingApproval(MspTranscript, entry);
            RemoveWorkspaceTranscriptEntry(entry);
            var approved = await ExecuteAndRecordMspCommandAsync(entry.CommandText, entry.Actor, approved: true);
            SelectedMspTranscriptEntry = approved;
            StatusMessage = approved.WasCanceled
                ? $"已取消：{approved.CommandText}"
                : approved.Succeeded
                ? $"已批准并执行：{approved.CommandText}"
                : $"批准后执行失败：{approved.CommandText}";
        }
        finally
        {
            IsBusy = false;
        }
    }

    [RelayCommand]
    private async Task DenyMspCommandAsync(MspTranscriptEntry? entry)
    {
        if (!approvalReviewService.CanReview(entry) || entry is null)
        {
            return;
        }

        var index = approvalReviewService.RemovePendingApproval(MspTranscript, entry);
        RemoveWorkspaceTranscriptEntry(entry);
        var deniedEntry = approvalReviewService.CreateDeniedEntry(entry, DateTimeOffset.Now);

        var insertIndex = Math.Max(0, index);
        MspTranscript.Insert(insertIndex, deniedEntry);
        PersistTranscriptEntry(deniedEntry, insertIndex);
        SelectedMspTranscriptEntry = deniedEntry;
        await SaveWorkspaceAsync();
        StatusMessage = $"已拒绝 MSP 命令：{entry.CommandText}";
        NotifyMspActivityState();
    }

    [RelayCommand]
    private void CancelMspCommand(MspTranscriptEntry? entry)
    {
        if (!activeMspCommandService.CanCancel(entry) || entry is null)
        {
            return;
        }

        activeMspCommandService.Cancel(entry);
        entry.ProgressMessage = "正在取消 MSP 命令...";
        entry.ProgressPercent = null;
        StatusMessage = $"正在取消 MSP 命令：{entry.CommandText}";
    }

    public async Task InitializeAsync()
    {
        IsBusy = true;
        try
        {
            workspace = await workspaceStore.LoadAsync();
            LoadSettings(workspace.Settings);
            RefreshMspTranscript();
            RefreshArtifacts();
            RefreshMspSessions();
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

    partial void OnSelectedApprovalModeOptionChanged(ApprovalModeOption? value)
    {
        NotifyApprovalModeState();
        if (value is null)
        {
            return;
        }

        if (workspace is not null)
        {
            workspace.Settings.MspApprovalMode = MspApprovalModeCodes.Normalize(value.Code);
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

    partial void OnSelectedConversationChanged(ChatConversation? value)
    {
        RefreshChatMessages();
        OnPropertyChanged(nameof(ActiveTitle));
    }

    async partial void OnSelectedDocumentChanged(LibraryItem? value)
    {
        var context = activeDocumentContextService.Resolve(value);
        if (context.ShouldClearPresenter)
        {
            CurrentPageImage = null;
            PresenterTextContent = string.Empty;
        }

        CurrentPageNumber = context.CurrentPageNumber;
        if (context.DocumentNameDraft is not null)
        {
            DocumentNameDraft = context.DocumentNameDraft;
        }

        if (context.PageJumpText is not null)
        {
            PageJumpText = context.PageJumpText;
        }

        if (context.CurrentPageLabelDraft is not null)
        {
            CurrentPageLabelDraft = context.CurrentPageLabelDraft;
        }

        RefreshDocumentCollections();
        NotifyActiveContext();
        NotifyGuidedWorkflowState();
        if (!context.ShouldLoadCurrentPage)
        {
            return;
        }

        await LoadCurrentPageAsync();
        if (context.ShouldLoadThumbnails)
        {
            _ = LoadThumbnailsAsync();
        }
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
        NotifyGuidedWorkflowState();
        if (value is null || value.Page <= 0)
        {
            return;
        }

        await GoToPageAsync(value.Page);
    }

    async partial void OnSelectedSearchResultChanged(PdfTextHit? value)
    {
        var route = documentSearchService.ResolveRoute(value);
        if (!route.ShouldNavigate)
        {
            return;
        }

        await GoToPageAsync(route.PageNumber);
    }

    partial void OnSearchQueryChanged(string value)
    {
        RefreshNavigation();
        RefreshLibrary();
        RefreshMspSessions();
        RefreshArtifacts();
    }

    partial void OnComposerDraftChanged(string value)
    {
        OnPropertyChanged(nameof(HasComposerArtifactRefinementTarget));
    }

    async partial void OnReaderImageWidthChanged(double value)
    {
        if (HasPdfDocument)
        {
            await LoadCurrentPageAsync();
        }
    }

    partial void OnCurrentPageNumberChanged(int value)
    {
        OnPropertyChanged(nameof(ActiveWorkspaceScope));
    }

    partial void OnCurrentPageImageChanged(BitmapImage? value)
    {
        OnPropertyChanged(nameof(HasPageImage));
        OnPropertyChanged(nameof(IsPresenterPlaceholderVisible));
    }

    partial void OnPresenterTextContentChanged(string value)
    {
        OnPropertyChanged(nameof(HasTextPresenter));
        OnPropertyChanged(nameof(IsPresenterPlaceholderVisible));
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
        OnPropertyChanged(nameof(IsWorkspaceRoute));
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

    partial void OnWorkspaceLayoutChanged(WorkspaceLayoutMode value)
    {
        OnPropertyChanged(nameof(WorkspaceModeLabel));
    }

    partial void OnSidebarModeChanged(WorkspaceSidebarMode value)
    {
        OnPropertyChanged(nameof(IsConversationsSidebarSelected));
        OnPropertyChanged(nameof(IsMaterialsSidebarSelected));
        OnPropertyChanged(nameof(IsSessionsSidebarSelected));
        OnPropertyChanged(nameof(IsArtifactsSidebarSelected));
    }

    partial void OnSelectedInspectorTabChanged(InspectorTab value)
    {
        OnPropertyChanged(nameof(IsRunInspectorSelected));
        OnPropertyChanged(nameof(IsArtifactsInspectorSelected));
        OnPropertyChanged(nameof(IsPolicyInspectorSelected));
        OnPropertyChanged(nameof(IsContextInspectorSelected));
        OnPropertyChanged(nameof(IsActionsInspectorSelected));
        OnPropertyChanged(nameof(IsEvidenceInspectorSelected));
        OnPropertyChanged(nameof(IsAttachmentsInspectorSelected));
        OnPropertyChanged(nameof(IsOutlineInspectorSelected));
        OnPropertyChanged(nameof(IsSearchInspectorSelected));
        OnPropertyChanged(nameof(IsPreviewInspectorSelected));
    }

    partial void OnSelectedMspSessionChanged(MspSessionEntry? value)
    {
        if (value is not null)
        {
            var selection = mspSessionViewService.SelectSessionTranscript(value, MspTranscript);
            SelectedMspTranscriptEntry = selection.Transcript;
            SelectedInspectorTab = selection.InspectorTab;
            IsInspectorVisible = true;
        }
    }

    partial void OnSelectedMspTranscriptEntryChanged(MspTranscriptEntry? value)
    {
        UpdateMspTranscriptSelectionMarkers();
        OnPropertyChanged(nameof(HasSelectedFailureReviewTarget));
    }

    partial void OnSelectedArtifactChanged(WorkspaceArtifact? value)
    {
        RefreshSelectedArtifactLineage();
        OnPropertyChanged(nameof(SelectedArtifactPreview));
        NotifyGuidedWorkflowState();
        OnPropertyChanged(nameof(HasComposerArtifactRefinementTarget));
        if (value is null && isArtifactPreviewActive)
        {
            isArtifactPreviewActive = false;
            NotifyActiveContext();
        }

        if (value is not null)
        {
            SelectedInspectorTab = InspectorTab.Artifacts;
            IsInspectorVisible = true;
        }
    }

    partial void OnIsBusyChanged(bool value)
    {
        NotifyMspActivityState();
    }

    partial void OnIsChatVisibleChanged(bool value)
    {
        // IsInspectorVisible is now a standalone toggle, not dependent on chat visibility.
    }

    partial void OnIsLibraryVisibleChanged(bool value)
    {
        OnPropertyChanged(nameof(IsSidebarVisible));
    }

    partial void OnIsInspectorVisibleChanged(bool value)
    {
        OnPropertyChanged(nameof(IsPresenterVisible));
    }

    private void OnMspTranscriptCollectionChanged(object? sender, NotifyCollectionChangedEventArgs e)
    {
        NotifyMspActivityState();
    }

    private void OnPreparedMspCommandsCollectionChanged(object? sender, NotifyCollectionChangedEventArgs e)
    {
        NotifyPreparedMspCommandState();
    }

    private void OnQueuedComposerPromptsCollectionChanged(object? sender, NotifyCollectionChangedEventArgs e)
    {
        NotifyComposerQueueState();
    }

    [RelayCommand]
    private void NavigateHome()
    {
        CurrentRoute = ShellRoute.Home;
    }

    [RelayCommand]
    private void NavigateReader()
    {
        var layout = readerLayoutService.NavigateReader();
        CurrentRoute = layout.Route;
        IsInspectorVisible = layout.IsInspectorVisible;
        SelectedInspectorTab = layout.InspectorTab;
        SidebarMode = layout.SidebarMode;
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
    private void SelectApprovalMode(string modeCode)
    {
        var normalizedMode = MspApprovalModeCodes.Normalize(modeCode);
        var option = ApprovalModeOptions.FirstOrDefault(item =>
            string.Equals(item.Code, normalizedMode, StringComparison.OrdinalIgnoreCase));
        if (option is null)
        {
            StatusMessage = "未知审批模式。";
            return;
        }

        SelectedApprovalModeOption = option;
        StatusMessage = $"审批模式已切换：{option.DisplayName}";
    }

    [RelayCommand]
    private void SetWorkspaceLayout(string mode)
    {
        ApplyWorkspaceLayoutPreset(readerLayoutService.ResolveWorkspaceLayout(mode));
    }

    [RelayCommand]
    private void SwapPrimaryPane()
    {
        IsInspectorVisible = !IsInspectorVisible;
    }

    [RelayCommand]
    private void SelectSidebarMode(string mode)
    {
        var selection = readerLayoutService.SelectSidebarMode(mode);
        if (selection.ShouldApply)
        {
            SidebarMode = selection.SidebarMode;
            IsLibraryVisible = selection.IsLibraryVisible;
        }
    }

    [RelayCommand]
    private void SelectInspectorTab(string tab)
    {
        var selection = readerLayoutService.SelectInspectorTab(tab);
        if (selection.ShouldApply)
        {
            SelectedInspectorTab = selection.InspectorTab;
            IsChatVisible = selection.IsChatVisible;
        }
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
        var result = pageLabelEditingService.SaveLabel(
            workspace is not null,
            SelectedDocument,
            CurrentPageNumber,
            CurrentPageLabelDraft);
        if (!result.ShouldSave)
        {
            return;
        }

        if (result.CurrentPageLabelDraft is not null)
        {
            CurrentPageLabelDraft = result.CurrentPageLabelDraft;
        }

        await SaveWorkspaceAsync();
        StatusMessage = result.StatusMessage ?? StatusMessage;
    }

    [RelayCommand]
    private async Task AutoMapPagesAsync()
    {
        var result = pageLabelEditingService.AutoMap(workspace is not null, SelectedDocument);
        if (!result.ShouldSave)
        {
            return;
        }

        if (result.CurrentPageLabelDraft is not null)
        {
            CurrentPageLabelDraft = result.CurrentPageLabelDraft;
        }

        await SaveWorkspaceAsync();
        StatusMessage = result.StatusMessage ?? StatusMessage;
    }

    [RelayCommand]
    private async Task GenerateOutlineAsync()
    {
        if (workspace is null || SelectedDocument is null || !HasPdfDocument)
        {
            return;
        }

        IsBusy = true;
        var gen = ++_outlineGeneration;
        _outlineCts?.Cancel();
        _outlineCts?.Dispose();
        _outlineCts = new CancellationTokenSource();
        var token = _outlineCts.Token;
        try
        {
            var path = workspaceStore.GetAbsolutePath(SelectedDocument);
            var maxPage = Math.Min(SelectedDocument.PageCount, 12);
            var text = await pdfService.ExtractPageTextAsync(path, 1, maxPage, token);
            if (gen != _outlineGeneration)
            {
                return;
            }

            var result = outlineEditingService.GenerateOutline(
                workspace is not null,
                SelectedDocument,
                HasPdfDocument,
                text);
            await ApplyOutlineEditResultAsync(result);
        }
        catch (OperationCanceledException)
        {
            // 过期或被取消的请求：丢弃结果，不写状态。
        }
        catch (Exception ex)
        {
            if (gen != _outlineGeneration)
            {
                return;
            }

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
        var result = outlineEditingService.AddOutlineItem(
            workspace is not null,
            SelectedDocument,
            CurrentPageNumber,
            OutlineTitleDraft);
        await ApplyOutlineEditResultAsync(result);
    }

    [RelayCommand]
    private async Task DeleteOutlineItemAsync()
    {
        var result = outlineEditingService.DeleteOutlineItem(
            workspace is not null,
            SelectedDocument,
            SelectedOutlineItem);
        await ApplyOutlineEditResultAsync(result);
    }

    private async Task ApplyOutlineEditResultAsync(ReadOsOutlineEditResult result)
    {
        if (!result.ShouldSave)
        {
            return;
        }

        if (result.ShouldRefreshOutline)
        {
            RefreshOutline();
        }

        if (result.ShouldClearTitleDraft)
        {
            OutlineTitleDraft = string.Empty;
        }

        await SaveWorkspaceAsync();
        if (result.StatusMessage is not null)
        {
            StatusMessage = result.StatusMessage;
        }
    }

    [RelayCommand]
    private async Task SearchInDocumentAsync()
    {
        var search = documentSearchService.Prepare(
            SelectedDocument,
            HasPdfDocument,
            DocumentSearchQuery);
        if (search.ShouldClearResults)
        {
            DocumentSearchResults.Clear();
        }

        if (!search.ShouldSearch || search.Document is null)
        {
            return;
        }

        IsBusy = true;
        var gen = ++_searchGeneration;
        _searchCts?.Cancel();
        _searchCts?.Dispose();
        _searchCts = new CancellationTokenSource();
        var token = _searchCts.Token;
        try
        {
            var hits = await pdfService.SearchAsync(workspaceStore.GetAbsolutePath(search.Document), search.Query, token);
            if (gen != _searchGeneration)
            {
                return;
            }

            var result = documentSearchService.ProjectResults(hits);
            foreach (var hit in result.Hits)
            {
                DocumentSearchResults.Add(hit);
            }

            StatusMessage = result.StatusMessage;
        }
        catch (OperationCanceledException)
        {
            // 过期或被取消的请求：丢弃结果，不写状态。
        }
        catch (Exception ex)
        {
            if (gen != _searchGeneration)
            {
                return;
            }

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
        ApplyReaderAttachmentResult(readerAttachmentService.AttachCurrentPage(
            SelectedDocument,
            CurrentPageNumber,
            SelectedDocument is null ? string.Empty : workspaceStore.GetAbsolutePath(SelectedDocument)));
    }

    [RelayCommand]
    private void AttachRange()
    {
        ApplyReaderAttachmentResult(readerAttachmentService.AttachRange(
            SelectedDocument,
            SelectedDocument is null ? string.Empty : workspaceStore.GetAbsolutePath(SelectedDocument),
            PageRangeDraft));
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
        ApplyReaderAttachmentResult(readerAttachmentService.AttachRegion(
            SelectedDocument,
            CurrentPageNumber,
            x,
            y,
            width,
            height,
            RegionExplainPrompt));
    }

    private void ApplyReaderAttachmentResult(ReadOsReaderAttachmentResult result)
    {
        if (!result.ShouldApply)
        {
            if (result.StatusMessage is not null)
            {
                StatusMessage = result.StatusMessage;
            }

            return;
        }

        foreach (var attachment in result.Attachments)
        {
            PendingAttachments.Add(attachment);
        }

        if (result.PageRangeDraft is not null)
        {
            PageRangeDraft = result.PageRangeDraft;
        }

        if (result.ComposerDraft is not null)
        {
            ComposerDraft = result.ComposerDraft;
        }

        if (result.ShouldExitRegionMode)
        {
            IsRegionModeActive = false;
        }

        NotifyAttachmentState();
        if (result.StatusMessage is not null)
        {
            StatusMessage = result.StatusMessage;
        }
    }

    [RelayCommand]
    private void ClearAttachments()
    {
        attachmentQueueService.Clear(PendingAttachments);
        NotifyAttachmentState();
    }

    [RelayCommand]
    private void RemovePendingAttachment(ChatAttachment? attachment)
    {
        var existing = attachmentQueueService.RemoveById(PendingAttachments, attachment);
        if (existing is null)
        {
            return;
        }

        NotifyAttachmentState();
        StatusMessage = $"已移除附件：{existing.Title}";
    }

    [RelayCommand]
    private void QueueComposerDraft()
    {
        if (string.IsNullOrWhiteSpace(ComposerDraft) && PendingAttachments.Count == 0)
        {
            StatusMessage = "没有可加入队列的提示或附件。";
            return;
        }

        var queued = attachmentQueueService.QueueDraft(
            QueuedComposerPrompts,
            PendingAttachments,
            ComposerDraft,
            DateTimeOffset.Now);
        if (queued is null)
        {
            return;
        }

        ComposerDraft = string.Empty;
        NotifyAttachmentState();
        StatusMessage = $"已加入队列：{queued.Title}";
    }

    [RelayCommand]
    private void RestoreQueuedComposerPrompt(QueuedComposerPrompt? queued)
    {
        if (queued is null)
        {
            return;
        }

        if (!attachmentQueueService.CanRestore(ComposerDraft, PendingAttachments))
        {
            StatusMessage = "请先处理当前 composer 内容，再恢复队列项。";
            return;
        }

        ComposerDraft = attachmentQueueService.RestoreQueuedPrompt(
            QueuedComposerPrompts,
            PendingAttachments,
            queued) ?? string.Empty;
        NotifyAttachmentState();
        StatusMessage = $"已恢复队列项：{queued.Title}";
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

        var attachments = attachmentQueueService.Snapshot(PendingAttachments).ToList();
        var prompt = chatTurnService.ResolvePrompt(ComposerDraft, attachments, AttachmentDefaultPrompt);
        var userMessage = chatTurnService.CreateUserMessage(prompt, attachments, DateTimeOffset.Now);

        SelectedConversation.Messages.Add(userMessage);
        attachmentQueueService.Clear(PendingAttachments);
        ComposerDraft = string.Empty;
        RefreshChatMessages();
        NotifyAttachmentState();

        IsBusy = true;
        try
        {
            var settings = BuildSettingsFromInputs();
            var answer = await aiChatService.SendAsync(
                settings,
                SelectedDocument,
                SelectedConversation.Messages.Where(message => message.Id != userMessage.Id),
                prompt,
                attachments,
                GetAttachmentTextAsync,
                mspAgentBridgeService.BuildInstruction(),
                allowMspCommandRequests: true);

            SelectedConversation.Messages.Add(chatTurnService.CreateReadOsMessage(answer, DateTimeOffset.Now));

            var requestedCommands = mspAgentBridgeService.ExtractRequestedCommands(answer);
            if (requestedCommands.Count > 0)
            {
                var mspReport = await ExecuteAgentMspCommandsAsync(requestedCommands);
                SelectedConversation.Messages.Add(chatTurnService.CreateMspMessage(mspReport, DateTimeOffset.Now));

                var finalAnswer = await aiChatService.SendAsync(
                    settings,
                    SelectedDocument,
                    SelectedConversation.Messages,
                    chatTurnService.BuildMspFinalAnswerPrompt(prompt),
                    Array.Empty<ChatAttachment>(),
                    GetAttachmentTextAsync,
                    mspAgentBridgeService.BuildInstruction(),
                    mspReport,
                    allowMspCommandRequests: false);

                SelectedConversation.Messages.Add(chatTurnService.CreateReadOsMessage(finalAnswer, DateTimeOffset.Now));
            }

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
            var failureMessage = ex is AiChatServiceException
                ? ChatModelProviderFailureMessage
                : $"请求失败：{ex.Message}";
            SelectedConversation.Messages.Add(
                ex is AiChatServiceException
                    ? chatTurnService.CreateReadOsMessage(failureMessage, DateTimeOffset.Now)
                    : chatTurnService.CreateFailureMessage(ex, DateTimeOffset.Now));
            RefreshChatMessages();
            StatusMessage = failureMessage;
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
    private void ToggleSidebar()
    {
        IsLibraryVisible = !IsLibraryVisible;
    }

    [RelayCommand]
    private void ToggleInspector()
    {
        IsInspectorVisible = !IsInspectorVisible;
    }

    [RelayCommand]
    private void ToggleRunDrawer()
    {
        IsRunDrawerOpen = !IsRunDrawerOpen;
    }

    [RelayCommand]
    private void PinRunDrawer()
    {
        var pin = readerLayoutService.ToggleRunDrawerPin(IsRunDrawerPinned, IsRunDrawerOpen);
        IsRunDrawerPinned = pin.IsRunDrawerPinned;
        IsRunDrawerOpen = pin.IsRunDrawerOpen;
        _ = CommitRunDrawerLayoutAsync();
    }

    [RelayCommand]
    private void CancelActiveMspCommand()
    {
        var runningEntry = MspTranscript.FirstOrDefault(entry => entry.IsRunning);
        if (runningEntry is not null)
        {
            CancelMspCommand(runningEntry);
        }
    }

    [RelayCommand]
    private void OpenPendingApproval()
    {
        var navigation = pendingApprovalNavigationService.Resolve(MspTranscript);
        if (!navigation.ShouldOpen || navigation.Transcript is null)
        {
            StatusMessage = navigation.StatusMessage;
            return;
        }

        SelectedMspTranscriptEntry = navigation.Transcript;
        SelectedInspectorTab = navigation.InspectorTab;
        IsInspectorVisible = true;
        IsRunDrawerOpen = true;
        StatusMessage = navigation.StatusMessage;
    }

    // ── Pure-conversation shell overlays (re-home deferred MSP chrome) ──

    [RelayCommand]
    private void OpenHistoryFlyout()
    {
        IsHistoryFlyoutOpen = true;
    }

    [RelayCommand]
    private void OpenApprovalsFlyout()
    {
        IsApprovalsFlyoutOpen = !IsApprovalsFlyoutOpen;
    }

    [RelayCommand]
    private void SelectConversation(ChatConversation? conversation)
    {
        if (conversation is not null)
        {
            SelectedConversation = conversation;
            IsHistoryFlyoutOpen = false;
        }
    }

    [RelayCommand]
    private void CloseOverlays()
    {
        IsHistoryFlyoutOpen = false;
        IsApprovalsFlyoutOpen = false;
    }

    [RelayCommand]
    private void OpenProviderSettings()
    {
        SelectSettingsCategory("Provider");
        OpenSettings();
    }

    [RelayCommand]
    private void SelectArtifact(WorkspaceArtifact? artifact)
    {
        if (artifact is null)
        {
            return;
        }

        SelectedArtifact = artifact;
        SelectedInspectorTab = InspectorTab.Artifacts;
        IsInspectorVisible = true;
    }

    [RelayCommand]
    private void OpenArtifactLineageItem(ArtifactLineageItem? item)
    {
        var action = artifactLineageActionService.Resolve(item, workspace?.Artifacts);
        if (!action.ShouldApply)
        {
            return;
        }

        if (action.Artifact is null)
        {
            StatusMessage = action.StatusMessage ?? string.Empty;
            return;
        }

        SelectArtifact(action.Artifact);
        StatusMessage = action.StatusMessage ?? string.Empty;
    }

    [RelayCommand]
    private void OpenSelectedArtifactPreview()
    {
        var preview = artifactReuseService.PreparePreview(SelectedArtifact);
        if (!preview.Succeeded)
        {
            StatusMessage = preview.StatusMessage;
            return;
        }

        isArtifactPreviewActive = true;
        PresenterTextContent = preview.Content ?? string.Empty;
        CurrentPageImage = null;
        SelectedInspectorTab = InspectorTab.Preview;
        IsInspectorVisible = true;
        NotifyActiveContext();
        StatusMessage = preview.StatusMessage;
    }

    [RelayCommand]
    private async Task CopySelectedArtifactContentAsync()
    {
        var copy = artifactReuseService.PrepareCopy(SelectedArtifact);
        if (!copy.Succeeded)
        {
            StatusMessage = copy.StatusMessage;
            return;
        }

        await clipboardService.SetTextAsync(copy.Content ?? string.Empty);
        StatusMessage = copy.StatusMessage;
    }

    [RelayCommand]
    private async Task ExportSelectedArtifactAsync()
    {
        var export = artifactReuseService.PrepareExport(SelectedArtifact);
        if (!export.Succeeded || export.ExportMetadata is null)
        {
            StatusMessage = export.StatusMessage;
            return;
        }

        var path = await fileDialogService.PickArtifactExportAsync(
            hostWindow,
            export.ExportMetadata.SuggestedName,
            export.ExportMetadata.Extension);
        if (string.IsNullOrWhiteSpace(path))
        {
            return;
        }

        var directory = Path.GetDirectoryName(path);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        await File.WriteAllTextAsync(path, export.Content ?? string.Empty);
        StatusMessage = artifactReuseService.BuildExportedStatus(path);
    }

    [RelayCommand]
    private void PrepareExplainSectionWorkflow()
    {
        PrepareDocumentWorkflowCommand(
            "explain-section",
            "explanation",
            ".md",
            "已准备章节讲解工作流。");
    }

    [RelayCommand]
    private void PrepareExtractEvidenceWorkflow()
    {
        PrepareDocumentWorkflowCommand(
            "extract-evidence",
            "evidence",
            ".json",
            "已准备证据抽取工作流。");
    }

    [RelayCommand]
    private void PrepareReviewEvidenceWorkflow()
    {
        ApplyWorkflowPreparation(workflowPreparationService.PrepareReviewEvidenceWorkflow(SelectedArtifact));
    }

    [RelayCommand]
    private void PrepareSynthesizeEvidenceWorkflow()
    {
        ApplyWorkflowPreparation(workflowPreparationService.PrepareSynthesizeEvidenceWorkflow(SelectedArtifact));
    }

    [RelayCommand]
    private void PrepareRefineArtifactWorkflow()
    {
        ApplyWorkflowPreparation(
            workflowPreparationService.PrepareRefineArtifactWorkflow(
                SelectedArtifact,
                DefaultArtifactRefinementInstruction));
    }

    [RelayCommand]
    private void PrepareComposerArtifactRefinementWorkflow()
    {
        ApplyWorkflowPreparation(
            workflowPreparationService.PrepareComposerRefineArtifactWorkflow(SelectedArtifact, ComposerDraft));
    }

    [RelayCommand]
    private void PrepareReviewFailuresWorkflow()
    {
        ApplyWorkflowPreparation(
            workflowPreparationService.PrepareReviewFailuresWorkflow(SelectedMspTranscriptEntry));
    }

    [RelayCommand]
    private void RestorePreparedMspCommand(PreparedMspCommand? command)
    {
        if (command is null)
        {
            StatusMessage = "请选择一个已准备命令。";
            return;
        }

        MspCommandDraft = command.CommandText;
        PromotePreparedMspCommand(command);
        SelectedInspectorTab = InspectorTab.Run;
        IsInspectorVisible = true;
        StatusMessage = $"已恢复命令草稿：{command.Title}";
    }

    [RelayCommand]
    private void OpenTimelineItem(ThreadTimelineItem? item)
    {
        var action = timelineActionService.Resolve(item);
        if (!action.ShouldOpenInspector)
        {
            return;
        }

        IsInspectorVisible = true;
        if (action.Artifact is not null)
        {
            SelectArtifact(action.Artifact);
        }
        else
        {
            if (action.Transcript is not null)
            {
                SelectedMspTranscriptEntry = action.Transcript;
            }

            SelectedInspectorTab = action.InspectorTab;
        }

        if (!string.IsNullOrWhiteSpace(action.StatusMessage))
        {
            StatusMessage = action.StatusMessage;
        }
    }

    [RelayCommand]
    private void AttachSelectedArtifact()
    {
        var attachment = artifactReuseService.PrepareAttachment(SelectedArtifact, SelectedDocument?.Id);
        if (!attachment.Succeeded || attachment.Attachment is null)
        {
            StatusMessage = attachment.StatusMessage;
            return;
        }

        PendingAttachments.Add(attachment.Attachment);
        ComposerDraft = string.IsNullOrWhiteSpace(ComposerDraft)
            ? attachment.DefaultComposerPrompt
            : ComposerDraft;
        NotifyAttachmentState();
        SelectedInspectorTab = InspectorTab.Evidence;
        StatusMessage = attachment.StatusMessage;
    }

    [RelayCommand]
    private void ToggleChat()
    {
        IsChatVisible = !IsChatVisible;
    }

    [RelayCommand]
    private void ToggleOutline()
    {
        var toggle = readerLayoutService.ToggleOutline(IsOutlineVisible);
        IsOutlineVisible = toggle.IsOutlineVisible;
        SelectedInspectorTab = toggle.InspectorTab;
        IsChatVisible = toggle.IsChatVisible;
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
            RefreshMspTranscript();
            RefreshArtifacts();
            RefreshMspSessions();
            RefreshProjects();
        await SelectProjectAsync(workspace.Projects.First(), workspace.Projects.First().LibraryItems.FirstOrDefault());
        StatusMessage = "工作区已导入并重新加载。";
    }

    public void ApplyWorkspaceLayoutPreset(WorkspaceLayoutMode layout)
    {
        ApplyWorkspaceLayoutPreset(readerLayoutService.ApplyWorkspaceLayout(layout));
    }

    private void ApplyWorkspaceLayoutPreset(ReadOsWorkspaceLayoutPreset preset)
    {
        if (!preset.ShouldApply)
        {
            return;
        }

        WorkspaceLayout = preset.Layout;
        IsInspectorVisible = preset.IsInspectorVisible;
        if (preset.InspectorTab is not null)
        {
            SelectedInspectorTab = preset.InspectorTab.Value;
        }
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
        var navigation = pageNavigationService.Prepare(workspace is not null, SelectedDocument, page);
        if (!navigation.ShouldNavigate || SelectedDocument is null)
        {
            return;
        }

        CurrentPageNumber = navigation.TargetPage;
        SelectedDocument.CurrentPage = navigation.TargetPage;
        PageJumpText = navigation.PageJumpText ?? navigation.TargetPage.ToString();
        CurrentPageLabelDraft = navigation.CurrentPageLabelDraft ?? SelectedDocument.CurrentPageLabel;
        if (navigation.ShouldLoadCurrentPage)
        {
            await LoadCurrentPageAsync();
        }

        if (navigation.ShouldSaveWorkspace)
        {
            await SaveWorkspaceAsync();
        }
    }

    private async Task LoadCurrentPageAsync()
    {
        var preparation = presenterLoadPreparationService.Prepare(SelectedDocument, CurrentPageNumber);
        if (preparation.ShouldDeactivateArtifactPreview)
        {
            isArtifactPreviewActive = false;
        }

        if (preparation.ShouldClearCurrentPageImage)
        {
            CurrentPageImage = null;
        }

        if (preparation.ShouldClearPresenterText)
        {
            PresenterTextContent = string.Empty;
        }

        if (preparation.Mode == ReadOsPresenterLoadMode.Clear)
        {
            if (preparation.ShouldNotifyActiveContext)
            {
                NotifyActiveContext();
            }

            return;
        }

        if (preparation.ShouldLoadText && preparation.Document is not null)
        {
            await LoadPresenterTextAsync(preparation.Document);
            if (preparation.ShouldNotifyActiveContext)
            {
                NotifyActiveContext();
            }

            return;
        }

        if (preparation.ShouldRenderPdfPage && preparation.Document is not null)
        {
            var gen = ++_pageLoadGeneration;
            _pageCts?.Cancel();
            _pageCts?.Dispose();
            _pageCts = new CancellationTokenSource();
            var token = _pageCts.Token;
            try
            {
                CurrentPageImage = await pdfService.RenderPageAsync(
                    workspaceStore.GetAbsolutePath(preparation.Document),
                    preparation.PageNumber,
                    ReaderImageWidth,
                    token);
                if (gen != _pageLoadGeneration)
                {
                    return;
                }

                CurrentPageLabelDraft = preparation.Document.CurrentPageLabel;
            }
            catch (OperationCanceledException)
            {
                // 过期或被取消的请求：丢弃结果，不写状态。
            }
            catch (Exception ex)
            {
                if (gen != _pageLoadGeneration)
                {
                    return;
                }

                CurrentPageImage = null;
                StatusMessage = $"页面渲染失败：{ex.Message}";
            }
        }

        if (preparation.ShouldRefreshPageSignals)
        {
            OnPropertyChanged(nameof(HasPageImage));
            OnPropertyChanged(nameof(CurrentPageIndicator));
        }
    }

    private async Task LoadPresenterTextAsync(LibraryItem document)
    {
        var gen = ++_presenterLoadGeneration;
        _presenterCts?.Cancel();
        _presenterCts?.Dispose();
        _presenterCts = new CancellationTokenSource();
        var token = _presenterCts.Token;
        try
        {
            var path = workspaceStore.GetAbsolutePath(document);
            var text = File.Exists(path)
                ? await File.ReadAllTextAsync(path, token)
                : string.Empty;
            if (gen != _presenterLoadGeneration)
            {
                return;
            }

            PresenterTextContent = text;
        }
        catch (OperationCanceledException)
        {
            // 过期或被取消的请求：丢弃结果，不写状态。
        }
        catch (Exception ex)
        {
            if (gen != _presenterLoadGeneration)
            {
                return;
            }

            PresenterTextContent = string.Empty;
            StatusMessage = $"文本加载失败：{ex.Message}";
        }
    }

    private async Task LoadThumbnailsAsync()
    {
        var preparation = thumbnailLoadPreparationService.Prepare(SelectedDocument, CurrentPageNumber);
        if (!preparation.ShouldLoad || preparation.Document is null)
        {
            if (preparation.ShouldClearThumbnails)
            {
                Thumbnails.Clear();
            }

            return;
        }

        var gen = ++_thumbnailGeneration;
        _thumbCts?.Cancel();
        _thumbCts?.Dispose();
        _thumbCts = new CancellationTokenSource();
        var token = _thumbCts.Token;
        try
        {
            var thumbnails = await pdfService.RenderThumbnailsAsync(
                workspaceStore.GetAbsolutePath(preparation.Document),
                preparation.PageCount,
                preparation.MaxPages,
                token);
            if (gen != _thumbnailGeneration)
            {
                return;
            }

            var projection = thumbnailLoadPreparationService.Project(
                preparation.Document,
                thumbnails,
                preparation.SelectedPageNumber);

            Thumbnails.Clear();
            foreach (var thumbnail in projection.Thumbnails)
            {
                Thumbnails.Add(thumbnail);
            }

            suppressThumbnailSelection = true;
            SelectedThumbnail = projection.SelectedThumbnail;
            suppressThumbnailSelection = false;
        }
        catch (OperationCanceledException)
        {
            // 过期或被取消的请求：丢弃结果，不写状态。
        }
        catch (Exception ex)
        {
            if (gen != _thumbnailGeneration)
            {
                return;
            }

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
        var refresh = documentCollectionRefreshService.Build(SelectedDocument, SelectedProject);
        RefreshOutline(refresh.OutlineItems);
        RefreshConversations(refresh.Conversations);
        OnPropertyChanged(nameof(HasDocument));
        OnPropertyChanged(nameof(HasPdfDocument));
        OnPropertyChanged(nameof(CurrentPageIndicator));
    }

    private void RefreshOutline()
    {
        RefreshOutline(documentCollectionRefreshService.GetOutlineItems(SelectedDocument));
    }

    private void RefreshOutline(IEnumerable<OutlineItem> outlineItems)
    {
        Outline.Clear();
        foreach (var item in outlineItems)
        {
            Outline.Add(item);
        }
    }

    private void RefreshConversations()
    {
        RefreshConversations(conversationService.GetVisibleConversations(SelectedDocument, SelectedProject));
    }

    private void RefreshConversations(IEnumerable<ChatConversation> conversations)
    {
        Conversations.Clear();
        foreach (var conversation in conversations)
        {
            Conversations.Add(conversation);
        }

        SelectedConversation = Conversations.FirstOrDefault();
        RefreshChatMessages();
    }

    private void RefreshChatMessages()
    {
        ChatMessages.Clear();
        if (SelectedConversation is null)
        {
            RefreshTimelineItems();
            return;
        }

        foreach (var message in conversationService.GetMessages(SelectedConversation))
        {
            ChatMessages.Add(message);
        }

        RefreshTimelineItems();
    }

    private void RefreshArtifacts()
    {
        Artifacts.Clear();
        if (workspace is null)
        {
            NotifyArtifactState();
            RefreshTimelineItems();
            return;
        }

        foreach (var artifact in artifactService.GetVisibleArtifacts(workspace, SearchQuery))
        {
            Artifacts.Add(artifact);
        }

        if (SelectedArtifact is not null &&
            Artifacts.All(item => item.Id != SelectedArtifact.Id))
        {
            SelectedArtifact = null;
        }

        RefreshSelectedArtifactLineage();
        NotifyArtifactState();
        RefreshTimelineItems();
    }

    private void RefreshSelectedArtifactLineage()
    {
        SelectedArtifactLineage.Clear();

        var workspaceArtifacts = workspace?.Artifacts ?? Enumerable.Empty<WorkspaceArtifact>();
        foreach (var item in artifactService.BuildLineage(SelectedArtifact, workspaceArtifacts))
        {
            SelectedArtifactLineage.Add(item);
        }

        OnPropertyChanged(nameof(HasSelectedArtifactLineage));
    }

    private void PrepareDocumentWorkflowCommand(
        string workflowName,
        string artifactSuffix,
        string extension,
        string statusMessage)
    {
        ApplyWorkflowPreparation(
            workflowPreparationService.PrepareDocumentWorkflow(
                SelectedDocument,
                SelectedOutlineItem,
                workflowName,
                artifactSuffix,
                extension,
                statusMessage));
    }

    private void ApplyWorkflowPreparation(ReadOsWorkflowPreparationResult result)
    {
        if (!result.Succeeded || string.IsNullOrWhiteSpace(result.CommandText))
        {
            StatusMessage = result.StatusMessage;
            return;
        }

        PrepareMspCommandDraft(result.CommandText, result.StatusMessage);
    }

    private void PrepareMspCommandDraft(string commandText, string statusMessage)
    {
        MspCommandDraft = commandText;
        RememberPreparedMspCommand(commandText, statusMessage);
        SelectedInspectorTab = InspectorTab.Run;
        IsInspectorVisible = true;
        StatusMessage = statusMessage;
    }

    private void RememberPreparedMspCommand(string commandText, string statusMessage)
    {
        preparedCommandHistoryService.Record(
            PreparedMspCommands,
            commandText,
            statusMessage,
            DateTimeOffset.Now);
    }

    private void PromotePreparedMspCommand(PreparedMspCommand command)
    {
        preparedCommandHistoryService.Promote(PreparedMspCommands, command);
    }

    private void RefreshMspSessions()
    {
        MspSessions.Clear();
        if (workspace is null)
        {
            return;
        }

        foreach (var session in mspSessionViewService.GetVisibleSessions(workspace.MspSessions, SearchQuery))
        {
            MspSessions.Add(session);
        }

        if (mspSessionViewService.ShouldClearSelection(SelectedMspSession, MspSessions))
        {
            SelectedMspSession = null;
        }
    }

    private void RefreshTimelineItems()
    {
        TimelineItems.Clear();
        foreach (var item in timelineService.BuildTimeline(ChatMessages, MspTranscript, Artifacts))
        {
            TimelineItems.Add(item);
        }

        var canonical = chatUiProjectionService.BuildTimeline(ChatMessages, MspTranscript, Artifacts);
        ChatTimeline.ApplyTimeline(canonical);
    }

    // Host bridge callback: copy the markdown text of the requested message id.
    // The renderer calls bridge.CopyMessage(messageId) when the user clicks the
    // "copy" action. We resolve the message from the canonical timeline and
    // write its concatenated markdown/tool text to the clipboard.
    private void OnChatUiMessageCopyRequested(object? sender, string messageId)
    {
        var timeline = ChatTimeline.Current;
        if (timeline is null) return;

        var message = timeline.Messages.FirstOrDefault(m => m.Id == messageId);
        if (message is null) return;

        var sb = new System.Text.StringBuilder();
        foreach (var block in message.Blocks)
        {
            if (block is Models.ChatUi.ChatUiMarkdownBlock md)
            {
                if (sb.Length > 0) sb.AppendLine();
                sb.Append(md.Text);
            }
            else if (block is Models.ChatUi.ChatUiToolCallBlock tc)
            {
                if (sb.Length > 0) sb.AppendLine();
                sb.Append($"[{tc.ToolName}] {tc.Title}");
                if (!string.IsNullOrEmpty(tc.OutputText)) sb.Append(tc.OutputText);
            }
        }

        if (sb.Length > 0)
        {
            _ = clipboardService.SetTextAsync(sb.ToString());
            StatusMessage = "已复制消息内容";
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
        var result = conversationService.EnsureConversation(
            SelectedDocument,
            SelectedProject,
            SelectedConversation,
            forceNew,
            DateTimeOffset.Now);
        if (!result.ShouldRefresh)
        {
            return;
        }

        RefreshConversations();
        if (result.CreatedConversation is not null)
        {
            SelectedConversation = result.CreatedConversation;
            RefreshChatMessages();
        }
    }

    private async Task<string> GetAttachmentTextAsync(ChatAttachment attachment)
    {
        var document = FindDocument(attachment.DocumentId);
        if (attachment.Kind == AttachmentKind.File)
        {
            if (!string.IsNullOrWhiteSpace(attachment.FilePath) &&
                attachment.FilePath.StartsWith("/artifacts/", StringComparison.OrdinalIgnoreCase))
            {
                return workspace?.Artifacts.FirstOrDefault(item =>
                    string.Equals(item.Path, attachment.FilePath, StringComparison.OrdinalIgnoreCase))?.Content ?? string.Empty;
            }

            var path = document is null ? string.Empty : workspaceStore.GetAbsolutePath(document);
            return File.Exists(path) ? await File.ReadAllTextAsync(path) : string.Empty;
        }

        if (document is null)
        {
            return string.Empty;
        }

        if (document.Kind is LibraryItemKind.Markdown or LibraryItemKind.Note)
        {
            var path = workspaceStore.GetAbsolutePath(document);
            return File.Exists(path) ? await File.ReadAllTextAsync(path) : string.Empty;
        }

        if (document.Kind != LibraryItemKind.Pdf)
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
        return pageNavigationService.ResolvePage(SelectedDocument, text);
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
            MspApprovalMode = ApprovalModeCode,
            AttachmentDefaultPrompt = AttachmentDefaultPrompt,
            RegionExplainPrompt = RegionExplainPrompt,
            ChapterExplainPrompt = ChapterExplainPrompt,
            MinorUEndpoint = MinorUEndpoint,
            RunDrawerHeight = RunDrawerHeight,
            IsRunDrawerPinned = IsRunDrawerPinned
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
        var approvalMode = MspApprovalModeCodes.Normalize(settings.MspApprovalMode);
        SelectedApprovalModeOption = ApprovalModeOptions.FirstOrDefault(item => item.Code == approvalMode) ?? ApprovalModeOptions.First();
        AttachmentDefaultPrompt = settings.AttachmentDefaultPrompt;
        RegionExplainPrompt = settings.RegionExplainPrompt;
        ChapterExplainPrompt = settings.ChapterExplainPrompt;
        MinorUEndpoint = settings.MinorUEndpoint;

        RunDrawerHeight = settings.RunDrawerHeight;
        IsRunDrawerPinned = settings.IsRunDrawerPinned;

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

    /// <summary>
    /// Persists the current Run Drawer layout (height + pin) after an interactive
    /// resize or pin toggle. Called on drag-completed and on pin change.
    /// </summary>
    public async Task CommitRunDrawerLayoutAsync()
    {
        if (workspace is null)
        {
            return;
        }

        workspace.Settings.RunDrawerHeight = RunDrawerHeight;
        workspace.Settings.IsRunDrawerPinned = IsRunDrawerPinned;
        await SaveWorkspaceAsync();
    }

    public void OpenMspTranscriptDetail(MspTranscriptEntry entry)
    {
        if (entry is null)
        {
            return;
        }

        SelectedMspTranscriptEntry = entry;
        SelectedInspectorTab = InspectorTab.Run;
        IsInspectorVisible = true;
    }

    private void NotifyActiveContext()
    {
        OnPropertyChanged(nameof(ActiveTitle));
        OnPropertyChanged(nameof(ActiveSubtitle));
        OnPropertyChanged(nameof(HasDocument));
        OnPropertyChanged(nameof(HasPdfDocument));
        OnPropertyChanged(nameof(HasPageImage));
        OnPropertyChanged(nameof(CurrentPageIndicator));
        OnPropertyChanged(nameof(PresenterKindLabel));
        OnPropertyChanged(nameof(ActiveWorkspaceScope));
        OnPropertyChanged(nameof(HasTextPresenter));
        OnPropertyChanged(nameof(IsPresenterPlaceholderVisible));
    }

    private void NotifyAttachmentState()
    {
        OnPropertyChanged(nameof(PendingAttachmentSummary));
        OnPropertyChanged(nameof(SendButtonLabel));
    }

    private void NotifyComposerQueueState()
    {
        OnPropertyChanged(nameof(ComposerQueueSummary));
        OnPropertyChanged(nameof(HasQueuedComposerPrompts));
    }

    private void NotifyApprovalModeState()
    {
        OnPropertyChanged(nameof(ApprovalModeCode));
        OnPropertyChanged(nameof(ApprovalModeLabel));
        OnPropertyChanged(nameof(ApprovalModeDetail));
        OnPropertyChanged(nameof(IsPolicyApprovalModeSelected));
        OnPropertyChanged(nameof(IsConfirmAllApprovalModeSelected));
        OnPropertyChanged(nameof(IsAllowWorkspaceApprovalModeSelected));
    }

    private void NotifyPreparedMspCommandState()
    {
        OnPropertyChanged(nameof(HasPreparedMspCommands));
    }

    private void NotifyArtifactState()
    {
        OnPropertyChanged(nameof(ArtifactSummary));
        OnPropertyChanged(nameof(SelectedArtifactPreview));
        OnPropertyChanged(nameof(HasSelectedArtifactLineage));
        NotifyGuidedWorkflowState();
    }

    private void NotifyGuidedWorkflowState()
    {
        OnPropertyChanged(nameof(HasSelectedOutlineWorkflowTarget));
        OnPropertyChanged(nameof(HasSelectedArtifactWorkflowTarget));
        OnPropertyChanged(nameof(HasComposerArtifactRefinementTarget));
        OnPropertyChanged(nameof(HasSelectedEvidenceArtifact));
    }

    private void NotifyMspActivityState()
    {
        OnPropertyChanged(nameof(MspTranscriptSummary));
        OnPropertyChanged(nameof(PendingApprovalCount));
        OnPropertyChanged(nameof(HasPendingApprovals));
        OnPropertyChanged(nameof(PendingApprovalLabel));
        OnPropertyChanged(nameof(MspActivitySummary));
        OnPropertyChanged(nameof(RuntimeStatusLabel));
        OnPropertyChanged(nameof(SendButtonLabel));
        OnPropertyChanged(nameof(ComposerPrimaryGlyph));
        OnPropertyChanged(nameof(ComposerPrimaryToolTip));
        OnPropertyChanged(nameof(ComposerPrimaryCommand));
        OnPropertyChanged(nameof(IsAnyMspRunning));
    }

    private void QueueMspAttachment(ChatAttachment attachment)
    {
        PendingAttachments.Add(attachment);
        NotifyAttachmentState();
        SelectedInspectorTab = InspectorTab.Evidence;
        StatusMessage = $"MSP 已加入附件：{attachment.Title}";
    }

    private void ClearMspAttachments()
    {
        attachmentQueueService.Clear(PendingAttachments);
        NotifyAttachmentState();
    }

    private void ApplyMspChatResult(LibraryItem document, ChatConversation conversation)
    {
        if (SelectedDocument?.Id == document.Id)
        {
            RefreshConversations();
            SelectedConversation = Conversations.FirstOrDefault(item => item.Id == conversation.Id) ?? SelectedConversation;
            RefreshChatMessages();
        }

        StatusMessage = $"MSP chat 已写入：{conversation.Title}";
    }

    private async Task<MspTranscriptEntry> ExecuteAndRecordMspCommandAsync(
        string commandText,
        string actor,
        bool approved = false,
        CancellationToken cancellationToken = default)
    {
        var startedAt = DateTimeOffset.Now;
        var entry = mspTranscriptService.CreateRunningEntry(commandText, actor, startedAt);

        MspTranscript.Insert(0, entry);
        RefreshTimelineItems();
        NotifyMspActivityState();
        SelectedInspectorTab = InspectorTab.Run;
        IsRunDrawerOpen = true;
        var activeCommand = activeMspCommandService.Begin(entry.Id, cancellationToken);

        MspCommandResult? result = null;
        try
        {
            var stream = approved
                ? ExecuteApprovedMspCommandStreamingAsync(commandText, actor, activeCommand.Token)
                : ExecuteMspCommandStreamingAsync(commandText, actor, activeCommand.Token);
            await foreach (var commandEvent in stream)
            {
                ApplyMspCommandEvent(entry, commandEvent);
                result = commandEvent.Result ?? result;
            }
        }
        catch (OperationCanceledException)
        {
            result = MspCommandResult.Failure(
                "Operator canceled MSP command.",
                exitCode: 130,
                code: "msp.canceled",
                recoveryHint: "Rerun the command if the canceled operation is still needed.");
            mspTranscriptService.MarkOperatorCanceled(entry);
        }
        catch (Exception ex)
        {
            result = MspCommandResult.Failure(
                ex.Message,
                code: "msp.workbench.exception",
                recoveryHint: "Inspect the command, workspace state, and provider settings before retrying.");
        }
        finally
        {
            activeCommand.Dispose();
        }

        result ??= MspCommandResult.Failure("MSP command did not return a result.");
        mspTranscriptService.CompleteEntry(entry, result, DateTimeOffset.Now);
        PersistTranscriptEntry(entry, 0);
        RefreshArtifacts();
        RebuildAllMspSessions();
        RefreshMspSessions();
        RefreshTimelineItems();
        await SaveWorkspaceAsync();
        NotifyMspActivityState();
        return entry;
    }

    private void ApplyMspCommandEvent(MspTranscriptEntry entry, MspCommandEvent commandEvent)
    {
        var projection = mspTranscriptService.ApplyEvent(entry, commandEvent);
        if (!string.IsNullOrWhiteSpace(projection.StatusMessage))
        {
            StatusMessage = projection.StatusMessage;
        }

        if (projection.ShouldOpenPolicy)
        {
            IsRunDrawerOpen = true;
            SelectedInspectorTab = InspectorTab.Policy;
        }

        if (projection.ShouldRefreshTimeline)
        {
            RefreshTimelineItems();
        }

        NotifyMspActivityState();
    }
    private void RefreshMspTranscript()
    {
        MspTranscript.Clear();
        var entries = mspTranscriptWorkspaceService.RefreshTranscript(workspace);
        foreach (var entry in entries)
        {
            MspTranscript.Add(entry);
        }

        UpdateMspTranscriptSelectionMarkers();
        RefreshMspSessions();
        RefreshTimelineItems();
        NotifyMspActivityState();
    }

    private void UpdateMspTranscriptSelectionMarkers()
    {
        var selectedId = SelectedMspTranscriptEntry?.Id;
        foreach (var entry in MspTranscript)
        {
            entry.IsInspectorSelected = !string.IsNullOrWhiteSpace(selectedId) &&
                string.Equals(entry.Id, selectedId, StringComparison.OrdinalIgnoreCase);
        }

        if (SelectedMspTranscriptEntry is not null &&
            MspTranscript.All(entry => !string.Equals(entry.Id, SelectedMspTranscriptEntry.Id, StringComparison.OrdinalIgnoreCase)))
        {
            SelectedMspTranscriptEntry.IsInspectorSelected = true;
        }
    }

    private void PersistTranscriptEntry(MspTranscriptEntry entry, int index)
    {
        if (!mspTranscriptWorkspaceService.PersistTranscriptEntry(workspace, entry, index))
        {
            return;
        }

        RefreshMspSessions();
        RefreshTimelineItems();
        NotifyMspActivityState();
    }

    private void RemoveWorkspaceTranscriptEntry(MspTranscriptEntry entry)
    {
        if (mspTranscriptWorkspaceService.RemoveTranscriptEntry(workspace, entry))
        {
            RefreshMspSessions();
            RefreshTimelineItems();
            NotifyMspActivityState();
        }
    }

    private void RebuildAllMspSessions()
    {
        mspTranscriptWorkspaceService.RebuildAllSessions(workspace);
    }

    private async Task<string> ExecuteAgentMspCommandsAsync(IReadOnlyList<string> commandTexts)
    {
        var entries = new List<MspTranscriptEntry>();
        foreach (var commandText in commandTexts)
        {
            var entry = await ExecuteAndRecordMspCommandAsync(commandText, "reados-agent");
            entries.Add(entry);
        }

        StatusMessage = $"已执行 {commandTexts.Count} 条 MSP 命令。";
        SelectedInspectorTab = InspectorTab.Run;
        return mspAgentBridgeService.BuildExecutionReport(entries);
    }

    public void Dispose()
    {
        mspHost.Dispose();
        GC.SuppressFinalize(this);
    }

}
