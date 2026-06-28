using System.ComponentModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using ReadOS.App.ViewModels;
using Windows.Foundation;

namespace ReadOS.App.Views;

public sealed partial class PresenterSurfaceView : UserControl
{
    private const double PresenterDrawerHideThreshold = 84;
    private const double PresenterDrawerSoftMinimum = 116;
    private ShellViewModel? subscribedViewModel;
    private bool selectingRegion;
    private Point regionStart;

    public PresenterSurfaceView()
    {
        InitializeComponent();
        Loaded += PresenterSurfaceView_Loaded;
        DataContextChanged += PresenterSurfaceView_DataContextChanged;
    }

    private ShellViewModel? ViewModel => DataContext as ShellViewModel;

    private void PresenterSurfaceView_Loaded(object sender, RoutedEventArgs e)
    {
        ApplyDrawerWidth();
    }

    private void PresenterSurfaceView_DataContextChanged(FrameworkElement sender, DataContextChangedEventArgs args)
    {
        if (subscribedViewModel is not null)
        {
            subscribedViewModel.PropertyChanged -= ViewModel_PropertyChanged;
        }

        subscribedViewModel = args.NewValue as ShellViewModel;
        if (subscribedViewModel is not null)
        {
            subscribedViewModel.PropertyChanged += ViewModel_PropertyChanged;
        }

        ApplyDrawerWidth();
    }

    private void ViewModel_PropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is nameof(ShellViewModel.IsThumbnailsVisible)
            or nameof(ShellViewModel.PresenterDrawerWidth)
            or nameof(ShellViewModel.WorkspaceLayout))
        {
            ApplyDrawerWidth();
        }
    }

    private void PresenterDrawerResizeThumb_DragDelta(object sender, DragDeltaEventArgs e)
    {
        if (ViewModel is null)
        {
            return;
        }

        var proposed = ViewModel.PresenterDrawerWidth + e.HorizontalChange;
        if (proposed <= PresenterDrawerHideThreshold)
        {
            ViewModel.IsThumbnailsVisible = false;
        }
        else
        {
            ViewModel.IsThumbnailsVisible = true;
            ViewModel.PresenterDrawerWidth = Math.Max(PresenterDrawerSoftMinimum, proposed);
        }

        ApplyDrawerWidth();
    }

    private void RegionCanvas_PointerPressed(object sender, PointerRoutedEventArgs e)
    {
        if (ViewModel is null || !ViewModel.IsRegionModeActive)
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
        if (ViewModel is null || !selectingRegion)
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

    private void ApplyDrawerWidth()
    {
        if (ViewModel is null)
        {
            return;
        }

        var visible = ViewModel.IsThumbnailsVisible;
        PresenterThumbnailColumn.Width = visible ? new GridLength(ViewModel.PresenterDrawerWidth) : new GridLength(0);
        PresenterDrawerSplitterColumn.Width = visible ? new GridLength(1) : new GridLength(0);
    }
}
