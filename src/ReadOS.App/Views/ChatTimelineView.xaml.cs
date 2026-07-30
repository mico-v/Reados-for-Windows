using System.Collections.Specialized;
using System.ComponentModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;
using ReadOS.App.Models.ChatUi;
using ReadOS.App.ViewModels;

namespace ReadOS.App.Views;

// Code-behind for the canonical MSP Chat UI timeline surface.
//
// The view is a thin WinUI host: it reads ChatTimelineViewModel (exposed on
// ShellViewModel.ChatTimeline) and wires the observable message list to the
// ItemsRepeater. It contains no markdown parsing, streaming-patch, or
// tool-block logic — that lives in the projection layer and the immutable
// ChatUi* contract types.
//
// Header buttons (restore sidebar / inspector) and the compact-layout flyouts
// are ported from ChatSurfaceView so the canonical surface stays usable when
// the workspace chrome is collapsed.

public sealed partial class ChatTimelineView : UserControl
{
    private ShellViewModel? subscribedShell;
    private ChatTimelineViewModel? timelineViewModel;

    public ChatTimelineView()
    {
        InitializeComponent();
        Loaded += OnLoaded;
        Unloaded += OnUnloaded;
    }

    private ShellViewModel? Shell => DataContext as ShellViewModel;

    private ChatTimelineViewModel? TimelineViewModel
    {
        get => timelineViewModel ??= Shell?.ChatTimeline;
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        SubscribeShell(Shell);

        var timeline = TimelineViewModel;
        if (timeline is not null)
        {
            TimelineRepeater.ItemsSource = timeline.Messages;
            timeline.Messages.CollectionChanged += OnMessagesChanged;
            UpdateEmptyState(timeline.Messages.Count);
        }

        // Wire the host bridge's late-bound scroll resolver so renderer-side
        // ScrollToBottom calls land on this ScrollViewer. Mirrors the WebView2
        // host forwarding scroll.sync events to the page.
        if (Shell?.ChatUiHostBridge is { } bridge)
        {
            bridge.ScrollResolver = () => TimelineScroll;
        }
    }

    private void OnUnloaded(object sender, RoutedEventArgs e)
    {
        SubscribeShell(null);

        if (timelineViewModel is not null)
        {
            timelineViewModel.Messages.CollectionChanged -= OnMessagesChanged;
        }
    }

    private void SubscribeShell(ShellViewModel? shell)
    {
        if (ReferenceEquals(subscribedShell, shell)) return;

        if (subscribedShell is not null)
        {
            subscribedShell.PropertyChanged -= Shell_PropertyChanged;
        }

        subscribedShell = shell;
        if (subscribedShell is not null)
        {
            subscribedShell.PropertyChanged += Shell_PropertyChanged;
        }
    }

    private void Shell_PropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        // Compact-pane reactions were removed with the workbench teardown;
        // the canonical surface no longer owns sidebar/inspector flyouts.
    }

    private void OnMessagesChanged(object? sender, NotifyCollectionChangedEventArgs e)
    {
        if (sender is System.Collections.IList list)
        {
            UpdateEmptyState(list.Count);
        }
        else if (sender is INotifyCollectionChanged)
        {
            UpdateEmptyState(timelineViewModel?.Messages.Count ?? 0);
        }

        // Auto-scroll to bottom on new messages so streaming stays in view.
        if (e.Action == NotifyCollectionChangedAction.Add)
        {
            DispatcherQueue.TryEnqueue(() =>
            {
                if (TimelineScroll.ScrollableHeight > 0)
                {
                    TimelineScroll.ChangeView(null, TimelineScroll.ScrollableHeight, null);
                }
            });
        }
    }

    private void UpdateEmptyState(int count)
    {
        if (EmptyState is null) return;
        EmptyState.Visibility = count == 0 ? Visibility.Visible : Visibility.Collapsed;
    }

    // ── Message copy affordance ───────────────────────────────────────
    // Copies the message's concatenated block text via the host bridge
    // (ShellViewModel.OnChatUiMessageCopyRequested does the concatenation).

    private void CopyMessageButton_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not FrameworkElement fe || fe.DataContext is not ChatUiMessage message) return;
        Shell?.ChatUiHostBridge.CopyMessage(message.Id);
    }
}
