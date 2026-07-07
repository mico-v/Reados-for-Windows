using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsMspSessionStoreTests
{
    [Fact]
    public void RefreshTranscript_sorts_trims_normalizes_and_rebuilds_sessions()
    {
        var workspace = new WorkspaceState();
        workspace.MspTranscript.Add(CreateEntry("old", string.Empty, "old command", 8));
        workspace.MspTranscript.Add(CreateEntry("new", "new-session", "new command", 10));
        workspace.MspTranscript.Add(CreateEntry("middle", string.Empty, "middle command", 9, isRunning: true));
        workspace.MspSessions.Add(new MspSessionEntry
        {
            Id = "stale-session",
            Title = "Stale"
        });
        var store = new ReadOsMspSessionStore("default-session", 2);

        var entries = store.RefreshTranscript(workspace);

        Assert.Equal(new[] { "new", "middle" }, entries.Select(entry => entry.Id));
        Assert.Equal(new[] { "new", "middle" }, workspace.MspTranscript.Select(entry => entry.Id));
        Assert.Equal("default-session", workspace.MspTranscript.Single(entry => entry.Id == "middle").SessionId);
        Assert.False(workspace.MspTranscript.Single(entry => entry.Id == "middle").IsRunning);
        Assert.DoesNotContain(workspace.MspSessions, session => session.Id == "stale-session");
        Assert.Equal(
            new[] { "default-session", "new-session" },
            workspace.MspSessions.Select(session => session.Id).Order(StringComparer.OrdinalIgnoreCase));
    }

    [Fact]
    public void PersistTranscriptEntry_replaces_existing_entry_and_rebuilds_affected_sessions()
    {
        var workspace = new WorkspaceState();
        workspace.MspTranscript.Add(CreateEntry("same", "old-session", "old command", 8));
        var store = new ReadOsMspSessionStore("default-session", 3);
        store.RebuildAllSessions(workspace);

        var replacement = CreateEntry("same", "new-session", "new command", 9);
        store.PersistTranscriptEntry(workspace, replacement, 0);

        var transcript = Assert.Single(workspace.MspTranscript);
        Assert.Equal("new command", transcript.CommandText);
        Assert.Equal("new-session", transcript.SessionId);
        Assert.DoesNotContain(workspace.MspSessions, session => session.Id == "old-session");
        var session = Assert.Single(workspace.MspSessions);
        Assert.Equal("new-session", session.Id);
        Assert.Equal("new command", session.LastCommandText);
        Assert.Equal("same", Assert.Single(session.TranscriptIds));
    }

    [Fact]
    public void PersistTranscriptEntry_trims_oldest_entries_and_removes_trimmed_sessions()
    {
        var workspace = new WorkspaceState();
        workspace.MspTranscript.Add(CreateEntry("keep", "keep-session", "keep command", 9));
        workspace.MspTranscript.Add(CreateEntry("trim", "trim-session", "trim command", 8));
        var store = new ReadOsMspSessionStore("default-session", 2);
        store.RebuildAllSessions(workspace);

        store.PersistTranscriptEntry(workspace, CreateEntry("new", "new-session", "new command", 10), 0);

        Assert.Equal(new[] { "new", "keep" }, workspace.MspTranscript.Select(entry => entry.Id));
        Assert.DoesNotContain(workspace.MspSessions, session => session.Id == "trim-session");
        Assert.Contains(workspace.MspSessions, session => session.Id == "new-session");
        Assert.Contains(workspace.MspSessions, session => session.Id == "keep-session");
    }

    [Fact]
    public void RemoveTranscriptEntry_removes_empty_session_but_keeps_artifact_only_session()
    {
        var workspace = new WorkspaceState();
        var entry = CreateEntry("transcript", "artifact-session", "artifact command", 8);
        workspace.MspTranscript.Add(entry);
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Path = "/artifacts/report.md",
            Content = "report",
            SessionId = "artifact-session"
        });
        var store = new ReadOsMspSessionStore("default-session", 5);
        store.RebuildAllSessions(workspace);

        var removed = store.RemoveTranscriptEntry(workspace, entry);

        Assert.True(removed);
        Assert.Empty(workspace.MspTranscript);
        var session = Assert.Single(workspace.MspSessions);
        Assert.Equal("artifact-session", session.Id);
        Assert.Equal(0, session.CommandCount);
        Assert.Equal(string.Empty, session.LastCommandText);
        Assert.Equal("/artifacts/report.md", Assert.Single(session.ArtifactPaths));
    }

    [Fact]
    public void RebuildAllSessions_projects_running_pending_failure_and_artifact_state()
    {
        var workspace = new WorkspaceState();
        workspace.MspTranscript.Add(CreateEntry("success", "session", "workspace info", 8));
        workspace.MspTranscript.Add(CreateEntry(
            "pending",
            "session",
            "artifact write /artifacts/pending.md",
            9,
            decision: "RequireConfirmation",
            exitCode: 126));
        workspace.MspTranscript.Add(CreateEntry(
            "running",
            "session",
            "pdf text current 1 3",
            10,
            decision: "Running",
            isRunning: true));
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Path = "/artifacts/a.md",
            Content = "a",
            SessionId = "session"
        });
        workspace.Artifacts.Add(new WorkspaceArtifact
        {
            Path = "/artifacts/b.md",
            Content = "b",
            SessionId = "session"
        });
        var store = new ReadOsMspSessionStore("default-session", 5);

        store.RebuildAllSessions(workspace);

        var session = Assert.Single(workspace.MspSessions);
        Assert.Equal("session", session.Id);
        Assert.Equal(3, session.CommandCount);
        Assert.Equal(1, session.RunningCount);
        Assert.Equal(1, session.PendingApprovalCount);
        Assert.Equal(1, session.ApprovalCount);
        Assert.Equal(1, session.FailureCount);
        Assert.Equal("pdf text current 1 3", session.LastCommandText);
        Assert.Equal(new[] { "success", "pending", "running" }, session.TranscriptIds);
        Assert.Equal(new[] { "/artifacts/a.md", "/artifacts/b.md" }, session.ArtifactPaths);
    }

    private static MspTranscriptEntry CreateEntry(
        string id,
        string sessionId,
        string commandText,
        int minute,
        string decision = "Allow",
        int exitCode = 0,
        bool isRunning = false)
    {
        var startedAt = new DateTimeOffset(2026, 7, 7, 10, minute, 0, TimeSpan.Zero);
        return new MspTranscriptEntry
        {
            Id = id,
            SessionId = sessionId,
            Actor = "test-agent",
            CommandText = commandText,
            StartedAt = startedAt,
            CompletedAt = startedAt.AddSeconds(1),
            Decision = decision,
            ExitCode = exitCode,
            IsRunning = isRunning
        };
    }
}
