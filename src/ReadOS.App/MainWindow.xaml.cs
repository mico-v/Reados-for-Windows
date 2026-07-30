using System.ComponentModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using ReadOS.App.Models;
using ReadOS.App.Services;
using ReadOS.App.ViewModels;

namespace ReadOS.App;

public sealed partial class MainWindow : Window
{
    private readonly LayoutService layoutService;

    public MainWindow(ShellViewModel viewModel, LayoutService layoutService)
    {
        InitializeComponent();

        this.layoutService = layoutService;
        ViewModel = viewModel;
        // Retained for layout-service symmetry; no longer drives splitter columns.
        this.layoutService.RememberPaneWidths(ViewModel.SidebarWidth, ViewModel.InspectorWidth);
        ViewModel.SetHostWindow(this);

        RootShell.DataContext = ViewModel;
        ViewModel.PropertyChanged += ViewModel_PropertyChanged;
        RootShell.Loaded += MainWindow_Loaded;
        ConfigureTitleBar();
        ApplyTheme();
    }

    public ShellViewModel ViewModel { get; }

    private async void MainWindow_Loaded(object sender, RoutedEventArgs e)
    {
        await ViewModel.InitializeAsync();
        ApplyTheme();
    }

    private void ViewModel_PropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is nameof(ViewModel.IsDarkTheme))
        {
            ApplyTheme();
        }
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

    private double resizeStartHeight;
    private double resizeStartY;
    private bool resizeCaptured;

    private void RunDrawerResizeGrip_PointerPressed(object sender, PointerRoutedEventArgs e)
    {
        resizeCaptured = true;
        resizeStartHeight = ViewModel.RunDrawerHeight;
        resizeStartY = e.GetCurrentPoint(RootShell).Position.Y;
        (sender as UIElement)?.CapturePointer(e.Pointer);
    }

    private void RunDrawerResizeGrip_PointerMoved(object sender, PointerRoutedEventArgs e)
    {
        if (!resizeCaptured)
        {
            return;
        }

        var currentY = e.GetCurrentPoint(RootShell).Position.Y;
        var delta = resizeStartY - currentY;
        var maxHeight = RootShell.ActualHeight * 0.6;
        ViewModel.RunDrawerHeight = Math.Clamp(resizeStartHeight + delta, 120, Math.Max(120, maxHeight));
    }

    private async void RunDrawerResizeGrip_PointerReleased(object sender, PointerRoutedEventArgs e)
    {
        if (!resizeCaptured)
        {
            return;
        }

        resizeCaptured = false;
        (sender as UIElement)?.ReleasePointerCapture(e.Pointer);
        await ViewModel.CommitRunDrawerLayoutAsync();
    }

    private void RunDrawerTranscript_ItemClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is MspTranscriptEntry entry)
        {
            ViewModel.OpenMspTranscriptDetail(entry);
        }
    }
}
