using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls.Primitives;
using ReadOS.App.Services;
using ReadOS.App.ViewModels;

namespace ReadOS.App;

public sealed partial class MainWindow : Window
{
    private const double MinSidebarWidth = 260;
    private const double MaxSidebarWidth = 440;

    public MainWindow()
    {
        InitializeComponent();

        ViewModel = new ShellViewModel(new MockWorkspaceService());
        RootShell.DataContext = ViewModel;
    }

    public ShellViewModel ViewModel { get; }

    private void SidebarResizeThumb_DragDelta(object sender, DragDeltaEventArgs e)
    {
        ViewModel.SidebarWidth = Clamp(ViewModel.SidebarWidth + e.HorizontalChange, MinSidebarWidth, MaxSidebarWidth);
        SidebarColumn.Width = new GridLength(ViewModel.SidebarWidth);
    }

    private static double Clamp(double value, double min, double max)
    {
        return Math.Min(Math.Max(value, min), max);
    }
}
