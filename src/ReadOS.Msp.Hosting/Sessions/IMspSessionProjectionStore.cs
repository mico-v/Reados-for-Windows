using ReadOS.Msp.Models;

namespace ReadOS.Msp.Hosting.Sessions;

public interface IMspSessionProjectionStore
{
    string DefaultSessionId { get; }

    int MaxTranscriptEntries { get; }

    IReadOnlyList<MspCommandTranscriptRecord> Transcripts { get; }

    IReadOnlyList<MspSessionRecord> Sessions { get; }

    IReadOnlyList<MspCommandTranscriptRecord> RefreshTranscripts();

    void PersistTranscript(MspCommandTranscriptRecord transcript, int index);

    bool RemoveTranscript(string transcriptId);

    void RebuildSession(string? sessionId);

    void RebuildAllSessions();
}
