using System.Collections.ObjectModel;
using System.ComponentModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using ReadOS.App.Models;

namespace ReadOS.App.ViewModels;

public enum SettingsRoute
{
    General,
    Provider,
    Prompts,
    Workspace
}

public sealed partial class SettingsViewModel : ObservableObject
{
    private readonly ShellViewModel shell;

    public SettingsViewModel(ShellViewModel shell)
    {
        this.shell = shell;
        shell.PropertyChanged += OnShellPropertyChanged;
    }

    // ── Collections ─────────────────────────────────────────────────────

    public ObservableCollection<LanguageOption> LanguageOptions => shell.LanguageOptions;

    // ── State (pass-through to shell) ───────────────────────────────────

    public SettingsRoute SelectedSettingsRoute
    {
        get => shell.SelectedSettingsRoute;
        set => shell.SelectedSettingsRoute = value;
    }

    public LanguageOption? SelectedLanguageOption
    {
        get => shell.SelectedLanguageOption;
        set => shell.SelectedLanguageOption = value;
    }

    public string ProviderName
    {
        get => shell.ProviderName;
        set => shell.ProviderName = value;
    }

    public string ProviderBaseUrl
    {
        get => shell.ProviderBaseUrl;
        set => shell.ProviderBaseUrl = value;
    }

    public string ProviderApiKey
    {
        get => shell.ProviderApiKey;
        set => shell.ProviderApiKey = value;
    }

    public string ModelName
    {
        get => shell.ModelName;
        set => shell.ModelName = value;
    }

    public bool UseOfflineResponses
    {
        get => shell.UseOfflineResponses;
        set => shell.UseOfflineResponses = value;
    }

    public bool IsDarkTheme
    {
        get => shell.IsDarkTheme;
        set => shell.IsDarkTheme = value;
    }

    public string AttachmentDefaultPrompt
    {
        get => shell.AttachmentDefaultPrompt;
        set => shell.AttachmentDefaultPrompt = value;
    }

    public string RegionExplainPrompt
    {
        get => shell.RegionExplainPrompt;
        set => shell.RegionExplainPrompt = value;
    }

    public string ChapterExplainPrompt
    {
        get => shell.ChapterExplainPrompt;
        set => shell.ChapterExplainPrompt = value;
    }

    public string MinorUEndpoint
    {
        get => shell.MinorUEndpoint;
        set => shell.MinorUEndpoint = value;
    }

    // ── Computed ────────────────────────────────────────────────────────

    public bool IsGeneralSettingsSelected => SelectedSettingsRoute == SettingsRoute.General;
    public bool IsProviderSettingsSelected => SelectedSettingsRoute == SettingsRoute.Provider;
    public bool IsPromptSettingsSelected => SelectedSettingsRoute == SettingsRoute.Prompts;
    public bool IsWorkspaceSettingsSelected => SelectedSettingsRoute == SettingsRoute.Workspace;

    public string ActiveModelLabel => shell.ActiveModelLabel;
    public string ActiveTitle => shell.ActiveTitle;
    public string ActiveSubtitle => shell.ActiveSubtitle;
    public string ThemeToggleLabel => shell.ThemeToggleLabel;
    public string ActiveWorkspaceScope => shell.ActiveWorkspaceScope;
    public string StatusMessage => shell.StatusMessage;

    // ── Commands (delegate to shell's public command properties) ────────

    [RelayCommand]
    private void SelectSettingsCategory(string category)
    {
        if (Enum.TryParse<SettingsRoute>(category, ignoreCase: true, out var route))
            SelectedSettingsRoute = route;
    }

    public IAsyncRelayCommand SaveSettingsCommand => shell.SaveSettingsCommand;
    public IRelayCommand ToggleThemeCommand => shell.ToggleThemeCommand;
    public IRelayCommand NavigateHomeCommand => shell.NavigateHomeCommand;
    public IRelayCommand NavigateSettingsCommand => shell.NavigateSettingsCommand;
    public IRelayCommand CloseSettingsCommand => shell.CloseSettingsCommand;
    public IAsyncRelayCommand ExportWorkspaceCommand => shell.ExportWorkspaceCommand;
    public IAsyncRelayCommand ImportWorkspaceCommand => shell.ImportWorkspaceCommand;
    public IRelayCommand OpenLocationCommand => shell.OpenLocationCommand;

    // ── Property change forwarding ──────────────────────────────────────

    private void OnShellPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        switch (e.PropertyName)
        {
            case nameof(ShellViewModel.SelectedSettingsRoute):
                OnPropertyChanged(nameof(SelectedSettingsRoute));
                OnPropertyChanged(nameof(IsGeneralSettingsSelected));
                OnPropertyChanged(nameof(IsProviderSettingsSelected));
                OnPropertyChanged(nameof(IsPromptSettingsSelected));
                OnPropertyChanged(nameof(IsWorkspaceSettingsSelected));
                break;
            case nameof(ShellViewModel.SelectedLanguageOption):
                OnPropertyChanged(nameof(SelectedLanguageOption)); break;
            case nameof(ShellViewModel.ProviderName):
                OnPropertyChanged(nameof(ProviderName)); break;
            case nameof(ShellViewModel.ProviderBaseUrl):
                OnPropertyChanged(nameof(ProviderBaseUrl)); break;
            case nameof(ShellViewModel.ProviderApiKey):
                OnPropertyChanged(nameof(ProviderApiKey)); break;
            case nameof(ShellViewModel.ModelName):
                OnPropertyChanged(nameof(ModelName)); break;
            case nameof(ShellViewModel.UseOfflineResponses):
                OnPropertyChanged(nameof(UseOfflineResponses)); break;
            case nameof(ShellViewModel.IsDarkTheme):
                OnPropertyChanged(nameof(IsDarkTheme)); break;
            case nameof(ShellViewModel.AttachmentDefaultPrompt):
                OnPropertyChanged(nameof(AttachmentDefaultPrompt)); break;
            case nameof(ShellViewModel.RegionExplainPrompt):
                OnPropertyChanged(nameof(RegionExplainPrompt)); break;
            case nameof(ShellViewModel.ChapterExplainPrompt):
                OnPropertyChanged(nameof(ChapterExplainPrompt)); break;
            case nameof(ShellViewModel.MinorUEndpoint):
                OnPropertyChanged(nameof(MinorUEndpoint)); break;
            case nameof(ShellViewModel.ActiveModelLabel):
                OnPropertyChanged(nameof(ActiveModelLabel)); break;
            case nameof(ShellViewModel.ActiveTitle):
                OnPropertyChanged(nameof(ActiveTitle)); break;
            case nameof(ShellViewModel.ActiveSubtitle):
                OnPropertyChanged(nameof(ActiveSubtitle)); break;
            case nameof(ShellViewModel.ThemeToggleLabel):
                OnPropertyChanged(nameof(ThemeToggleLabel)); break;
            case nameof(ShellViewModel.ActiveWorkspaceScope):
                OnPropertyChanged(nameof(ActiveWorkspaceScope)); break;
            case nameof(ShellViewModel.StatusMessage):
                OnPropertyChanged(nameof(StatusMessage)); break;
        }
    }
}
