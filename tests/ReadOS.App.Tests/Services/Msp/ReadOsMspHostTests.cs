using Microsoft.UI.Xaml.Media.Imaging;
using ReadOS.App.Models;
using ReadOS.App.Services;
using ReadOS.App.Services.Msp;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsMspHostTests
{
    [Fact]
    public async Task Attach_page_requires_approval_before_queueing_attachment()
    {
        var workspace = CreateWorkspace(out var document);
        var attachments = new List<ChatAttachment>();
        var host = CreateHost(workspace, document, attachmentSink: attachments.Add);

        var pending = await host.ExecuteAsync("attach page current 3", "test-agent");

        Assert.Empty(attachments);
        var pendingAudit = Assert.Single(pending.AuditRecords);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, pendingAudit.Decision);
        Assert.Contains("Queue a PDF page", pendingAudit.Preview.Summary);
        Assert.Contains(document.Id, Assert.Single(pendingAudit.Preview.Targets));
        Assert.Contains("page: 3", pendingAudit.Preview.Details);

        var approved = await host.ExecuteApprovedAsync("attach page current 3", "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        Assert.Equal(MspPolicyDecision.Allow, Assert.Single(approved.AuditRecords).Decision);

        var attachment = Assert.Single(attachments);
        Assert.Equal(AttachmentKind.Page, attachment.Kind);
        Assert.Equal(document.Id, attachment.DocumentId);
        Assert.Equal(3, attachment.StartPage);
        Assert.Equal(3, attachment.EndPage);
    }

    [Fact]
    public async Task Attach_range_queues_page_range_attachment_after_approval()
    {
        var workspace = CreateWorkspace(out var document);
        var attachments = new List<ChatAttachment>();
        var host = CreateHost(workspace, document, attachmentSink: attachments.Add);

        var result = await host.ExecuteApprovedAsync($"attach range {document.Id} 7 4", "test-agent");

        Assert.True(result.Succeeded, result.Stderr);
        Assert.Contains($"attached-range\t{document.Id}\t4\t7", result.Stdout);

        var attachment = Assert.Single(attachments);
        Assert.Equal(AttachmentKind.PageRange, attachment.Kind);
        Assert.Equal(document.Id, attachment.DocumentId);
        Assert.Equal(4, attachment.StartPage);
        Assert.Equal(7, attachment.EndPage);
    }

    [Fact]
    public async Task Chat_ask_requires_approval_before_writing_conversation()
    {
        var workspace = CreateWorkspace(out var document);
        var pendingAttachments = new List<ChatAttachment>
        {
            new()
            {
                Kind = AttachmentKind.Page,
                DocumentId = document.Id,
                Title = "guide.pdf · 第 2 页",
                StartPage = 2,
                EndPage = 2
            }
        };
        var chatUpdates = new List<ChatConversation>();
        var chatService = new TestAiChatService("answer from model");
        var host = CreateHost(
            workspace,
            document,
            chatService,
            pendingAttachmentsProvider: () => pendingAttachments.ToArray(),
            clearAttachments: pendingAttachments.Clear,
            chatResultSink: (_, conversation) => chatUpdates.Add(conversation));

        var pending = await host.ExecuteAsync("chat ask current \"explain attached page\"", "test-agent");

        Assert.Empty(document.Conversations);
        Assert.Single(pendingAttachments);
        var pendingAudit = Assert.Single(pending.AuditRecords);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, pendingAudit.Decision);
        Assert.Contains("configured chat model", pendingAudit.Preview.Summary);
        Assert.Contains(document.Id, Assert.Single(pendingAudit.Preview.Targets));
        Assert.Contains("prompt: explain attached page", pendingAudit.Preview.Details);
        Assert.Contains("queuedAttachments: 1", pendingAudit.Preview.Details);

        var approved = await host.ExecuteApprovedAsync("chat ask current \"explain attached page\"", "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        Assert.Equal(MspPolicyDecision.Allow, Assert.Single(approved.AuditRecords).Decision);
        Assert.Contains("answer from model", approved.Stdout);
        Assert.Empty(pendingAttachments);

        var conversation = Assert.Single(document.Conversations);
        Assert.Same(conversation, Assert.Single(chatUpdates));
        Assert.Equal(2, conversation.Messages.Count);
        Assert.Equal(ChatRole.User, conversation.Messages[0].Role);
        Assert.Equal("MSP", conversation.Messages[0].Author);
        Assert.Equal("explain attached page", conversation.Messages[0].Content);
        Assert.Equal(2, Assert.Single(conversation.Messages[0].Attachments).StartPage);
        Assert.Equal(ChatRole.Assistant, conversation.Messages[1].Role);
        Assert.Equal("answer from model", conversation.Messages[1].Content);
        Assert.Equal("explain attached page", chatService.LastPrompt);
        Assert.Single(chatService.LastAttachments);
    }

    [Fact]
    public async Task Chat_ask_streams_progress_events_after_approval()
    {
        var workspace = CreateWorkspace(out var document);
        var chatService = new TestAiChatService("streamed answer");
        var host = CreateHost(workspace, document, chatService);
        var events = new List<MspCommandEvent>();

        await foreach (var commandEvent in host.ExecuteApprovedStreamingAsync(
            "chat ask current \"summarize progress\"",
            "test-agent"))
        {
            events.Add(commandEvent);
        }

        Assert.Contains(events, item =>
            item.Kind == MspCommandEventKind.PolicyDecision &&
            item.Decision == MspPolicyDecision.Allow);
        Assert.Contains(events, item =>
            item.Kind == MspCommandEventKind.Progress &&
            item.Message.Contains("Preparing chat request", StringComparison.Ordinal));
        Assert.Contains(events, item =>
            item.Kind == MspCommandEventKind.Progress &&
            item.Message.Contains("Calling chat model", StringComparison.Ordinal));
        Assert.Contains(events, item =>
            item.Kind == MspCommandEventKind.Progress &&
            item.Message.Contains("Chat answer saved", StringComparison.Ordinal));
        var completed = events[^1];
        Assert.Equal(MspCommandEventKind.Completed, completed.Kind);
        Assert.Equal(0, completed.ExitCode);
        Assert.Equal("reados-workbench", completed.SessionId);
        Assert.Single(document.Conversations);
    }

    [Fact]
    public async Task Artifact_write_persists_provenance_after_approval()
    {
        var workspace = CreateWorkspace(out var document);
        var host = CreateHost(workspace, document);
        const string commandText = "artifact write /artifacts/notes.md \"agent notes\"";

        var pending = await host.ExecuteAsync(commandText, "test-agent");

        Assert.Empty(workspace.Artifacts);
        var pendingAudit = Assert.Single(pending.AuditRecords);
        Assert.Equal(MspPolicyDecision.RequireConfirmation, pendingAudit.Decision);
        Assert.Contains("/artifacts/notes.md", Assert.Single(pendingAudit.Preview.Targets));

        var approved = await host.ExecuteApprovedAsync(commandText, "test-agent");

        Assert.True(approved.Succeeded, approved.Stderr);
        Assert.Equal(MspPolicyDecision.Allow, Assert.Single(approved.AuditRecords).Decision);

        var resultArtifact = Assert.Single(approved.Artifacts);
        Assert.Equal("/artifacts/notes.md", resultArtifact.Path);
        Assert.Equal("test-agent", resultArtifact.Actor);
        Assert.Equal("reados-workbench", resultArtifact.SessionId);

        var persisted = Assert.Single(workspace.Artifacts);
        Assert.Equal("/artifacts/notes.md", persisted.Path);
        Assert.Equal("agent notes", persisted.Content);
        Assert.Equal("text/markdown", persisted.MediaType);
        Assert.Equal(commandText, persisted.SourceCommand);
        Assert.Equal("test-agent", persisted.Actor);
        Assert.Equal("reados-workbench", persisted.SessionId);
        Assert.Equal("contentLength: 11", persisted.Preview);
    }

    private static ReadOsMspHost CreateHost(
        WorkspaceState workspace,
        LibraryItem document,
        IAiChatService? chatService = null,
        Func<IReadOnlyList<ChatAttachment>>? pendingAttachmentsProvider = null,
        Action<ChatAttachment>? attachmentSink = null,
        Action? clearAttachments = null,
        Action<LibraryItem, ChatConversation>? chatResultSink = null)
    {
        return new ReadOsMspHost(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            chatService ?? new TestAiChatService("unused"),
            () => workspace,
            () => workspace.Settings,
            () => document,
            pendingAttachmentsProvider ?? (() => Array.Empty<ChatAttachment>()),
            _ => Task.FromResult("attachment text"),
            attachmentSink ?? (_ => { }),
            clearAttachments ?? (() => { }),
            chatResultSink ?? ((_, _) => { }));
    }

    private static WorkspaceState CreateWorkspace(out LibraryItem document)
    {
        var workspace = new WorkspaceState();
        var project = new ProjectItem
        {
            Name = "Project"
        };
        document = new LibraryItem
        {
            ProjectId = project.Id,
            Kind = LibraryItemKind.Pdf,
            Name = "guide.pdf",
            PageCount = 12,
            CurrentPage = 1
        };
        project.LibraryItems.Add(document);
        workspace.Projects.Add(project);
        return workspace;
    }

    private sealed class TestWorkspaceStore : IWorkspaceStore
    {
        private readonly WorkspaceState workspace;

        public TestWorkspaceStore(WorkspaceState workspace)
        {
            this.workspace = workspace;
        }

        public string WorkspaceRoot => "V:\\ReadOS-Test";

        public string LibraryRoot => "V:\\ReadOS-Test\\Library";

        public Task<WorkspaceState> LoadAsync(CancellationToken cancellationToken = default)
        {
            return Task.FromResult(workspace);
        }

        public Task SaveAsync(WorkspaceState state, CancellationToken cancellationToken = default)
        {
            return Task.CompletedTask;
        }

        public Task<ProjectItem> CreateProjectAsync(WorkspaceState state, string name, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task<LibraryItem> ImportDocumentAsync(WorkspaceState state, ProjectItem project, string sourcePath, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task RenameDocumentAsync(WorkspaceState state, LibraryItem document, string newName, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task DeleteDocumentAsync(WorkspaceState state, ProjectItem project, LibraryItem document, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task<string> ExportWorkspaceAsync(WorkspaceState state, string destinationPath, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task<WorkspaceState> ImportWorkspaceAsync(string sourcePath, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public string GetAbsolutePath(LibraryItem item)
        {
            return item.RelativePath;
        }
    }

    private sealed class TestPdfDocumentService : IPdfDocumentService
    {
        public Task<PdfDocumentInfo> InspectAsync(string path, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task<BitmapImage> RenderPageAsync(string path, int pageNumber, double width, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task<IReadOnlyList<PageImageItem>> RenderThumbnailsAsync(string path, int pageCount, int maxPages, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task<IReadOnlyList<PdfTextHit>> SearchAsync(string path, string query, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task<string> ExtractPageTextAsync(string path, int startPage, int endPage, CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }
    }

    private sealed class TestAiChatService : IAiChatService
    {
        private readonly string response;

        public TestAiChatService(string response)
        {
            this.response = response;
        }

        public string? LastPrompt { get; private set; }

        public IReadOnlyList<ChatAttachment> LastAttachments { get; private set; } = Array.Empty<ChatAttachment>();

        public Task<string> SendAsync(
            WorkspaceSettings settings,
            LibraryItem? document,
            IEnumerable<ChatMessage> history,
            string userPrompt,
            IEnumerable<ChatAttachment> attachments,
            Func<ChatAttachment, Task<string>> attachmentTextProvider,
            string? mspInstruction = null,
            string? mspExecutionContext = null,
            bool allowMspCommandRequests = true,
            CancellationToken cancellationToken = default)
        {
            LastPrompt = userPrompt;
            LastAttachments = attachments.ToArray();
            return Task.FromResult(response);
        }
    }
}
