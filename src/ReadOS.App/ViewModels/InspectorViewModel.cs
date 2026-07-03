using System.Collections.ObjectModel;
using System.ComponentModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Microsoft.UI.Xaml.Media.Imaging;
using ReadOS.App.Models;
using ReadOS.App.Services;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.ViewModels;

public enum InspectorTab
{
    Context,
    Actions,
    Evidence,
    Attachments,
    Outline,
    Search,
    Preview
}

public enum PresenterKind
{
    None,
    Pdf,
    Markdown,
    Text,
    ImagePlaceholder,
    VideoPlaceholder,
    DocumentPlaceholder
}

public sealed partial class InspectorViewModel : ObservableObject
{
    private readonly ShellViewModel shell;

    public InspectorViewModel(ShellViewModel shell)
    {
        this.shell = shell;
        shell.PropertyChanged += OnShellPropertyChanged;
    }

    // ── Collections ─────────────────────────────────────────────────────

    public ObservableCollection<MspTranscriptEntry> MspTranscript => shell.MspTranscript;
    public ObservableCollection<PdfTextHit> DocumentSearchResults => shell.DocumentSearchResults;
    public ObservableCollection<OutlineItem> Outline => shell.Outline;
    public ObservableCollection<PageImageItem> Thumbnails => shell.Thumbnails;

    // ── State (pass-through to shell) ───────────────────────────────────

    public InspectorTab SelectedInspectorTab
    {
        get => shell.SelectedInspectorTab;
        set => shell.SelectedInspectorTab = value;
    }

    public string MspCommandDraft
    {
        get => shell.MspCommandDraft;
        set => shell.MspCommandDraft = value;
    }

    public string DocumentSearchQuery
    {
        get => shell.DocumentSearchQuery;
        set => shell.DocumentSearchQuery = value;
    }

    public string PageJumpText
    {
        get => shell.PageJumpText;
        set => shell.PageJumpText = value;
    }

    public int CurrentPageNumber
    {
        get => shell.CurrentPageNumber;
        set => shell.CurrentPageNumber = value;
    }

    public BitmapImage? CurrentPageImage
    {
        get => shell.CurrentPageImage;
        set => shell.CurrentPageImage = value;
    }

    public double ReaderImageWidth
    {
        get => shell.ReaderImageWidth;
        set => shell.ReaderImageWidth = value;
    }

    public string PresenterTextContent
    {
        get => shell.PresenterTextContent;
        set => shell.PresenterTextContent = value;
    }

    public bool IsRegionModeActive
    {
        get => shell.IsRegionModeActive;
        set => shell.IsRegionModeActive = value;
    }

    public string DocumentNameDraft
    {
        get => shell.DocumentNameDraft;
        set => shell.DocumentNameDraft = value;
    }

    public string OutlineTitleDraft
    {
        get => shell.OutlineTitleDraft;
        set => shell.OutlineTitleDraft = value;
    }

    public string CurrentPageLabelDraft
    {
        get => shell.CurrentPageLabelDraft;
        set => shell.CurrentPageLabelDraft = value;
    }

    public string PageRangeDraft
    {
        get => shell.PageRangeDraft;
        set => shell.PageRangeDraft = value;
    }

    public OutlineItem? SelectedOutlineItem
    {
        get => shell.SelectedOutlineItem;
        set => shell.SelectedOutlineItem = value;
    }

    public PageImageItem? SelectedThumbnail
    {
        get => shell.SelectedThumbnail;
        set => shell.SelectedThumbnail = value;
    }

    public PdfTextHit? SelectedSearchResult
    {
        get => shell.SelectedSearchResult;
        set => shell.SelectedSearchResult = value;
    }

    public LibraryItem? SelectedDocument => shell.SelectedDocument;

    // ── Computed ────────────────────────────────────────────────────────

    public bool IsContextInspectorSelected => SelectedInspectorTab == InspectorTab.Context;
    public bool IsActionsInspectorSelected => SelectedInspectorTab == InspectorTab.Actions;
    public bool IsEvidenceInspectorSelected => SelectedInspectorTab == InspectorTab.Evidence;
    public bool IsAttachmentsInspectorSelected => SelectedInspectorTab == InspectorTab.Attachments;
    public bool IsOutlineInspectorSelected => SelectedInspectorTab == InspectorTab.Outline;
    public bool IsSearchInspectorSelected => SelectedInspectorTab == InspectorTab.Search;
    public bool IsPreviewInspectorSelected => SelectedInspectorTab == InspectorTab.Preview;

    public string MspTranscriptSummary => shell.MspTranscriptSummary;
    public string CurrentPageIndicator => shell.CurrentPageIndicator;
    public string PresenterKindLabel => shell.PresenterKindLabel;
    public string ActiveModelLabel => shell.ActiveModelLabel;
    public string ActiveTitle => shell.ActiveTitle;
    public string ActiveSubtitle => shell.ActiveSubtitle;
    public string ActiveWorkspaceScope => shell.ActiveWorkspaceScope;
    public string PendingAttachmentSummary => shell.PendingAttachmentSummary;
    public string StatusMessage => shell.StatusMessage;
    public bool IsBusy => shell.IsBusy;
    public bool HasDocument => shell.HasDocument;
    public bool HasPdfDocument => shell.HasPdfDocument;
    public bool HasPageImage => shell.HasPageImage;
    public bool HasTextPresenter => shell.HasTextPresenter;
    public bool IsPresenterPlaceholderVisible => shell.IsPresenterPlaceholderVisible;

    // ── Commands (delegate to shell's public command properties) ────────

    [RelayCommand]
    private void SelectInspectorTab(string tab)
    {
        if (Enum.TryParse<InspectorTab>(tab, ignoreCase: true, out var inspectorTab))
        {
            SelectedInspectorTab = inspectorTab;
            shell.IsInspectorVisible = true;
        }
    }

    [RelayCommand]
    private void ToggleInspector() => shell.IsInspectorVisible = !shell.IsInspectorVisible;

    [RelayCommand]
    private void SetWorkspaceLayout(string mode) => shell.SetWorkspaceLayoutCommand.Execute(mode);

    public IAsyncRelayCommand RunMspCommandCommand => shell.RunMspCommandCommand;
    public IAsyncRelayCommand ApproveMspCommandCommand => shell.ApproveMspCommandCommand;
    public IAsyncRelayCommand DenyMspCommandCommand => shell.DenyMspCommandCommand;
    public IRelayCommand CancelMspCommandCommand => shell.CancelMspCommandCommand;
    public IAsyncRelayCommand SearchInDocumentCommand => shell.SearchInDocumentCommand;
    public IAsyncRelayCommand PreviousPageCommand => shell.PreviousPageCommand;
    public IAsyncRelayCommand NextPageCommand => shell.NextPageCommand;
    public IAsyncRelayCommand JumpToPageCommand => shell.JumpToPageCommand;
    public IAsyncRelayCommand SavePageLabelCommand => shell.SavePageLabelCommand;
    public IAsyncRelayCommand AutoMapPagesCommand => shell.AutoMapPagesCommand;
    public IAsyncRelayCommand GenerateOutlineCommand => shell.GenerateOutlineCommand;
    public IAsyncRelayCommand AddOutlineItemCommand => shell.AddOutlineItemCommand;
    public IAsyncRelayCommand DeleteOutlineItemCommand => shell.DeleteOutlineItemCommand;
    public IAsyncRelayCommand RenameDocumentCommand => shell.RenameDocumentCommand;
    public IAsyncRelayCommand DeleteDocumentCommand => shell.DeleteDocumentCommand;
    public IRelayCommand AttachCurrentPageCommand => shell.AttachCurrentPageCommand;
    public IRelayCommand AttachRangeCommand => shell.AttachRangeCommand;
    public IRelayCommand StartRegionSelectionCommand => shell.StartRegionSelectionCommand;
    public IRelayCommand ClearAttachmentsCommand => shell.ClearAttachmentsCommand;
    public IRelayCommand NewConversationCommand => shell.NewConversationCommand;
    public IAsyncRelayCommand ClearConversationCommand => shell.ClearConversationCommand;
    public IRelayCommand NavigateSettingsCommand => shell.NavigateSettingsCommand;
    public IRelayCommand ImportMaterialCommand => shell.ImportMaterialCommand;
    public IAsyncRelayCommand ExportWorkspaceCommand => shell.ExportWorkspaceCommand;
    public IAsyncRelayCommand ImportWorkspaceCommand => shell.ImportWorkspaceCommand;
    public IRelayCommand OpenLocationCommand => shell.OpenLocationCommand;

    // ── Region methods delegated to shell ───────────────────────────────

    public void AttachRegionSelection(double x, double y, double width, double height)
        => shell.AttachRegionSelection(x, y, width, height);

    // ── Property change forwarding ──────────────────────────────────────

    private void OnShellPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        switch (e.PropertyName)
        {
            case nameof(ShellViewModel.SelectedInspectorTab):
                OnPropertyChanged(nameof(SelectedInspectorTab));
                OnPropertyChanged(nameof(IsContextInspectorSelected));
                OnPropertyChanged(nameof(IsActionsInspectorSelected));
                OnPropertyChanged(nameof(IsEvidenceInspectorSelected));
                OnPropertyChanged(nameof(IsAttachmentsInspectorSelected));
                OnPropertyChanged(nameof(IsOutlineInspectorSelected));
                OnPropertyChanged(nameof(IsSearchInspectorSelected));
                OnPropertyChanged(nameof(IsPreviewInspectorSelected));
                break;
            case nameof(ShellViewModel.MspCommandDraft):
                OnPropertyChanged(nameof(MspCommandDraft)); break;
            case nameof(ShellViewModel.DocumentSearchQuery):
                OnPropertyChanged(nameof(DocumentSearchQuery)); break;
            case nameof(ShellViewModel.PageJumpText):
                OnPropertyChanged(nameof(PageJumpText)); break;
            case nameof(ShellViewModel.CurrentPageNumber):
                OnPropertyChanged(nameof(CurrentPageNumber)); break;
            case nameof(ShellViewModel.CurrentPageImage):
                OnPropertyChanged(nameof(CurrentPageImage)); break;
            case nameof(ShellViewModel.ReaderImageWidth):
                OnPropertyChanged(nameof(ReaderImageWidth)); break;
            case nameof(ShellViewModel.PresenterTextContent):
                OnPropertyChanged(nameof(PresenterTextContent)); break;
            case nameof(ShellViewModel.IsRegionModeActive):
                OnPropertyChanged(nameof(IsRegionModeActive)); break;
            case nameof(ShellViewModel.DocumentNameDraft):
                OnPropertyChanged(nameof(DocumentNameDraft)); break;
            case nameof(ShellViewModel.OutlineTitleDraft):
                OnPropertyChanged(nameof(OutlineTitleDraft)); break;
            case nameof(ShellViewModel.CurrentPageLabelDraft):
                OnPropertyChanged(nameof(CurrentPageLabelDraft)); break;
            case nameof(ShellViewModel.PageRangeDraft):
                OnPropertyChanged(nameof(PageRangeDraft)); break;
            case nameof(ShellViewModel.SelectedOutlineItem):
                OnPropertyChanged(nameof(SelectedOutlineItem)); break;
            case nameof(ShellViewModel.SelectedThumbnail):
                OnPropertyChanged(nameof(SelectedThumbnail)); break;
            case nameof(ShellViewModel.SelectedSearchResult):
                OnPropertyChanged(nameof(SelectedSearchResult)); break;
            case nameof(ShellViewModel.MspTranscriptSummary):
                OnPropertyChanged(nameof(MspTranscriptSummary)); break;
            case nameof(ShellViewModel.CurrentPageIndicator):
                OnPropertyChanged(nameof(CurrentPageIndicator)); break;
            case nameof(ShellViewModel.PresenterKindLabel):
                OnPropertyChanged(nameof(PresenterKindLabel)); break;
            case nameof(ShellViewModel.ActiveModelLabel):
                OnPropertyChanged(nameof(ActiveModelLabel)); break;
            case nameof(ShellViewModel.ActiveTitle):
                OnPropertyChanged(nameof(ActiveTitle)); break;
            case nameof(ShellViewModel.ActiveSubtitle):
                OnPropertyChanged(nameof(ActiveSubtitle)); break;
            case nameof(ShellViewModel.ActiveWorkspaceScope):
                OnPropertyChanged(nameof(ActiveWorkspaceScope)); break;
            case nameof(ShellViewModel.PendingAttachmentSummary):
                OnPropertyChanged(nameof(PendingAttachmentSummary)); break;
            case nameof(ShellViewModel.StatusMessage):
                OnPropertyChanged(nameof(StatusMessage)); break;
            case nameof(ShellViewModel.IsBusy):
                OnPropertyChanged(nameof(IsBusy)); break;
            case nameof(ShellViewModel.HasDocument):
                OnPropertyChanged(nameof(HasDocument)); break;
            case nameof(ShellViewModel.HasPdfDocument):
                OnPropertyChanged(nameof(HasPdfDocument)); break;
            case nameof(ShellViewModel.HasPageImage):
                OnPropertyChanged(nameof(HasPageImage)); break;
            case nameof(ShellViewModel.HasTextPresenter):
                OnPropertyChanged(nameof(HasTextPresenter)); break;
            case nameof(ShellViewModel.IsPresenterPlaceholderVisible):
                OnPropertyChanged(nameof(IsPresenterPlaceholderVisible)); break;
        }
    }
}
