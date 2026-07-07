using ReadOS.Msp.Models;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Hosting.Sessions;

public sealed class MspSessionWorkspaceProjectionService
{
    public const string JsonMediaType = "application/json";

    public IReadOnlyList<MspWorkspaceEntry> ListSessionEntries(IEnumerable<MspSessionRecord> sessions)
    {
        return sessions
            .Select(session => File(
                GetSessionPath(session.Id),
                $"{session.Id}.json",
                EstimateSessionSize(session)))
            .ToArray();
    }

    public IReadOnlyList<MspWorkspaceEntry> ListTranscriptEntries(IEnumerable<MspCommandTranscriptRecord> transcripts)
    {
        return transcripts
            .Select(transcript => File(
                GetTranscriptPath(transcript.Id),
                $"{transcript.Id}.json",
                EstimateTranscriptSize(transcript)))
            .ToArray();
    }

    public MspSessionRecord? FindSession(IEnumerable<MspSessionRecord> sessions, string path)
    {
        return TryGetRecordId(path, "/sessions/", out var sessionId)
            ? sessions.FirstOrDefault(session =>
                string.Equals(session.Id, sessionId, StringComparison.OrdinalIgnoreCase))
            : null;
    }

    public MspCommandTranscriptRecord? FindTranscript(
        IEnumerable<MspCommandTranscriptRecord> transcripts,
        string path)
    {
        return TryGetRecordId(path, "/transcripts/", out var transcriptId)
            ? transcripts.FirstOrDefault(transcript =>
                string.Equals(transcript.Id, transcriptId, StringComparison.OrdinalIgnoreCase))
            : null;
    }

    public string GetSessionPath(string sessionId)
    {
        return $"/sessions/{sessionId}.json";
    }

    public string GetTranscriptPath(string transcriptId)
    {
        return $"/transcripts/{transcriptId}.json";
    }

    public long EstimateSessionSize(MspSessionRecord session)
    {
        return session.Id.Length +
            session.Title.Length +
            session.LastCommandText.Length +
            session.LastProgressMessage.Length +
            session.LastDiagnosticsSummary.Length +
            session.LastRecoveryHint.Length +
            session.TranscriptIds.Sum(id => id.Length) +
            session.ArtifactPaths.Sum(path => path.Length) +
            512;
    }

    public long EstimateTranscriptSize(MspCommandTranscriptRecord transcript)
    {
        return transcript.CommandText.Length +
            transcript.Stdout.Length +
            transcript.Stderr.Length +
            transcript.ArtifactsSummary.Length +
            transcript.ProgressMessage.Length +
            transcript.DiagnosticsSummary.Length +
            transcript.RecoveryHint.Length +
            256;
    }

    private static MspWorkspaceEntry File(string path, string name, long? sizeBytes)
    {
        return new MspWorkspaceEntry
        {
            Path = path,
            Name = name,
            IsDirectory = false,
            SizeBytes = sizeBytes,
            MediaType = JsonMediaType
        };
    }

    private static bool TryGetRecordId(string path, string prefix, out string id)
    {
        id = string.Empty;
        if (!path.StartsWith(prefix, StringComparison.OrdinalIgnoreCase) ||
            !path.EndsWith(".json", StringComparison.OrdinalIgnoreCase))
        {
            return false;
        }

        id = Path.GetFileNameWithoutExtension(path);
        return !string.IsNullOrWhiteSpace(id);
    }
}
