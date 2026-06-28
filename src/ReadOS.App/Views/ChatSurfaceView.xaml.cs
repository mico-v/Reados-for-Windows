using System.ComponentModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using ReadOS.App.ViewModels;

namespace ReadOS.App.Views;

public sealed partial class ChatSurfaceView : UserControl
{
    private const double InspectorHideThreshold = 132;
    private const double InspectorRestoreWidth = 316;
    private const double InspectorSoftMinimum = 260;
    private ShellViewModel? subscribedViewModel;

    public ChatSurfaceView()
    {
        InitializeComponent();
        Loaded += ChatSurfaceView_Loaded;
        DataContextChanged += ChatSurfaceView_DataContextChanged;
    }

    private ShellViewModel? ViewModel => DataContext as ShellViewModel;

    private void ChatSurfaceView_Loaded(object sender, RoutedEventArgs e)
    {
        ApplyInspectorWidth();
    }

    private void ChatSurfaceView_DataContextChanged(FrameworkElement sender, DataContextChangedEventArgs args)
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

        ApplyInspectorWidth();
    }

    private void ViewModel_PropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is nameof(ShellViewModel.IsChatVisible)
            or nameof(ShellViewModel.InspectorWidth)
            or nameof(ShellViewModel.WorkspaceLayout))
        {
            ApplyInspectorWidth();
        }
    }

    private void InspectorResizeThumb_DragDelta(object sender, DragDeltaEventArgs e)
    {
        if (ViewModel is null)
        {
            return;
        }

        var proposed = ViewModel.InspectorWidth - e.HorizontalChange;
        if (proposed <= InspectorHideThreshold)
        {
            ViewModel.IsChatVisible = false;
        }
        else
        {
            ViewModel.IsChatVisible = true;
            ViewModel.InspectorWidth = Math.Max(InspectorSoftMinimum, proposed);
        }

        ApplyInspectorWidth();
    }

    private void RestoreSidebarButton_Click(object sender, RoutedEventArgs e)
    {
        if (ViewModel is null)
        {
            return;
        }

        ViewModel.SidebarWidth = Math.Max(ViewModel.SidebarWidth, 280);
        ViewModel.IsLibraryVisible = true;
    }

    private void RestoreInspectorButton_Click(object sender, RoutedEventArgs e)
    {
        if (ViewModel is null)
        {
            return;
        }

        ViewModel.InspectorWidth = Math.Max(ViewModel.InspectorWidth, InspectorRestoreWidth);
        ViewModel.IsChatVisible = true;
        ApplyInspectorWidth();
    }

    private void ApplyInspectorWidth()
    {
        if (ViewModel is null)
        {
            return;
        }

        var visible = ViewModel.IsInspectorVisible;
        InspectorSplitterColumn.Width = visible ? new GridLength(1) : new GridLength(0);
        InspectorColumn.Width = visible ? new GridLength(ViewModel.InspectorWidth) : new GridLength(0);
    }
}
