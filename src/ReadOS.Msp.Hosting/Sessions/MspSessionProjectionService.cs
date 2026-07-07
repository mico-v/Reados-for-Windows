using ReadOS.Msp.Models;

namespace ReadOS.Msp.Hosting.Sessions;

public sealed class MspSessionProjectionService
{
    public const string DefaultWorkbenchSessionTitle = "ReadOS Workbench MSP Session";

    public IReadOnlyList<MspSessionRecord> ProjectSessions(
        string defaultSessionId,
        IEnumerable<MspCommandTranscriptRecord> transcripts,
        IEnumerable<MspArtifact> artifacts,
        DateTimeOffset? now = null)
    {
        var resolvedDefaultSessionId = NormalizeSessionId(defaultSessionId, "default");
        var timestamp = now ?? DateTimeOffset.Now;
        var normalizedTranscripts = transcripts
            .Select(transcript => transcript with
            {
                SessionId = NormalizeSessionId(transcript.SessionId, resolvedDefaultSessionId)
            })
            .ToArray();
        var sessionArtifacts = artifacts
            .Where(artifact => !string.IsNullOrWhiteSpace(artifact.SessionId))
            .ToArray();
        var sessionIds = normalizedTranscripts
            .Select(transcript => transcript.SessionId)
            .Concat(sessionArtifacts.Select(artifact => artifact.SessionId!))
            .Where(sessionId => !string.IsNullOrWhiteSpace(sessionId))
            .ToHashSet(StringComparer.OrdinalIgnoreCase);

        return sessionIds
            .Order(StringComparer.OrdinalIgnoreCase)
            .Select(sessionId => ProjectSession(
                resolvedDefaultSessionId,
                sessionId,
                normalizedTranscripts,
                sessionArtifacts,
                timestamp))
            .ToArray();
    }

    private static MspSessionRecord ProjectSession(
        string defaultSessionId,
        string sessionId,
        IReadOnlyList<MspCommandTranscriptRecord> transcripts,
        IReadOnlyList<MspArtifact> artifacts,
        DateTimeOffset now)
    {
        var sessionTranscripts = transcripts
            .Where(transcript => string.Equals(transcript.SessionId, sessionId, StringComparison.OrdinalIgnoreCase))
            .OrderBy(transcript => transcript.StartedAt)
            .ToArray();
        var sessionArtifacts = artifacts
            .Where(artifact => string.Equals(artifact.SessionId, sessionId, StringComparison.OrdinalIgnoreCase))
            .OrderBy(artifact => artifact.Path, StringComparer.OrdinalIgnoreCase)
            .ToArray();
        var firstTranscript = sessionTranscripts.FirstOrDefault();
        var lastTranscript = sessionTranscripts.LastOrDefault();

        return new MspSessionRecord
        {
            Id = sessionId,
            Title = string.Equals(sessionId, defaultSessionId, StringComparison.OrdinalIgnoreCase)
                ? DefaultWorkbenchSessionTitle
                : $"MSP Session {sessionId}",
            Actor = lastTranscript?.Actor ?? firstTranscript?.Actor ?? "agent",
            StartedAt = firstTranscript?.StartedAt ?? now,
            UpdatedAt = lastTranscript?.CompletedAt ?? now,
            LastCommandText = lastTranscript?.CommandText ?? string.Empty,
            LastDecision = lastTranscript?.Decision ?? "Allow",
            LastExitCode = lastTranscript?.ExitCode ?? 0,
            LastProgressMessage = lastTranscript?.ProgressMessage ?? string.Empty,
            LastDiagnosticsSummary = lastTranscript?.DiagnosticsSummary ?? string.Empty,
            LastRecoveryHint = lastTranscript?.RecoveryHint ?? string.Empty,
            CommandCount = sessionTranscripts.Length,
            RunningCount = sessionTranscripts.Count(transcript => transcript.IsRunning),
            PendingApprovalCount = sessionTranscripts.Count(transcript =>
                string.Equals(transcript.Decision, "RequireConfirmation", StringComparison.Ordinal)),
            ApprovalCount = sessionTranscripts.Count(transcript =>
                string.Equals(transcript.Decision, "RequireConfirmation", StringComparison.OrdinalIgnoreCase) ||
                string.Equals(transcript.Decision, "Deny", StringComparison.OrdinalIgnoreCase)),
            FailureCount = sessionTranscripts.Count(transcript => transcript.ExitCode != 0),
            TranscriptIds = sessionTranscripts.Select(transcript => transcript.Id).ToArray(),
            ArtifactPaths = sessionArtifacts.Select(artifact => artifact.Path).ToArray()
        };
    }

    private static string NormalizeSessionId(string? sessionId, string defaultSessionId)
    {
        return string.IsNullOrWhiteSpace(sessionId)
            ? defaultSessionId
            : sessionId;
    }
}
