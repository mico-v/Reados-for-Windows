using System.ComponentModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using ReadOS.App.Services;
using ReadOS.App.ViewModels;

namespace ReadOS.App;

public sealed partial class MainWindow : Window
{
    private readonly LayoutService layoutService;

    public MainWindow()
    {
        InitializeComponent();

        layoutService = new LayoutService();

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
        RootShell.SizeChanged += RootShell_SizeChanged;
        ConfigureTitleBar();
        ApplyTheme();
    }

    public ShellViewModel ViewModel { get; }

    private async void MainWindow_Loaded(object sender, RoutedEventArgs e)
    {
        await ViewModel.InitializeAsync();
        ApplyTheme();
        ApplyLayout();
    }

    private void RootShell_SizeChanged(object sender, SizeChangedEventArgs e)
    {
        ApplyLayout();
    }

    private void ViewModel_PropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is nameof(ViewModel.IsSidebarVisible)
            or nameof(ViewModel.IsInspectorVisible)
            or nameof(ViewModel.CurrentRoute))
        {
            ApplyLayout();
        }

        if (e.PropertyName is nameof(ViewModel.IsDarkTheme))
        {
            ApplyTheme();
        }
    }

    // ── Splitter drag handlers ──────────────────────────────────────────

    private void SidebarSplitter_DragDelta(object sender, DragDeltaEventArgs e)
    {
        var proposed = ViewModel.SidebarWidth + e.HorizontalChange;
        if (proposed <= LayoutService.SidebarMin)
        {
            ViewModel.IsSidebarVisible = false;
        }
        else
        {
            ViewModel.IsSidebarVisible = true;
            ViewModel.SidebarWidth = Math.Max(LayoutService.SidebarMin, proposed);
        }

        ApplyLayout();
    }

    private void InspectorSplitter_DragDelta(object sender, DragDeltaEventArgs e)
    {
        var proposed = ViewModel.InspectorWidth - e.HorizontalChange;
        if (proposed <= LayoutService.InspectorMin)
        {
            ViewModel.IsInspectorVisible = false;
        }
        else
        {
            ViewModel.IsInspectorVisible = true;
            ViewModel.InspectorWidth = Math.Max(LayoutService.InspectorMin, proposed);
        }

        ApplyLayout();
    }

    // ── Layout application ──────────────────────────────────────────────

    private void ApplyLayout()
    {
        var windowWidth = RootShell.ActualWidth;
        if (windowWidth <= 0) windowWidth = 1400;

        var config = layoutService.ComputeConfiguration(
            windowWidth,
            ViewModel.IsSidebarVisible,
            ViewModel.IsInspectorVisible,
            dragSidebarWidth: ViewModel.SidebarWidth,
            dragInspectorWidth: ViewModel.InspectorWidth);

        SidebarColumn.Width = config.SidebarVisible
            ? new GridLength(config.SidebarWidth)
            : new GridLength(0);
        SidebarSplitterColumn.Width = config.SidebarSplitterVisible
            ? new GridLength(6)
            : new GridLength(0);

        InspectorColumn.Width = config.InspectorVisible
            ? new GridLength(config.InspectorWidth)
            : new GridLength(0);
        InspectorSplitterColumn.Width = config.InspectorSplitterVisible
            ? new GridLength(6)
            : new GridLength(0);

        ViewModel.SidebarWidth = config.SidebarWidth;
        ViewModel.InspectorWidth = config.InspectorWidth;
    }

    private void ApplyTheme()
    {
        RootShell.RequestedTheme = ViewModel.IsDarkTheme
            ? ElementTheme.Dark
            : ElementTheme.Light;
    }

    private void ConfigureTitleBar()
    {
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(TitleBarDragRegion);
    }
}
