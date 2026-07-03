using System.Collections.ObjectModel;
using System.ComponentModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using ReadOS.App.Models;

namespace ReadOS.App.ViewModels;

public enum WorkspaceSidebarMode
{
    Conversations,
    Materials
}

public sealed partial class SidebarViewModel : ObservableObject
{
    private readonly ShellViewModel shell;

    public SidebarViewModel(ShellViewModel shell)
    {
        this.shell = shell;
        shell.PropertyChanged += OnShellPropertyChanged;
    }

    // ── Collections ─────────────────────────────────────────────────────

    public ObservableCollection<NavigationEntry> NavigationEntries => shell.NavigationEntries;
    public ObservableCollection<LibraryItem> LibraryItems => shell.LibraryItems;
    public ObservableCollection<ChatConversation> Conversations => shell.Conversations;

    // ── State (pass-through to shell) ───────────────────────────────────

    public WorkspaceSidebarMode SidebarMode
    {
        get => shell.SidebarMode;
        set => shell.SidebarMode = value;
    }

    public string SearchQuery
    {
        get => shell.SearchQuery;
        set => shell.SearchQuery = value;
    }

    public NavigationEntry? SelectedNavigationEntry
    {
        get => shell.SelectedNavigationEntry;
        set => shell.SelectedNavigationEntry = value;
    }

    public ChatConversation? SelectedConversation
    {
        get => shell.SelectedConversation;
        set => shell.SelectedConversation = value;
    }

    // ── Computed ────────────────────────────────────────────────────────

    public bool IsConversationsSidebarSelected => SidebarMode == WorkspaceSidebarMode.Conversations;
    public bool IsMaterialsSidebarSelected => SidebarMode == WorkspaceSidebarMode.Materials;
    public string LibrarySummary => shell.LibrarySummary;

    // ── Commands (delegate to shell's public command properties) ────────

    [RelayCommand]
    private void SelectSidebarMode(string mode)
    {
        if (Enum.TryParse<WorkspaceSidebarMode>(mode, ignoreCase: true, out var sidebarMode))
        {
            SidebarMode = sidebarMode;
            shell.IsSidebarVisible = true;
        }
    }

    [RelayCommand]
    private void ToggleSidebar() => shell.IsSidebarVisible = !shell.IsSidebarVisible;

    // These forward to shell's generated RelayCommand properties
    public IRelayCommand NewConversationCommand => shell.NewConversationCommand;
    public IRelayCommand ImportMaterialCommand => shell.ImportMaterialCommand;
    public IRelayCommand CreateProjectCommand => shell.CreateProjectCommand;
    public IRelayCommand OpenLocationCommand => shell.OpenLocationCommand;

    // ── Property change forwarding ──────────────────────────────────────

    private void OnShellPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        switch (e.PropertyName)
        {
            case nameof(ShellViewModel.SidebarMode):
                OnPropertyChanged(nameof(SidebarMode));
                OnPropertyChanged(nameof(IsConversationsSidebarSelected));
                OnPropertyChanged(nameof(IsMaterialsSidebarSelected));
                break;
            case nameof(ShellViewModel.SearchQuery):
                OnPropertyChanged(nameof(SearchQuery));
                break;
            case nameof(ShellViewModel.SelectedNavigationEntry):
                OnPropertyChanged(nameof(SelectedNavigationEntry));
                break;
            case nameof(ShellViewModel.SelectedConversation):
                OnPropertyChanged(nameof(SelectedConversation));
                break;
            case nameof(ShellViewModel.LibrarySummary):
                OnPropertyChanged(nameof(LibrarySummary));
                break;
        }
    }
}
