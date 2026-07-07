using ReadOS.App.Models;
using ReadOS.App.Services.Msp;
using ReadOS.Msp.Models;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsWorkspaceMspSessionProjectionStoreTests
{
    [Fact]
    public void RefreshTranscripts_projects_records_after_sorting_trimming_and_session_rebuild()
    {
        var workspace = new WorkspaceState();
        workspace.MspTranscript.Add(CreateEntry("old", "old-session", "old command", 1));
        workspace.MspTranscript.Add(CreateEntry("new", string.Empty, "new command", 2, isRunning: true));
        var store = CreateStore(workspace, maxTranscriptEntries: 1);

        var records = store.RefreshTranscripts();

        var record = Assert.Single(records);
        Assert.Equal("new", record.Id);
        Assert.Equal("default-session", record.SessionId);
        Assert.False(record.IsRunning);
        Assert.Equal(new[] { "new" }, workspace.MspTranscript.Select(entry => entry.Id));
        var session = Assert.Single(store.Sessions);
        Assert.Equal("default-session", session.Id);
        Assert.Equal("new command", session.LastCommandText);
    }

    [Fact]
    public void PersistTranscript_replaces_existing_record_and_rebuilds_affected_sessions()
    {
        var workspace = new WorkspaceState();
        workspace.MspTranscript.Add(CreateEntry("same", "old-session", "old command", 1));
        var store = CreateStore(workspace);
        store.RebuildAllSessions();

        var replacement = CreateRecord("same", "new-session", "new command", 2);
        store.PersistTranscript(replacement, 0);

        var transcript = Assert.Single(store.Transcripts);
        Assert.Equal("new-session", transcript.SessionId);
        Assert.Equal("new command", transcript.CommandText);
        Assert.DoesNotContain(store.Sessions, session => session.Id == "old-session");
        var session = Assert.Single(store.Sessions);
        Assert.Equal("new-session", session.Id);
        Assert.Equal("same", Assert.Single(session.TranscriptIds));
    }

    [Fact]
    public void RemoveTranscript_removes_by_id_and_preserves_artifact_only_sessions()
    {
        var workspace = new WorkspaceState();
        workspace.MspTranscript.Add(CreateEntry("entry", "artifact-session", "workspace info", 1));
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Path = "/artifacts/report.md",
            SessionId = "artifact-session"
        });
        var store = CreateStore(workspace);
        store.RebuildAllSessions();

        var removed = store.RemoveTranscript("entry");

        Assert.True(removed);
        Assert.Empty(store.Transcripts);
        var session = Assert.Single(store.Sessions);
        Assert.Equal("artifact-session", session.Id);
        Assert.Equal("/artifacts/report.md", Assert.Single(session.ArtifactPaths));
    }

    [Fact]
    public void RemoveTranscript_returns_false_for_missing_record()
    {
        var workspace = new WorkspaceState();
        var store = CreateStore(workspace);

        Assert.False(store.RemoveTranscript("missing"));
    }

    private static ReadOsWorkspaceMspSessionProjectionStore CreateStore(
        WorkspaceState workspace,
        int maxTranscriptEntries = 5)
    {
        return new ReadOsWorkspaceMspSessionProjectionStore(
            workspace,
            new ReadOsMspSessionStore("default-session", maxTranscriptEntries));
    }

    private static MspTranscriptEntry CreateEntry(
        string id,
        string sessionId,
        string commandText,
        int minute,
        bool isRunning = false)
    {
        var startedAt = new DateTimeOffset(2026, 7, 7, 12, minute, 0, TimeSpan.Zero);
        return new MspTranscriptEntry
        {
            Id = id,
            SessionId = sessionId,
            CommandText = commandText,
            StartedAt = startedAt,
            CompletedAt = startedAt.AddSeconds(1),
            IsRunning = isRunning
        };
    }

    private static MspCommandTranscriptRecord CreateRecord(
        string id,
        string sessionId,
        string commandText,
        int minute)
    {
        var startedAt = new DateTimeOffset(2026, 7, 7, 12, minute, 0, TimeSpan.Zero);
        return new MspCommandTranscriptRecord
        {
            Id = id,
            SessionId = sessionId,
            CommandText = commandText,
            StartedAt = startedAt,
            CompletedAt = startedAt.AddSeconds(1)
        };
    }
}
