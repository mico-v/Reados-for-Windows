using System.Collections.ObjectModel;
using System.ComponentModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using ReadOS.App.Models;

namespace ReadOS.App.ViewModels;

public sealed partial class ThreadViewModel : ObservableObject
{
    private readonly ShellViewModel shell;

    public ThreadViewModel(ShellViewModel shell)
    {
        this.shell = shell;
        shell.PropertyChanged += OnShellPropertyChanged;
    }

    // ── Collections ─────────────────────────────────────────────────────

    public ObservableCollection<ChatMessage> ChatMessages => shell.ChatMessages;
    public ObservableCollection<ThreadTimelineItem> TimelineItems => shell.TimelineItems;
    public ObservableCollection<ChatAttachment> PendingAttachments => shell.PendingAttachments;

    // ── State (pass-through to shell) ───────────────────────────────────

    public string ComposerDraft
    {
        get => shell.ComposerDraft;
        set => shell.ComposerDraft = value;
    }

    public string PageRangeDraft
    {
        get => shell.PageRangeDraft;
        set => shell.PageRangeDraft = value;
    }

    // ── Computed ────────────────────────────────────────────────────────

    public string PendingAttachmentSummary => shell.PendingAttachmentSummary;
    public string ActiveTitle => shell.ActiveTitle;
    public string ActiveWorkspaceScope => shell.ActiveWorkspaceScope;
    public string WorkspaceModeLabel => shell.WorkspaceModeLabel;
    public string RuntimeStatusLabel => shell.RuntimeStatusLabel;
    public string ApprovalModeLabel => shell.ApprovalModeLabel;
    public string SendButtonLabel => shell.SendButtonLabel;
    public string MspActivitySummary => shell.MspActivitySummary;
    public string PendingApprovalLabel => shell.PendingApprovalLabel;
    public bool HasPendingAttachments => shell.PendingAttachments.Count > 0;
    public bool HasPendingApprovals => shell.HasPendingApprovals;
    public bool IsAnyMspRunning => shell.IsAnyMspRunning;

    // ── Commands (delegate to shell's public command properties) ────────

    public IAsyncRelayCommand SendPromptCommand => shell.SendPromptCommand;
    public IRelayCommand AttachCurrentPageCommand => shell.AttachCurrentPageCommand;
    public IRelayCommand AttachRangeCommand => shell.AttachRangeCommand;
    public IRelayCommand StartRegionSelectionCommand => shell.StartRegionSelectionCommand;
    public IRelayCommand ClearAttachmentsCommand => shell.ClearAttachmentsCommand;
    public IAsyncRelayCommand ClearConversationCommand => shell.ClearConversationCommand;
    public IAsyncRelayCommand ApproveMspCommandCommand => shell.ApproveMspCommandCommand;
    public IAsyncRelayCommand DenyMspCommandCommand => shell.DenyMspCommandCommand;
    public IRelayCommand CancelMspCommandCommand => shell.CancelMspCommandCommand;
    public IRelayCommand CancelActiveMspCommandCommand => shell.CancelActiveMspCommandCommand;
    public IRelayCommand ToggleRunDrawerCommand => shell.ToggleRunDrawerCommand;
    public IRelayCommand SelectArtifactCommand => shell.SelectArtifactCommand;

    // ── Property change forwarding ──────────────────────────────────────

    private void OnShellPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        switch (e.PropertyName)
        {
            case nameof(ShellViewModel.ComposerDraft):
                OnPropertyChanged(nameof(ComposerDraft));
                break;
            case nameof(ShellViewModel.PageRangeDraft):
                OnPropertyChanged(nameof(PageRangeDraft));
                break;
            case nameof(ShellViewModel.PendingAttachmentSummary):
                OnPropertyChanged(nameof(PendingAttachmentSummary));
                OnPropertyChanged(nameof(HasPendingAttachments));
                break;
            case nameof(ShellViewModel.ActiveTitle):
                OnPropertyChanged(nameof(ActiveTitle));
                break;
            case nameof(ShellViewModel.ActiveWorkspaceScope):
                OnPropertyChanged(nameof(ActiveWorkspaceScope));
                break;
            case nameof(ShellViewModel.WorkspaceModeLabel):
                OnPropertyChanged(nameof(WorkspaceModeLabel));
                break;
            case nameof(ShellViewModel.RuntimeStatusLabel):
                OnPropertyChanged(nameof(RuntimeStatusLabel));
                break;
            case nameof(ShellViewModel.ApprovalModeLabel):
                OnPropertyChanged(nameof(ApprovalModeLabel));
                break;
            case nameof(ShellViewModel.SendButtonLabel):
                OnPropertyChanged(nameof(SendButtonLabel));
                break;
            case nameof(ShellViewModel.MspActivitySummary):
                OnPropertyChanged(nameof(MspActivitySummary));
                break;
            case nameof(ShellViewModel.PendingApprovalLabel):
                OnPropertyChanged(nameof(PendingApprovalLabel));
                break;
            case nameof(ShellViewModel.HasPendingApprovals):
                OnPropertyChanged(nameof(HasPendingApprovals));
                break;
            case nameof(ShellViewModel.IsAnyMspRunning):
                OnPropertyChanged(nameof(IsAnyMspRunning));
                break;
        }
    }
}
