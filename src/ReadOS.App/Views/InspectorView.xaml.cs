using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using ReadOS.App.ViewModels;
using Windows.Foundation;

namespace ReadOS.App.Views;

public sealed partial class InspectorView : UserControl
{
    private bool selectingRegion;
    private Point regionStart;

    public InspectorView()
    {
        InitializeComponent();
    }

    private ShellViewModel? Shell => DataContext as ShellViewModel;
    private InspectorViewModel? Inspector => Shell?.Inspector;

    private void RegionCanvas_PointerPressed(object sender, PointerRoutedEventArgs e)
    {
        if (Inspector is not { IsRegionModeActive: true }) return;

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
        if (!selectingRegion) return;

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
        if (Inspector is null || !selectingRegion) return;

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
            Inspector.AttachRegionSelection(
                bounds.Width <= 0 ? 0 : left / bounds.Width,
                bounds.Height <= 0 ? 0 : top / bounds.Height,
                bounds.Width <= 0 ? 0 : width / bounds.Width,
                bounds.Height <= 0 ? 0 : height / bounds.Height);
        }

        RegionRectangle.Visibility = Visibility.Collapsed;
    }
}
