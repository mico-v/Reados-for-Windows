using System;
using System.ComponentModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using ReadOS.App.Services;
using ReadOS.App.ViewModels;
using Windows.Foundation;

namespace ReadOS.App;

public sealed partial class MainWindow : Window
{
    private const double MinSidebarWidth = 260;
    private const double MaxSidebarWidth = 460;
    private const double MinChatWidth = 320;
    private const double MaxChatWidth = 520;
    private bool selectingRegion;
    private Point regionStart;

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
        ApplyPaneWidths();
    }

    public ShellViewModel ViewModel { get; }

    private async void MainWindow_Loaded(object sender, RoutedEventArgs e)
    {
        await ViewModel.InitializeAsync();
        ApplyPaneWidths();
    }

    private void ViewModel_PropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is nameof(ViewModel.IsLibraryVisible)
            or nameof(ViewModel.IsChatVisible)
            or nameof(ViewModel.SidebarWidth)
            or nameof(ViewModel.ChatWidth))
        {
            ApplyPaneWidths();
        }
    }

    private void SidebarResizeThumb_DragDelta(object sender, DragDeltaEventArgs e)
    {
        ViewModel.SidebarWidth = Clamp(ViewModel.SidebarWidth + e.HorizontalChange, MinSidebarWidth, MaxSidebarWidth);
        ApplyPaneWidths();
    }

    private void ChatResizeThumb_DragDelta(object sender, DragDeltaEventArgs e)
    {
        ViewModel.ChatWidth = Clamp(ViewModel.ChatWidth - e.HorizontalChange, MinChatWidth, MaxChatWidth);
        ApplyPaneWidths();
    }

    private void ApplyPaneWidths()
    {
        SidebarColumn.Width = ViewModel.IsLibraryVisible ? new GridLength(ViewModel.SidebarWidth) : new GridLength(0);
        SidebarSplitterColumn.Width = ViewModel.IsLibraryVisible ? new GridLength(1) : new GridLength(0);
        ChatColumn.Width = ViewModel.IsChatVisible ? new GridLength(ViewModel.ChatWidth) : new GridLength(0);
        ChatSplitterColumn.Width = ViewModel.IsChatVisible ? new GridLength(1) : new GridLength(0);
    }

    private void RegionCanvas_PointerPressed(object sender, PointerRoutedEventArgs e)
    {
        if (!ViewModel.IsRegionModeActive)
        {
            return;
        }

        selectingRegion = true;
        regionStart = e.GetCurrentPoint(RegionCanvas).Position;
        RegionRectangle.Visibility = Visibility.Visible;
        Canvas.SetLeft(RegionRectangle, regionStart.X);
        Canvas.SetTop(RegionRectangle, regionStart.Y);
        RegionRectangle.Width = 0;
        RegionRectangle.Height = 0;
        RegionCanvas.CapturePointer(e.Pointer);
    }

    private void RegionCanvas_PointerMoved(object sender, PointerRoutedEventArgs e)
    {
        if (!selectingRegion)
        {
            return;
        }

        var point = e.GetCurrentPoint(RegionCanvas).Position;
        var left = Math.Min(regionStart.X, point.X);
        var top = Math.Min(regionStart.Y, point.Y);
        var width = Math.Abs(point.X - regionStart.X);
        var height = Math.Abs(point.Y - regionStart.Y);
        Canvas.SetLeft(RegionRectangle, left);
        Canvas.SetTop(RegionRectangle, top);
        RegionRectangle.Width = width;
        RegionRectangle.Height = height;
    }

    private void RegionCanvas_PointerReleased(object sender, PointerRoutedEventArgs e)
    {
        if (!selectingRegion)
        {
            return;
        }

        selectingRegion = false;
        RegionCanvas.ReleasePointerCapture(e.Pointer);
        var point = e.GetCurrentPoint(RegionCanvas).Position;
        var left = Math.Min(regionStart.X, point.X);
        var top = Math.Min(regionStart.Y, point.Y);
        var width = Math.Abs(point.X - regionStart.X);
        var height = Math.Abs(point.Y - regionStart.Y);
        if (width >= 12 && height >= 12)
        {
            var bounds = RegionCanvas.RenderSize;
            ViewModel.AttachRegionSelection(
                bounds.Width <= 0 ? 0 : left / bounds.Width,
                bounds.Height <= 0 ? 0 : top / bounds.Height,
                bounds.Width <= 0 ? 0 : width / bounds.Width,
                bounds.Height <= 0 ? 0 : height / bounds.Height);
        }

        RegionRectangle.Visibility = Visibility.Collapsed;
    }

    private static double Clamp(double value, double min, double max)
    {
        return Math.Min(Math.Max(value, min), max);
    }
}
