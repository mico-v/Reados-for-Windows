using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media.Imaging;
using ReadOS.App.Models;
using ReadOS.App.Services;
using ReadOS.App.ViewModels;

namespace ReadOS.App.Tests.ViewModels;

public sealed class ShellViewModelTests
{
    [Fact]
    public async Task Run_msp_command_records_durable_session()
    {
        var workspace = new WorkspaceState();
        var workspaceStore = new TestWorkspaceStore(workspace);
        var viewModel = new ShellViewModel(
            workspaceStore,
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        viewModel.MspCommandDraft = "workspace info";

        await viewModel.RunMspCommandCommand.ExecuteAsync(null);

        var transcript = Assert.Single(workspace.MspTranscript);
        Assert.Equal("reados-workbench", transcript.SessionId);
        Assert.Equal("workspace info", transcript.CommandText);
        Assert.True(transcript.Succeeded, transcript.Stderr);

        var session = Assert.Single(workspace.MspSessions);
        Assert.Equal("reados-workbench", session.Id);
        Assert.Equal(1, session.CommandCount);
        Assert.Equal("workspace info", session.LastCommandText);
        Assert.Equal("Allow", session.LastDecision);
        Assert.Equal(transcript.Id, Assert.Single(session.TranscriptIds));
        Assert.Empty(session.ArtifactPaths);
        Assert.True(workspaceStore.SaveCount > 0);
    }

    [Fact]
    public async Task Run_msp_command_records_policy_diagnostics_in_session()
    {
        var workspace = new WorkspaceState();
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        viewModel.MspCommandDraft = "artifact write /artifacts/pending.md \"pending\"";

        await viewModel.RunMspCommandCommand.ExecuteAsync(null);

        var transcript = Assert.Single(workspace.MspTranscript);
        Assert.Equal("RequireConfirmation", transcript.Decision);
        Assert.Contains("msp.policy.require_confirmation", transcript.DiagnosticsSummary);
        Assert.Contains("Approve", transcript.RecoveryHint);

        var session = Assert.Single(workspace.MspSessions);
        Assert.Equal(1, session.FailureCount);
        Assert.Contains("msp.policy.require_confirmation", session.LastDiagnosticsSummary);
        Assert.Contains("Approve", session.LastRecoveryHint);
    }

    [Fact]
    public async Task Initialize_rebuilds_sessions_and_removes_stale_records()
    {
        var workspace = new WorkspaceState();
        workspace.MspTranscript.Add(new MspTranscriptEntry
        {
            Id = "transcript-1",
            SessionId = string.Empty,
            Actor = "agent",
            CommandText = "workspace info",
            StartedAt = new DateTimeOffset(2026, 6, 30, 8, 0, 0, TimeSpan.Zero),
            CompletedAt = new DateTimeOffset(2026, 6, 30, 8, 0, 1, TimeSpan.Zero),
            Decision = "Allow"
        });
        workspace.MspSessions.Add(new MspSessionEntry
        {
            Id = "stale-session",
            Title = "Stale session"
        });

        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();

        var transcript = Assert.Single(workspace.MspTranscript);
        Assert.Equal("reados-workbench", transcript.SessionId);

        var session = Assert.Single(workspace.MspSessions);
        Assert.Equal("reados-workbench", session.Id);
        Assert.Equal("workspace info", session.LastCommandText);
        Assert.Equal("transcript-1", Assert.Single(session.TranscriptIds));

        var visibleSession = Assert.Single(viewModel.MspSessions);
        Assert.Equal("reados-workbench", visibleSession.Id);
        Assert.Equal("workspace info", visibleSession.LastCommandText);
        Assert.Contains(viewModel.TimelineItems, item =>
            item.Kind == TimelineItemKind.MspCommand &&
            item.MspEntry?.Id == "transcript-1");
        Assert.Equal("1 条 MSP 执行记录", viewModel.MspActivitySummary);
    }

    [Fact]
    public async Task Initialize_projects_artifacts_into_sessions_and_timeline()
    {
        var updatedAt = new DateTimeOffset(2026, 7, 1, 9, 30, 0, TimeSpan.Zero);
        var workspace = new WorkspaceState();
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Id = "artifact-1",
            Path = "/artifacts/brief.md",
            Content = "full artifact content",
            Preview = "artifact preview",
            Description = "Investigation brief",
            MediaType = "text/markdown",
            SourceCommand = "artifact write /artifacts/brief.md",
            SessionId = "artifact-session",
            CreatedAt = updatedAt,
            UpdatedAt = updatedAt
        });

        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();

        var artifact = Assert.Single(viewModel.Artifacts);
        Assert.Equal("/artifacts/brief.md", artifact.Path);
        Assert.Equal("1 个产物", viewModel.ArtifactSummary);
        viewModel.SelectedArtifact = artifact;
        Assert.Equal("full artifact content", viewModel.SelectedArtifactPreview);

        var session = Assert.Single(viewModel.MspSessions);
        Assert.Equal("artifact-session", session.Id);
        Assert.Equal("/artifacts/brief.md", Assert.Single(session.ArtifactPaths));

        var timelineArtifact = Assert.Single(viewModel.TimelineItems, item => item.Kind == TimelineItemKind.Artifact);
        Assert.Equal("/artifacts/brief.md", timelineArtifact.Artifact?.Path);
        Assert.Equal("artifact preview", timelineArtifact.Body);
    }

    [Fact]
    public async Task Search_filters_sessions_and_artifacts()
    {
        var workspace = new WorkspaceState();
        workspace.MspTranscript.Add(new MspTranscriptEntry
        {
            Id = "transcript-beta",
            SessionId = "beta-session",
            Actor = "agent",
            CommandText = "workspace status",
            StartedAt = new DateTimeOffset(2026, 7, 1, 10, 0, 0, TimeSpan.Zero),
            CompletedAt = new DateTimeOffset(2026, 7, 1, 10, 0, 1, TimeSpan.Zero),
            Decision = "Allow"
        });
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Id = "artifact-alpha",
            Path = "/artifacts/alpha-report.md",
            Content = "alpha artifact content",
            Description = "Alpha report",
            SourceCommand = "artifact write /artifacts/alpha-report.md",
            SessionId = "alpha-session",
            UpdatedAt = new DateTimeOffset(2026, 7, 1, 11, 0, 0, TimeSpan.Zero)
        });

        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();

        Assert.Equal(2, viewModel.MspSessions.Count);
        Assert.Single(viewModel.Artifacts);

        viewModel.SearchQuery = "alpha";

        var alphaSession = Assert.Single(viewModel.MspSessions);
        Assert.Equal("alpha-session", alphaSession.Id);
        Assert.Equal("/artifacts/alpha-report.md", Assert.Single(viewModel.Artifacts).Path);

        viewModel.SearchQuery = "workspace";

        var betaSession = Assert.Single(viewModel.MspSessions);
        Assert.Equal("beta-session", betaSession.Id);
        Assert.Empty(viewModel.Artifacts);
    }

    [Fact]
    public async Task Attach_selected_artifact_queues_virtual_file_attachment()
    {
        var workspace = new WorkspaceState();
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Id = "artifact-1",
            Path = "/artifacts/brief.md",
            Content = "full artifact content",
            Description = "Brief",
            SessionId = "artifact-session"
        });

        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            new TestAiChatService());

        await viewModel.InitializeAsync();
        viewModel.SelectedArtifact = Assert.Single(viewModel.Artifacts);

        viewModel.AttachSelectedArtifactCommand.Execute(null);

        var attachment = Assert.Single(viewModel.PendingAttachments);
        Assert.Equal(AttachmentKind.File, attachment.Kind);
        Assert.Equal("/artifacts/brief.md", attachment.FilePath);
        Assert.Equal("产物 · /artifacts/brief.md", attachment.Title);
        Assert.Equal("请基于我附加的产物继续分析。", viewModel.ComposerDraft);
        Assert.Equal(InspectorTab.Evidence, viewModel.SelectedInspectorTab);
    }

    [Fact]
    public async Task Artifact_attachment_content_is_sent_to_chat_service()
    {
        var workspace = new WorkspaceState();
        workspace.Projects.Add(new ProjectItem
        {
            Id = "project-1",
            Name = "Project"
        });
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Id = "artifact-1",
            Path = "/artifacts/brief.md",
            Content = "full artifact content",
            Description = "Brief",
            SessionId = "artifact-session"
        });
        var chatService = new TestAiChatService();
        var viewModel = new ShellViewModel(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            new TestFileDialogService(),
            chatService);

        await viewModel.InitializeAsync();
        viewModel.SelectedArtifact = Assert.Single(viewModel.Artifacts);
        viewModel.AttachSelectedArtifactCommand.Execute(null);

        await viewModel.SendPromptCommand.ExecuteAsync(null);

        Assert.Equal("请基于我附加的产物继续分析。", chatService.LastPrompt);
        Assert.Equal("/artifacts/brief.md", Assert.Single(chatService.LastAttachments).FilePath);
        Assert.Equal("full artifact content", Assert.Single(chatService.LastAttachmentTexts));
        var conversation = Assert.Single(workspace.Projects[0].StandaloneConversations);
        Assert.Equal(2, conversation.Messages.Count);
    }

    private sealed class TestWorkspaceStore : IWorkspaceStore
    {
        private readonly WorkspaceState workspace;

        public TestWorkspaceStore(WorkspaceState workspace)
        {
            this.workspace = workspace;
        }

        public int SaveCount { get; private set; }

        public string WorkspaceRoot => "V:\\ReadOS-Test";

        public string LibraryRoot => "V:\\ReadOS-Test\\Library";

        public Task<WorkspaceState> LoadAsync(CancellationToken cancellationToken = default)
        {
            return Task.FromResult(workspace);
        }

        public Task SaveAsync(WorkspaceState state, CancellationToken cancellationToken = default)
        {
            SaveCount++;
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

    private sealed class TestFileDialogService : IFileDialogService
    {
        public Task<IReadOnlyList<string>> PickPdfFilesAsync(Window window)
        {
            return Task.FromResult<IReadOnlyList<string>>(Array.Empty<string>());
        }

        public Task<string?> PickWorkspaceImportAsync(Window window)
        {
            return Task.FromResult<string?>(null);
        }

        public Task<string?> PickWorkspaceExportAsync(Window window, string suggestedName)
        {
            return Task.FromResult<string?>(null);
        }
    }

    private sealed class TestAiChatService : IAiChatService
    {
        public string Response { get; set; } = "unused";

        public string LastPrompt { get; private set; } = string.Empty;

        public IReadOnlyList<ChatAttachment> LastAttachments { get; private set; } = Array.Empty<ChatAttachment>();

        public IReadOnlyList<string> LastAttachmentTexts { get; private set; } = Array.Empty<string>();

        public async Task<string> SendAsync(
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
            var capturedAttachments = attachments.ToArray();
            LastAttachments = capturedAttachments;

            var attachmentTexts = new List<string>();
            foreach (var attachment in capturedAttachments)
            {
                attachmentTexts.Add(await attachmentTextProvider(attachment));
            }

            LastAttachmentTexts = attachmentTexts;
            return Response;
        }
    }
}
