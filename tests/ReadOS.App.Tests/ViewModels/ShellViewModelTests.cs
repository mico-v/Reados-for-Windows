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
            return Task.FromResult("unused");
        }
    }
}
