using ReadOS.App.Models;
using ReadOS.Msp.Hosting.Sessions;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsMspTranscriptWorkspaceService
{
    private readonly ReadOsMspSessionStore sessionStore;

    public ReadOsMspTranscriptWorkspaceService(string defaultSessionId, int maxTranscriptEntries)
        : this(new ReadOsMspSessionStore(defaultSessionId, maxTranscriptEntries))
    {
    }

    public ReadOsMspTranscriptWorkspaceService(ReadOsMspSessionStore sessionStore)
    {
        this.sessionStore = sessionStore;
    }

    public IReadOnlyList<MspTranscriptEntry> RefreshTranscript(WorkspaceState? workspace)
    {
        var projectionStore = CreateProjectionStore(workspace);
        return projectionStore is null
            ? Array.Empty<MspTranscriptEntry>()
            : projectionStore
                .RefreshTranscripts()
                .Select(MspTranscriptEntry.FromRecord)
                .ToArray();
    }

    public bool PersistTranscriptEntry(
        WorkspaceState? workspace,
        MspTranscriptEntry entry,
        int index)
    {
        var projectionStore = CreateProjectionStore(workspace);
        if (projectionStore is null)
        {
            return false;
        }

        projectionStore.PersistTranscript(entry.ToRecord(), index);
        return true;
    }

    public bool RemoveTranscriptEntry(WorkspaceState? workspace, MspTranscriptEntry entry)
    {
        return CreateProjectionStore(workspace)?.RemoveTranscript(entry.Id) ?? false;
    }

    public bool RebuildAllSessions(WorkspaceState? workspace)
    {
        var projectionStore = CreateProjectionStore(workspace);
        if (projectionStore is null)
        {
            return false;
        }

        projectionStore.RebuildAllSessions();
        return true;
    }

    private IMspSessionProjectionStore? CreateProjectionStore(WorkspaceState? workspace)
    {
        return workspace is null
            ? null
            : new ReadOsWorkspaceMspSessionProjectionStore(workspace, sessionStore);
    }
}
