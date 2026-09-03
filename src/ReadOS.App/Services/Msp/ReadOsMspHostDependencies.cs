using ReadOS.App.Models;
using ReadOS.Msp.Hosting.Native;
using ReadOS.Msp.Hosting.Native.RuntimeFfi;

namespace ReadOS.App.Services.Msp;

public sealed class ReadOsMspHostDependencies
{
    public ReadOsMspHostDependencies(
        IWorkspaceStore workspaceStore,
        IPdfDocumentService pdfService,
        IAiChatService aiChatService,
        Func<WorkspaceState?> workspaceProvider,
        Func<WorkspaceSettings> settingsProvider,
        Func<LibraryItem?> selectedDocumentProvider,
        Func<IReadOnlyList<ChatAttachment>> pendingAttachmentsProvider,
        Func<ChatAttachment, Task<string>> attachmentTextProvider,
        Action<ChatAttachment> attachmentSink,
        Action clearAttachments,
        Action<LibraryItem, ChatConversation> chatResultSink,
        MspCommandRuntimeFfiEchoCommandAdapter? runtimeFfiEchoCommandAdapter = null,
        MspVerifiedRuntimeProviderCatalog? runtimeProviderCatalog = null)
    {
        WorkspaceStore = workspaceStore ?? throw new ArgumentNullException(nameof(workspaceStore));
        PdfService = pdfService ?? throw new ArgumentNullException(nameof(pdfService));
        AiChatService = aiChatService ?? throw new ArgumentNullException(nameof(aiChatService));
        WorkspaceProvider = workspaceProvider ?? throw new ArgumentNullException(nameof(workspaceProvider));
        SettingsProvider = settingsProvider ?? throw new ArgumentNullException(nameof(settingsProvider));
        SelectedDocumentProvider = selectedDocumentProvider ?? throw new ArgumentNullException(nameof(selectedDocumentProvider));
        PendingAttachmentsProvider = pendingAttachmentsProvider ?? throw new ArgumentNullException(nameof(pendingAttachmentsProvider));
        AttachmentTextProvider = attachmentTextProvider ?? throw new ArgumentNullException(nameof(attachmentTextProvider));
        AttachmentSink = attachmentSink ?? throw new ArgumentNullException(nameof(attachmentSink));
        ClearAttachments = clearAttachments ?? throw new ArgumentNullException(nameof(clearAttachments));
        ChatResultSink = chatResultSink ?? throw new ArgumentNullException(nameof(chatResultSink));
        RuntimeFfiEchoCommandAdapter = runtimeFfiEchoCommandAdapter;
        RuntimeProviderCatalog = runtimeProviderCatalog;
    }

    public IWorkspaceStore WorkspaceStore { get; }

    public IPdfDocumentService PdfService { get; }

    public IAiChatService AiChatService { get; }

    public Func<WorkspaceState?> WorkspaceProvider { get; }

    public Func<WorkspaceSettings> SettingsProvider { get; }

    public Func<LibraryItem?> SelectedDocumentProvider { get; }

    public Func<IReadOnlyList<ChatAttachment>> PendingAttachmentsProvider { get; }

    public Func<ChatAttachment, Task<string>> AttachmentTextProvider { get; }

    public Action<ChatAttachment> AttachmentSink { get; }

    public Action ClearAttachments { get; }

    public Action<LibraryItem, ChatConversation> ChatResultSink { get; }

    /// <summary>
    /// Optional, explicitly supplied command-runtime FFI echo route. The
    /// product composition leaves this null unless a package/configuration
    /// owner has loaded and supplied the matching adapter. The supplied
    /// adapter/library remains caller-owned; the host does not auto-load or
    /// dispose it.
    /// </summary>
    public MspCommandRuntimeFfiEchoCommandAdapter? RuntimeFfiEchoCommandAdapter { get; }

    /// <summary>
    /// Optional host-owned verified provider catalog. Android/Linux hosts can
    /// supply registrations without changing the Windows product command pack;
    /// an absent catalog is an empty, fail-closed catalog.
    /// </summary>
    public MspVerifiedRuntimeProviderCatalog? RuntimeProviderCatalog { get; }
}
