using System.Text.Json;
using System.Text.Json.Serialization;
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
            SessionId = "session-test",
            CommandText = "workspace info",
            ExitCode = 0,
            Stdout = "ok",
            Decision = "Allow",
            Effects = "ReadWorkspace",
            ProgressMessage = "complete",
            ProgressPercent = 100
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
        Assert.Contains("\"sessionId\": \"session-test\"", content);
        Assert.Contains("\"commandText\": \"workspace info\"", content);
        Assert.Contains("\"decision\": \"Allow\"", content);
        Assert.Contains("\"effects\": \"ReadWorkspace\"", content);
        Assert.Contains("\"progressMessage\": \"complete\"", content);
        Assert.Contains("\"progressPercent\": 100", content);
    }

    [Fact]
    public async Task Virtual_workspace_lists_and_reads_session_records()
    {
        var workspace = new WorkspaceState();
        var session = new MspSessionEntry
        {
            Id = "reados-workbench",
            Title = "ReadOS Workbench MSP Session",
            Actor = "test-agent",
            StartedAt = new DateTimeOffset(2026, 6, 30, 9, 0, 0, TimeSpan.Zero),
            UpdatedAt = new DateTimeOffset(2026, 6, 30, 9, 2, 0, TimeSpan.Zero),
            LastCommandText = "artifact write /artifacts/report.md report",
            LastDecision = "Allow",
            LastExitCode = 0,
            LastProgressMessage = "Command execution completed.",
            CommandCount = 2,
            ApprovalCount = 1
        };
        session.TranscriptIds.Add("transcript-1");
        session.TranscriptIds.Add("transcript-2");
        session.ArtifactPaths.Add("/artifacts/report.md");
        workspace.MspSessions.Add(session);

        var virtualWorkspace = new ReadOsVirtualWorkspace(
            new TestWorkspaceStore(workspace),
            new TestPdfDocumentService(),
            () => workspace);

        var entries = await virtualWorkspace.ListAsync("/sessions");
        var entry = Assert.Single(entries);
        var content = await virtualWorkspace.TryReadTextAsync(entry.Path);

        Assert.Equal("/sessions/reados-workbench.json", entry.Path);
        Assert.Contains("\"id\": \"reados-workbench\"", content);
        Assert.Contains("\"commandCount\": 2", content);
        Assert.Contains("\"approvalCount\": 1", content);
        Assert.Contains("\"transcript-1\"", content);
        Assert.Contains("\"/artifacts/report.md\"", content);
    }

    [Fact]
    public void Session_entry_round_trips_core_record()
    {
        var record = new MspSessionRecord
        {
            Id = "session-1",
            Title = "Session 1",
            Actor = "agent",
            LastCommandText = "workspace info",
            LastDecision = "Allow",
            LastExitCode = 0,
            CommandCount = 1,
            TranscriptIds = new[] { "transcript-1" },
            ArtifactPaths = new[] { "/artifacts/a.md" }
        };

        var entry = MspSessionEntry.FromRecord(record);
        var roundTrip = entry.ToRecord();

        Assert.Equal(record.Id, roundTrip.Id);
        Assert.Equal(record.Title, roundTrip.Title);
        Assert.Equal(record.CommandCount, roundTrip.CommandCount);
        Assert.Equal(record.TranscriptIds, roundTrip.TranscriptIds);
        Assert.Equal(record.ArtifactPaths, roundTrip.ArtifactPaths);
    }

    [Fact]
    public void Workspace_state_round_trips_session_records()
    {
        var workspace = new WorkspaceState();
        var session = new MspSessionEntry
        {
            Id = "session-json",
            Title = "JSON session",
            Actor = "agent",
            LastCommandText = "workspace info",
            LastDecision = "Allow",
            CommandCount = 1
        };
        session.TranscriptIds.Add("transcript-json");
        session.ArtifactPaths.Add("/artifacts/json.md");
        workspace.MspSessions.Add(session);

        var options = new JsonSerializerOptions(JsonSerializerDefaults.Web)
        {
            PreferredObjectCreationHandling = JsonObjectCreationHandling.Populate
        };
        var json = JsonSerializer.Serialize(workspace, options);
        var roundTrip = JsonSerializer.Deserialize<WorkspaceState>(json, options);

        Assert.NotNull(roundTrip);
        var restored = Assert.Single(roundTrip.MspSessions);
        Assert.Equal("session-json", restored.Id);
        Assert.Equal("workspace info", restored.LastCommandText);
        Assert.Equal("transcript-json", Assert.Single(restored.TranscriptIds));
        Assert.Equal("/artifacts/json.md", Assert.Single(restored.ArtifactPaths));
    }

    [Fact]
    public void Transcript_entry_reports_running_and_canceled_progress()
    {
        var entry = new MspTranscriptEntry
        {
            CommandText = "pdf search current keyword",
            IsRunning = true,
            ProgressMessage = "Searching current document.",
            ProgressPercent = 45
        };

        Assert.True(entry.CanCancel);
        Assert.Equal("运行 45%", entry.StatusLabel);
        Assert.Equal(45, entry.ProgressPercentValue);
        Assert.Equal("Searching current document.", entry.OutputPreview);

        entry.IsRunning = false;
        entry.WasCanceled = true;
        entry.ExitCode = 130;
        entry.ProgressMessage = "MSP command was canceled.";

        Assert.False(entry.CanCancel);
        Assert.Equal("已取消", entry.StatusLabel);
        Assert.Equal("MSP command was canceled.", entry.OutputPreview);
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
