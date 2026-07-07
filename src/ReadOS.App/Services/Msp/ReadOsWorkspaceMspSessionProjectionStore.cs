using ReadOS.App.Models;
using ReadOS.Msp.Hosting.Sessions;
using ReadOS.Msp.Models;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsWorkspaceMspSessionProjectionStore : IMspSessionProjectionStore
{
    private readonly WorkspaceState workspace;
    private readonly ReadOsMspSessionStore sessionStore;

    public ReadOsWorkspaceMspSessionProjectionStore(
        WorkspaceState workspace,
        ReadOsMspSessionStore sessionStore)
    {
        this.workspace = workspace;
        this.sessionStore = sessionStore;
    }

    public string DefaultSessionId => sessionStore.DefaultSessionId;

    public int MaxTranscriptEntries => sessionStore.MaxTranscriptEntries;

    public IReadOnlyList<MspCommandTranscriptRecord> Transcripts =>
        workspace.MspTranscript.Select(entry => entry.ToRecord()).ToArray();

    public IReadOnlyList<MspSessionRecord> Sessions =>
        workspace.MspSessions.Select(session => session.ToRecord()).ToArray();

    public IReadOnlyList<MspCommandTranscriptRecord> RefreshTranscripts()
    {
        return sessionStore
            .RefreshTranscript(workspace)
            .Select(entry => entry.ToRecord())
            .ToArray();
    }

    public void PersistTranscript(MspCommandTranscriptRecord transcript, int index)
    {
        sessionStore.PersistTranscriptEntry(workspace, MspTranscriptEntry.FromRecord(transcript), index);
    }

    public bool RemoveTranscript(string transcriptId)
    {
        var entry = workspace.MspTranscript.FirstOrDefault(item =>
            string.Equals(item.Id, transcriptId, StringComparison.Ordinal));
        return entry is not null && sessionStore.RemoveTranscriptEntry(workspace, entry);
    }

    public void RebuildSession(string? sessionId)
    {
        sessionStore.RebuildSession(workspace, sessionId);
    }

    public void RebuildAllSessions()
    {
        sessionStore.RebuildAllSessions(workspace);
    }
}
