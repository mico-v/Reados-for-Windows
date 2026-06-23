using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls.Primitives;
using ReadOS.App.Services;
using ReadOS.App.ViewModels;

namespace ReadOS.App;

public sealed partial class MainWindow : Window
{
    private const double MinLibraryPaneWidth = 220;
    private const double MaxLibraryPaneWidth = 560;
    private const double MinChatPaneWidth = 320;
    private const double MaxChatPaneWidth = 680;
    private const double MinOutlinePaneWidth = 220;
    private const double MaxOutlinePaneWidth = 460;

    public MainWindow()
    {
        InitializeComponent();

        ViewModel = new ShellViewModel(new MockWorkspaceService());
        RootShell.DataContext = ViewModel;
    }

    public ShellViewModel ViewModel { get; }

    private void LibraryResizeThumb_DragDelta(object sender, DragDeltaEventArgs e)
    {
        ViewModel.LibraryPaneWidth = Clamp(ViewModel.LibraryPaneWidth + e.HorizontalChange, MinLibraryPaneWidth, MaxLibraryPaneWidth);
        LibrarySplitView.OpenPaneLength = ViewModel.LibraryPaneWidth;
    }

    private void ChatResizeThumb_DragDelta(object sender, DragDeltaEventArgs e)
    {
        ViewModel.ChatPaneWidth = Clamp(ViewModel.ChatPaneWidth - e.HorizontalChange, MinChatPaneWidth, MaxChatPaneWidth);
        ChatSplitView.OpenPaneLength = ViewModel.ChatPaneWidth;
    }

    private void OutlineResizeThumb_DragDelta(object sender, DragDeltaEventArgs e)
    {
        ViewModel.OutlinePaneWidth = Clamp(ViewModel.OutlinePaneWidth - e.HorizontalChange, MinOutlinePaneWidth, MaxOutlinePaneWidth);
        OutlineColumn.Width = new GridLength(ViewModel.OutlinePaneWidth);
    }

    private static double Clamp(double value, double min, double max)
    {
        return Math.Min(Math.Max(value, min), max);
    }
}
