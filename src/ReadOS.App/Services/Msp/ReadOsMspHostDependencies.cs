using ReadOS.App.Models;

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
        Action<LibraryItem, ChatConversation> chatResultSink)
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
}
