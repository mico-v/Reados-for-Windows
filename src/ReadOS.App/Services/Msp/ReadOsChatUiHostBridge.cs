using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using ReadOS.App.Services;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Services.Msp;

// ReadOS-side implementation of the MSP Chat UI host bridge.
//
// Mirrors Hosts/Windows/src/msp-chat-ui-webview2-host.ts: a WebView2 host
// would forward these to chrome.webview.postMessage; the XAML host forwards
// them to the existing ReadOS services instead. The bridge is the only place
// renderer code calls back into platform behaviour, and it must remain thin —
// no markdown, no streaming patch, no tool-block logic in here.

public sealed class ReadOsChatUiHostBridge : IChatUiHostBridge
{
    private readonly IClipboardService clipboard;
    private readonly IFileDialogService fileDialog;
    private Func<ScrollViewer?>? scrollResolver;

    public ReadOsChatUiHostBridge(
        IClipboardService clipboard,
        IFileDialogService fileDialog)
    {
        this.clipboard = clipboard;
        this.fileDialog = fileDialog;
    }

    // Late-bound resolver so the host can be constructed before the XAML view
    // loads its ScrollViewer. The view sets this on Loaded.
    public Func<ScrollViewer?>? ScrollResolver
    {
        get => scrollResolver;
        set => scrollResolver = value;
    }

    public void SendHostMessage(string channel, object? payload)
    {
        // The Default renderer manifest declares host channels such as
        // messageAction, openAttachment, copyCode, presentationProbe, etc.
        // We currently route copy/open explicitly below. Other channels are
        // surfaced to the shell via Raised so host-specific UI (compact menu,
        // selection overlay) can react when needed.
        Raised?.Invoke(this, new HostMessageEventArgs(channel, payload));
    }

    public void CopyMessage(string messageId)
    {
        // The actual message text is owned by the view-model; the shell listens
        // to MessageCopyRequested to resolve the message text and copy it.
        MessageCopyRequested?.Invoke(this, messageId);
    }

    public void OpenAttachment(string attachmentId)
    {
        Raised?.Invoke(this, new HostMessageEventArgs("openAttachment", attachmentId));
    }

    public void OpenArtifact(string artifactPath)
    {
        Raised?.Invoke(this, new HostMessageEventArgs("openArtifact", artifactPath));
    }

    public void ScrollToBottom()
    {
        if (scrollResolver?.Invoke() is { } scroll)
        {
            scroll.ChangeView(null, scroll.ScrollableHeight, null);
        }
    }

    public void RequestSelectionMenu(double x, double y)
    {
        Raised?.Invoke(this, new HostMessageEventArgs("selectionMenu", new { x, y }));
    }

    public void ToggleMessageAction(string messageId, string action)
    {
        Raised?.Invoke(this, new HostMessageEventArgs("messageAction", new { messageId, action }));
    }

    public event EventHandler<HostMessageEventArgs>? Raised;
    public event EventHandler<string>? MessageCopyRequested;
}

public sealed class HostMessageEventArgs : EventArgs
{
    public HostMessageEventArgs(string channel, object? payload)
    {
        Channel = channel;
        Payload = payload;
    }

    public string Channel { get; }
    public object? Payload { get; }
}
