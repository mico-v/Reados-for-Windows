using ReadOS.App.Models;
using ReadOS.App.ViewModels;

namespace ReadOS.App.Services.Msp;

internal readonly record struct ReadOsMspSessionSelection(
    MspTranscriptEntry? Transcript,
    InspectorTab InspectorTab);

internal sealed class ReadOsMspSessionViewService
{
    public IReadOnlyList<MspSessionEntry> GetVisibleSessions(
        IEnumerable<MspSessionEntry> sessions,
        string searchQuery)
    {
        var query = searchQuery.Trim();
        return sessions
            .Where(item => string.IsNullOrWhiteSpace(query) || Matches(item, query))
            .OrderByDescending(item => item.UpdatedAt)
            .ThenBy(item => item.Title, StringComparer.OrdinalIgnoreCase)
            .ToArray();
    }

    public bool ShouldClearSelection(
        MspSessionEntry? selectedSession,
        IEnumerable<MspSessionEntry> visibleSessions)
    {
        return selectedSession is not null &&
            visibleSessions.All(item => !string.Equals(item.Id, selectedSession.Id, StringComparison.OrdinalIgnoreCase));
    }

    public ReadOsMspSessionSelection SelectSessionTranscript(
        MspSessionEntry? session,
        IEnumerable<MspTranscriptEntry> transcripts)
    {
        if (session is null)
        {
            return new ReadOsMspSessionSelection(null, InspectorTab.Run);
        }

        var transcriptIds = session.TranscriptIds.ToHashSet(StringComparer.OrdinalIgnoreCase);
        var transcript = transcripts.FirstOrDefault(entry => transcriptIds.Contains(entry.Id));
        var inspectorTab = transcript is not null &&
            (transcript.IsApprovalRequired || (!transcript.IsRunning && !transcript.Succeeded))
            ? InspectorTab.Policy
            : InspectorTab.Run;
        return new ReadOsMspSessionSelection(transcript, inspectorTab);
    }

    private static bool Matches(MspSessionEntry session, string query)
    {
        return session.Id.Contains(query, StringComparison.OrdinalIgnoreCase) ||
            session.Title.Contains(query, StringComparison.OrdinalIgnoreCase) ||
            session.LastCommandText.Contains(query, StringComparison.OrdinalIgnoreCase) ||
            session.LastDiagnosticsSummary.Contains(query, StringComparison.OrdinalIgnoreCase);
    }
}
