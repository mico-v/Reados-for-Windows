using Microsoft.UI.Xaml.Media.Imaging;
using ReadOS.App.Models;
using ReadOS.App.Services;
using ReadOS.App.Services.Msp;
using ReadOS.Msp.Models;

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

    [Fact]
    public async Task Virtual_workspace_persists_artifact_provenance_and_manifest()
    {
        var workspace = new WorkspaceState();
        var virtualWorkspace = new ReadOsVirtualWorkspace(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            () => workspace);

        await virtualWorkspace.WriteTextAsync(
            "/artifacts/report.md",
            "# Report",
            new MspArtifact
            {
                Path = "/artifacts/report.md",
                MediaType = "text/markdown",
                Description = "Generated report",
                SourceCommand = "artifact write /artifacts/report.md report",
                Actor = "test-agent",
                SessionId = "session-7",
                SourceDocuments = new[] { "doc-1" },
                SourcePages = new[] { "doc-1:2-3" },
                Preview = "contentLength: 8"
            });

        var artifact = Assert.Single(workspace.Artifacts);
        Assert.Equal("/artifacts/report.md", artifact.Path);
        Assert.Equal("text/markdown", artifact.MediaType);
        Assert.Equal("Generated report", artifact.Description);
        Assert.Equal("artifact write /artifacts/report.md report", artifact.SourceCommand);
        Assert.Equal("test-agent", artifact.Actor);
        Assert.Equal("session-7", artifact.SessionId);
        Assert.Equal("doc-1", Assert.Single(artifact.SourceDocuments));
        Assert.Equal("doc-1:2-3", Assert.Single(artifact.SourcePages));

        var entries = await virtualWorkspace.ListAsync("/artifacts");
        Assert.Contains(entries, entry => entry.Path == "/artifacts/report.md");
        Assert.Contains(entries, entry => entry.Path == "/artifacts/report.md.manifest.json" && entry.MediaType == "application/json");

        var manifest = await virtualWorkspace.TryReadTextAsync("/artifacts/report.md.manifest.json");
        Assert.NotNull(manifest);
        Assert.Contains("\"sourceCommand\": \"artifact write /artifacts/report.md report\"", manifest);
        Assert.Contains("\"actor\": \"test-agent\"", manifest);
        Assert.Contains("\"sessionId\": \"session-7\"", manifest);
        Assert.Contains("\"sourceDocuments\": [", manifest);
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
