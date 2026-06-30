using Microsoft.UI.Xaml.Media.Imaging;
using ReadOS.App.Models;
using ReadOS.App.Services;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsVirtualWorkspaceTests
{
    [Fact]
    public async Task Virtual_workspace_lists_and_reads_transcript_records()
    {
        var workspace = new WorkspaceState();
        var transcript = new MspTranscriptEntry
        {
            Actor = "test-agent",
            CommandText = "workspace info",
            ExitCode = 0,
            Stdout = "ok",
            Decision = "Allow",
            Effects = "ReadWorkspace"
        };
        workspace.MspTranscript.Add(transcript);

        var virtualWorkspace = new ReadOsVirtualWorkspace(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            () => workspace);

        var entries = await virtualWorkspace.ListAsync("/transcripts");
        var entry = Assert.Single(entries);
        var content = await virtualWorkspace.TryReadTextAsync(entry.Path);

        Assert.False(entry.IsDirectory);
        Assert.Equal($"/transcripts/{transcript.Id}.json", entry.Path);
        Assert.Contains("\"commandText\": \"workspace info\"", content);
        Assert.Contains("\"decision\": \"Allow\"", content);
        Assert.Contains("\"effects\": \"ReadWorkspace\"", content);
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
}
