using ReadOS.App.Models;
using ReadOS.Msp.Hosting.Sessions;
using ReadOS.Msp.Models;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsMspSessionStore
{
    private readonly string defaultSessionId;
    private readonly int maxTranscriptEntries;
    private readonly MspSessionProjectionService sessionProjectionService;

    public ReadOsMspSessionStore(string defaultSessionId, int maxTranscriptEntries)
        : this(defaultSessionId, maxTranscriptEntries, new MspSessionProjectionService())
    {
    }

    public ReadOsMspSessionStore(
        string defaultSessionId,
        int maxTranscriptEntries,
        MspSessionProjectionService sessionProjectionService)
    {
        this.defaultSessionId = string.IsNullOrWhiteSpace(defaultSessionId)
            ? ReadOsMspHost.DefaultSessionId
            : defaultSessionId;
        this.maxTranscriptEntries = Math.Max(1, maxTranscriptEntries);
        this.sessionProjectionService = sessionProjectionService;
    }

    public string DefaultSessionId => defaultSessionId;

    public int MaxTranscriptEntries => maxTranscriptEntries;

    public IReadOnlyList<MspTranscriptEntry> RefreshTranscript(WorkspaceState workspace)
    {
        var entries = workspace.MspTranscript
            .OrderByDescending(entry => entry.CompletedAt)
            .Take(maxTranscriptEntries)
            .ToArray();

        workspace.MspTranscript.Clear();
        foreach (var entry in entries)
        {
            entry.IsRunning = false;
            entry.SessionId = NormalizeSessionId(entry.SessionId);
            workspace.MspTranscript.Add(entry);
        }

        RebuildAllSessions(workspace);
        return entries;
    }

    public void PersistTranscriptEntry(WorkspaceState workspace, MspTranscriptEntry entry, int index)
    {
        entry.SessionId = NormalizeSessionId(entry.SessionId);
        var affectedSessionIds = new HashSet<string>(StringComparer.OrdinalIgnoreCase)
        {
            entry.SessionId
        };

        var existing = workspace.MspTranscript.FirstOrDefault(item => item.Id == entry.Id);
        if (existing is not null)
        {
            affectedSessionIds.Add(NormalizeSessionId(existing.SessionId));
            workspace.MspTranscript.Remove(existing);
        }

        workspace.MspTranscript.Insert(Math.Clamp(index, 0, workspace.MspTranscript.Count), entry);
        while (workspace.MspTranscript.Count > maxTranscriptEntries)
        {
            var removed = workspace.MspTranscript[^1];
            affectedSessionIds.Add(NormalizeSessionId(removed.SessionId));
            workspace.MspTranscript.RemoveAt(workspace.MspTranscript.Count - 1);
        }

        foreach (var sessionId in affectedSessionIds)
        {
            RebuildSession(workspace, sessionId);
        }
    }

    public bool RemoveTranscriptEntry(WorkspaceState workspace, MspTranscriptEntry entry)
    {
        var existing = workspace.MspTranscript.FirstOrDefault(item => item.Id == entry.Id);
        if (existing is null)
        {
            return false;
        }

        var sessionId = NormalizeSessionId(existing.SessionId);
        workspace.MspTranscript.Remove(existing);
        RebuildSession(workspace, sessionId);
        return true;
    }

    public void RebuildAllSessions(WorkspaceState workspace)
    {
        EnsureTranscriptSessionIds(workspace);
        var sessions = ProjectSessions(workspace);
        var sessionIds = sessions
            .Select(session => session.Id)
            .ToHashSet(StringComparer.OrdinalIgnoreCase);
        var staleSessions = workspace.MspSessions
            .Where(session => !sessionIds.Contains(session.Id))
            .ToArray();
        foreach (var staleSession in staleSessions)
        {
            workspace.MspSessions.Remove(staleSession);
        }

        foreach (var session in sessions)
        {
            ApplySessionRecord(workspace, session);
        }
    }

    public void RebuildSession(WorkspaceState workspace, string? sessionId)
    {
        EnsureTranscriptSessionIds(workspace);
        var resolvedSessionId = NormalizeSessionId(sessionId);
        var record = ProjectSessions(workspace)
            .FirstOrDefault(session => string.Equals(session.Id, resolvedSessionId, StringComparison.OrdinalIgnoreCase));
        var session = workspace.MspSessions.FirstOrDefault(item =>
            string.Equals(item.Id, resolvedSessionId, StringComparison.OrdinalIgnoreCase));
        if (record is null)
        {
            if (session is not null)
            {
                workspace.MspSessions.Remove(session);
            }

            return;
        }

        ApplySessionRecord(workspace, record);
    }

    private void EnsureTranscriptSessionIds(WorkspaceState workspace)
    {
        foreach (var entry in workspace.MspTranscript.Where(entry => string.IsNullOrWhiteSpace(entry.SessionId)))
        {
            entry.SessionId = defaultSessionId;
        }
    }

    private string NormalizeSessionId(string? sessionId)
    {
        return string.IsNullOrWhiteSpace(sessionId)
            ? defaultSessionId
            : sessionId;
    }

    private IReadOnlyList<MspSessionRecord> ProjectSessions(WorkspaceState workspace)
    {
        return sessionProjectionService.ProjectSessions(
            defaultSessionId,
            workspace.MspTranscript.Select(entry => entry.ToRecord()),
            workspace.Artifacts.Select(ToMspArtifact));
    }

    private void ApplySessionRecord(WorkspaceState workspace, MspSessionRecord record)
    {
        var session = workspace.MspSessions.FirstOrDefault(item =>
            string.Equals(item.Id, record.Id, StringComparison.OrdinalIgnoreCase));
        if (session is null)
        {
            session = MspSessionEntry.FromRecord(record);
            workspace.MspSessions.Add(session);
            session.NotifyDerivedStateChanged();
            return;
        }

        if (record.CommandCount > 0)
        {
            session.Actor = record.Actor;
            session.StartedAt = record.StartedAt;
        }

        session.UpdatedAt = record.UpdatedAt;
        session.LastCommandText = record.LastCommandText;
        session.LastDecision = record.LastDecision;
        session.LastExitCode = record.LastExitCode;
        session.LastProgressMessage = record.LastProgressMessage;
        session.LastDiagnosticsSummary = record.LastDiagnosticsSummary;
        session.LastRecoveryHint = record.LastRecoveryHint;
        session.CommandCount = record.CommandCount;
        session.RunningCount = record.RunningCount;
        session.PendingApprovalCount = record.PendingApprovalCount;
        session.ApprovalCount = record.ApprovalCount;
        session.FailureCount = record.FailureCount;
        ReplaceValues(session.TranscriptIds, record.TranscriptIds);
        ReplaceValues(session.ArtifactPaths, record.ArtifactPaths);
        session.NotifyDerivedStateChanged();
    }

    private static MspArtifact ToMspArtifact(WorkspaceArtifact artifact)
    {
        return new MspArtifact
        {
            Path = artifact.Path,
            MediaType = artifact.MediaType,
            SizeBytes = artifact.SizeBytes,
            Description = artifact.Description,
            SourceCommand = artifact.SourceCommand,
            Actor = artifact.Actor,
            SessionId = artifact.SessionId,
            SourcePaths = artifact.SourcePaths.ToArray(),
            SourceDocuments = artifact.SourceDocuments.ToArray(),
            SourcePages = artifact.SourcePages.ToArray(),
            CreatedAt = artifact.CreatedAt,
            UpdatedAt = artifact.UpdatedAt,
            Preview = artifact.Preview
        };
    }

    private static void ReplaceValues(ICollection<string> target, IEnumerable<string> values)
    {
        target.Clear();
        foreach (var value in values.Where(value => !string.IsNullOrWhiteSpace(value)))
        {
            target.Add(value);
        }
    }
}
