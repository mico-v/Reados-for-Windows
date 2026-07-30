using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using ReadOS.App.Models.ChatUi;

namespace ReadOS.App.ViewModels;

// View-model for the canonical MSP Chat UI timeline surface.
//
// This is the WinUI host renderer entry point. It holds the current canonical
// timeline state and exposes an observable message list the XAML view binds to.
// Render operations from ChatUiRenderPlanner decide between a full re-render and
// an incremental update, mirroring how a WebView2 host would call
// renderTimeline / applyRuntimeEvent.
//
// The view-model does not own projection (that is ReadOsChatUiProjectionService)
// or render planning (that is ChatUiRenderPlanner). It only adapts canonical
// timeline state into observable WinUI binding state, matching the thin-host
// boundary: the host loads the renderer and forwards state, it does not contain
// markdown/tool-block/streaming-patch logic.

public sealed class ChatTimelineViewModel : ObservableObject
{
    private ChatUiTimeline? current;

    public ObservableCollection<ChatUiMessage> Messages { get; } = new();

    public ChatUiTimeline? Current
    {
        get => current;
        private set => SetProperty(ref current, value);
    }

    // Full (re)render: replace the observable message list from a canonical timeline.
    // Used on first render, timeline.replace, and when the planner picks fullRender.
    public void RenderTimeline(ChatUiTimeline timeline)
    {
        Current = timeline;
        Messages.Clear();
        foreach (var message in timeline.Messages)
        {
            Messages.Add(message);
        }
    }

    // Apply a runtime event through the reducer, then re-render.
    // A future slice can route this through ChatUiRenderPlanner to choose
    // incremental updates; for now the canonical reducer is the source of truth
    // and a full re-render keeps the observable list consistent.
    public void ApplyRuntimeEvent(ChatUiRuntimeEvent runtimeEvent)
    {
        var next = ChatUiTimelineStore.ApplyRuntimeEvent(Current, runtimeEvent);
        RenderTimeline(next);
    }

    // Incremental update using the render planner. Returns the planned operation
    // so callers/tests can assert which path was taken.
    public ChatUiRenderOperation ApplyTimeline(ChatUiTimeline next)
    {
        var operation = ChatUiRenderPlanner.Plan(Current, next);
        switch (operation)
        {
            case ChatUiScrollSyncOperation:
                break;
            case ChatUiFullRenderOperation:
                RenderTimeline(next);
                break;
            default:
                // presentationOnly, payloadPatch, directStreaming: re-render to stay consistent.
                // A future slice can apply these incrementally for finer-grained updates.
                RenderTimeline(next);
                break;
        }
        return operation;
    }

    public void Clear()
    {
        Current = null;
        Messages.Clear();
    }
}
