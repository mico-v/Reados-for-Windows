using System;
using System.ComponentModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using ReadOS.App.Services;
using ReadOS.App.ViewModels;

namespace ReadOS.App;

public sealed partial class MainWindow : Window
{
    private const double SidebarHideThreshold = 112;
    private const double SidebarSoftMinimum = 220;
    private const double SurfaceFocusThreshold = 220;

    public MainWindow()
    {
        InitializeComponent();

        var pdfService = new PdfDocumentService();
        var workspaceStore = new WorkspaceStore(pdfService);
        ViewModel = new ShellViewModel(
            workspaceStore,
            pdfService,
            new FileDialogService(),
            new AiChatService());
        ViewModel.SetHostWindow(this);

        RootShell.DataContext = ViewModel;
        ViewModel.PropertyChanged += ViewModel_PropertyChanged;
        RootShell.Loaded += MainWindow_Loaded;
        ConfigureTitleBar();
        ApplyTheme();
        ApplyPaneWidths();
    }

    public ShellViewModel ViewModel { get; }

    private async void MainWindow_Loaded(object sender, RoutedEventArgs e)
    {
        await ViewModel.InitializeAsync();
        ApplyTheme();
        ApplyPaneWidths();
    }

    private void ViewModel_PropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is nameof(ViewModel.IsLibraryVisible)
            or nameof(ViewModel.IsChatVisible)
            or nameof(ViewModel.IsThumbnailsVisible)
            or nameof(ViewModel.IsOutlineVisible)
            or nameof(ViewModel.WorkspaceLayout)
            or nameof(ViewModel.SidebarWidth)
            or nameof(ViewModel.ChatWidth)
            or nameof(ViewModel.PresenterWidth)
            or nameof(ViewModel.CurrentRoute))
        {
            ApplyPaneWidths();
        }

        if (e.PropertyName is nameof(ViewModel.IsDarkTheme))
        {
            ApplyTheme();
        }
    }

    private void SidebarResizeThumb_DragDelta(object sender, DragDeltaEventArgs e)
    {
        var proposed = ViewModel.SidebarWidth + e.HorizontalChange;
        if (proposed <= SidebarHideThreshold)
        {
            ViewModel.IsLibraryVisible = false;
        }
        else
        {
            ViewModel.IsLibraryVisible = true;
            ViewModel.SidebarWidth = Math.Max(SidebarSoftMinimum, proposed);
        }

        ApplyPaneWidths();
    }

    private void WorkspaceSurfaceResizeThumb_DragDelta(object sender, DragDeltaEventArgs e)
    {
        ApplyWorkspaceSurfaceDrag(e.HorizontalChange);
        ApplyPaneWidths();
    }

    private void ApplyPaneWidths()
    {
        var workspaceVisible = ViewModel.IsWorkspaceRoute;
        var sidebarVisible = workspaceVisible && ViewModel.IsLibraryVisible;
        var chatVisible = workspaceVisible && ViewModel.IsChatWorkspaceVisible;
        var presenterVisible = workspaceVisible && ViewModel.IsPresenterVisible;

        SidebarColumn.Width = sidebarVisible ? new GridLength(ViewModel.SidebarWidth) : new GridLength(0);
        SidebarSplitterColumn.Width = sidebarVisible ? new GridLength(1) : new GridLength(0);
        ChatColumn.Width = ResolveChatWidth(chatVisible);
        PresenterSplitterColumn.Width = chatVisible && presenterVisible ? new GridLength(1) : new GridLength(0);
        PresenterColumn.Width = ResolvePresenterWidth(presenterVisible);
    }

    private void ApplyTheme()
    {
        RootShell.RequestedTheme = ViewModel.IsDarkTheme ? ElementTheme.Dark : ElementTheme.Light;
    }

    private void ConfigureTitleBar()
    {
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(TitleBarDragRegion);
    }

    private GridLength ResolveChatWidth(bool chatVisible)
    {
        if (!chatVisible)
        {
            return new GridLength(0);
        }

        return ViewModel.WorkspaceLayout == WorkspaceLayoutMode.PresenterPrimary
            ? new GridLength(ViewModel.ChatWidth)
            : new GridLength(1, GridUnitType.Star);
    }

    private GridLength ResolvePresenterWidth(bool presenterVisible)
    {
        if (!presenterVisible)
        {
            return new GridLength(0);
        }

        return ViewModel.WorkspaceLayout is WorkspaceLayoutMode.FocusPresenter or WorkspaceLayoutMode.PresenterPrimary
            ? new GridLength(1, GridUnitType.Star)
            : new GridLength(ViewModel.PresenterWidth);
    }

    private void ApplyWorkspaceSurfaceDrag(double horizontalChange)
    {
        var chatWidth = ChatSurface.ActualWidth;
        var presenterWidth = PresenterSurface.ActualWidth;
        if (chatWidth <= 0 || presenterWidth <= 0)
        {
            ViewModel.ApplyWorkspaceLayoutPreset(horizontalChange < 0
                ? WorkspaceLayoutMode.PresenterPrimary
                : WorkspaceLayoutMode.ChatPrimary);
            return;
        }

        var proposedChat = chatWidth + horizontalChange;
        var proposedPresenter = presenterWidth - horizontalChange;
        if (proposedPresenter <= SurfaceFocusThreshold)
        {
            ViewModel.ApplyWorkspaceLayoutPreset(WorkspaceLayoutMode.FocusChat);
            return;
        }

        if (proposedChat <= SurfaceFocusThreshold)
        {
            ViewModel.ApplyWorkspaceLayoutPreset(WorkspaceLayoutMode.FocusPresenter);
            return;
        }

        if (proposedPresenter >= proposedChat)
        {
            ViewModel.WorkspaceLayout = WorkspaceLayoutMode.PresenterPrimary;
            ViewModel.CurrentRoute = ShellRoute.Reader;
            ViewModel.ChatWidth = proposedChat;
        }
        else
        {
            ViewModel.WorkspaceLayout = WorkspaceLayoutMode.ChatPrimary;
            ViewModel.CurrentRoute = ShellRoute.Home;
            ViewModel.PresenterWidth = proposedPresenter;
        }
    }

}
