using ReadOS.App.Models;
using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Runtime;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsMspCommandPackFactory
{
    private readonly IWorkspaceStore workspaceStore;
    private readonly IPdfDocumentService pdfService;
    private readonly IAiChatService aiChatService;
    private readonly Func<WorkspaceState?> workspaceProvider;
    private readonly Func<WorkspaceSettings> settingsProvider;
    private readonly Func<LibraryItem?> selectedDocumentProvider;
    private readonly Func<IReadOnlyList<ChatAttachment>> pendingAttachmentsProvider;
    private readonly Func<ChatAttachment, Task<string>> attachmentTextProvider;
    private readonly Action<ChatAttachment> attachmentSink;
    private readonly Action clearAttachments;
    private readonly Action<LibraryItem, ChatConversation> chatResultSink;

    public ReadOsMspCommandPackFactory(
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
        this.workspaceStore = workspaceStore;
        this.pdfService = pdfService;
        this.aiChatService = aiChatService;
        this.workspaceProvider = workspaceProvider;
        this.settingsProvider = settingsProvider;
        this.selectedDocumentProvider = selectedDocumentProvider;
        this.pendingAttachmentsProvider = pendingAttachmentsProvider;
        this.attachmentTextProvider = attachmentTextProvider;
        this.attachmentSink = attachmentSink;
        this.clearAttachments = clearAttachments;
        this.chatResultSink = chatResultSink;
    }

    public MspCommandPack CreateCommandPack()
    {
        return new MspCommandPack(
            "ReadOS app commands",
            new IMspCommand[]
            {
                new ReadOsWorkspaceCommand(workspaceStore, workspaceProvider),
                new ReadOsLibraryCommand(workspaceProvider),
                new ReadOsPdfCommand(workspaceStore, pdfService, workspaceProvider, selectedDocumentProvider),
                new ReadOsWindowsCommand(workspaceStore, selectedDocumentProvider),
                new ReadOsPageLabelCommand(workspaceStore, workspaceProvider, selectedDocumentProvider),
                new ReadOsOutlineCommand(workspaceStore, workspaceProvider, selectedDocumentProvider),
                new ReadOsAttachCommand(workspaceProvider, selectedDocumentProvider, attachmentSink),
                new ReadOsChatCommand(
                    workspaceStore,
                    aiChatService,
                    workspaceProvider,
                    settingsProvider,
                    selectedDocumentProvider,
                    pendingAttachmentsProvider,
                    attachmentTextProvider,
                    clearAttachments,
                    chatResultSink),
                new ReadOsWorkflowCommand(
                    workspaceStore,
                    pdfService,
                    aiChatService,
                    workspaceProvider,
                    settingsProvider,
                    selectedDocumentProvider)
            });
    }
}
