using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using ReadOS.App.ViewModels;

namespace ReadOS.App.Views;

public sealed partial class ChatSurfaceView : UserControl
{
    private const double InspectorRestoreWidth = 340;

    public ChatSurfaceView()
    {
        InitializeComponent();
    }

    private ShellViewModel? ViewModel => DataContext as ShellViewModel;

    private void RestoreSidebarButton_Click(object sender, RoutedEventArgs e)
    {
        if (ViewModel is null) return;

        ViewModel.SidebarWidth = Math.Max(ViewModel.SidebarWidth, 260);
        ViewModel.IsSidebarVisible = true;
    }

    private void RestoreInspectorButton_Click(object sender, RoutedEventArgs e)
    {
        if (ViewModel is null) return;

        ViewModel.InspectorWidth = Math.Max(ViewModel.InspectorWidth, InspectorRestoreWidth);
        ViewModel.IsInspectorVisible = true;
    }
}
