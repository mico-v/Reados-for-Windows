using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsMspTranscriptWorkspaceServiceTests
{
    [Fact]
    public void RefreshTranscript_returns_empty_for_null_workspace()
    {
        var service = CreateService();

        var entries = service.RefreshTranscript(null);

        Assert.Empty(entries);
    }

    [Fact]
    public void RefreshTranscript_delegates_sorting_trimming_and_session_rebuild()
    {
        var workspace = new WorkspaceState();
        workspace.MspTranscript.Add(CreateEntry("old", "old-session", "old command", 1));
        workspace.MspTranscript.Add(CreateEntry("new", string.Empty, "new command", 2));
        var service = CreateService(maxTranscriptEntries: 1);

        var entries = service.RefreshTranscript(workspace);

        var entry = Assert.Single(entries);
        Assert.Equal("new", entry.Id);
        Assert.Equal(new[] { "new" }, workspace.MspTranscript.Select(item => item.Id));
        Assert.Equal("default-session", workspace.MspTranscript[0].SessionId);
        var session = Assert.Single(workspace.MspSessions);
        Assert.Equal("default-session", session.Id);
        Assert.Equal("new command", session.LastCommandText);
    }

    [Fact]
    public void PersistTranscriptEntry_returns_false_for_null_workspace()
    {
        var service = CreateService();

        var persisted = service.PersistTranscriptEntry(null, CreateEntry("entry", "session", "workspace info", 1), 0);

        Assert.False(persisted);
    }

    [Fact]
    public void PersistTranscriptEntry_delegates_to_store_and_rebuilds_session()
    {
        var workspace = new WorkspaceState();
        var service = CreateService();
        var entry = CreateEntry("entry", string.Empty, "workspace info", 1);

        var persisted = service.PersistTranscriptEntry(workspace, entry, 0);

        Assert.True(persisted);
        Assert.Equal("default-session", workspace.MspTranscript[0].SessionId);
        var session = Assert.Single(workspace.MspSessions);
        Assert.Equal("default-session", session.Id);
        Assert.Equal("workspace info", session.LastCommandText);
        Assert.Equal("entry", Assert.Single(session.TranscriptIds));
    }

    [Fact]
    public void RemoveTranscriptEntry_returns_false_for_null_or_missing_entry()
    {
        var workspace = new WorkspaceState();
        var service = CreateService();

        var nullWorkspaceRemoved = service.RemoveTranscriptEntry(null, CreateEntry("missing", "session", "missing", 1));
        var missingRemoved = service.RemoveTranscriptEntry(workspace, CreateEntry("missing", "session", "missing", 1));

        Assert.False(nullWorkspaceRemoved);
        Assert.False(missingRemoved);
    }

    [Fact]
    public void RemoveTranscriptEntry_delegates_to_store()
    {
        var workspace = new WorkspaceState();
        var entry = CreateEntry("entry", "session", "workspace info", 1);
        workspace.MspTranscript.Add(entry);
        var service = CreateService();
        service.RebuildAllSessions(workspace);

        var removed = service.RemoveTranscriptEntry(workspace, entry);

        Assert.True(removed);
        Assert.Empty(workspace.MspTranscript);
        Assert.Empty(workspace.MspSessions);
    }

    [Fact]
    public void RebuildAllSessions_returns_false_for_null_workspace()
    {
        var service = CreateService();

        var rebuilt = service.RebuildAllSessions(null);

        Assert.False(rebuilt);
    }

    [Fact]
    public void RebuildAllSessions_delegates_to_store()
    {
        var workspace = new WorkspaceState();
        workspace.MspTranscript.Add(CreateEntry("entry", "session", "workspace info", 1));
        var service = CreateService();

        var rebuilt = service.RebuildAllSessions(workspace);

        Assert.True(rebuilt);
        var session = Assert.Single(workspace.MspSessions);
        Assert.Equal("session", session.Id);
        Assert.Equal("workspace info", session.LastCommandText);
    }

    private static ReadOsMspTranscriptWorkspaceService CreateService(int maxTranscriptEntries = 5)
    {
        return new ReadOsMspTranscriptWorkspaceService(
            new ReadOsMspSessionStore("default-session", maxTranscriptEntries));
    }

    private static MspTranscriptEntry CreateEntry(
        string id,
        string sessionId,
        string commandText,
        int minute)
    {
        var startedAt = new DateTimeOffset(2026, 7, 7, 12, minute, 0, TimeSpan.Zero);
        return new MspTranscriptEntry
        {
            Id = id,
            SessionId = sessionId,
            CommandText = commandText,
            StartedAt = startedAt,
            CompletedAt = startedAt.AddSeconds(1)
        };
    }
}
