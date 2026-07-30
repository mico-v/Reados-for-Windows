using ReadOS.App.Models.ChatUi;

namespace ReadOS.App.Services.Msp;

// Thin host bridge contract for the MSP Chat UI Windows host.
//
// Mirrors the Hosts/Windows boundary: the host loads a renderer (here, the
// XAML ChatTimelineView), passes timelines/runtime events into it, and bridges
// platform actions. It must not contain renderer DOM/markdown/tool-block logic.
//
// WinUI is a legitimate Windows host. A WebView2 host would implement the same
// contract by forwarding to chrome.webview.postMessage; the XAML host forwards
// to view-model commands. The bridge is the only place renderer code calls back
// into platform behavior.

public interface IChatUiHostBridge
{
    void SendHostMessage(string channel, object? payload);

    void CopyMessage(string messageId);

    void OpenAttachment(string attachmentId);

    void OpenArtifact(string artifactPath);

    void ScrollToBottom();

    void RequestSelectionMenu(double x, double y);

    void ToggleMessageAction(string messageId, string action);
}

// No-op default used by tests and before a real host is wired up.
public sealed class NullChatUiHostBridge : IChatUiHostBridge
{
    public static NullChatUiHostBridge Instance { get; } = new();

    public void SendHostMessage(string channel, object? payload) { }
    public void CopyMessage(string messageId) { }
    public void OpenAttachment(string attachmentId) { }
    public void OpenArtifact(string artifactPath) { }
    public void ScrollToBottom() { }
    public void RequestSelectionMenu(double x, double y) { }
    public void ToggleMessageAction(string messageId, string action) { }
}
