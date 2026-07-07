using ReadOS.App.Models;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsTimelineService
{
    public IReadOnlyList<ThreadTimelineItem> BuildTimeline(
        IEnumerable<ChatMessage> messages,
        IEnumerable<MspTranscriptEntry> transcripts,
        IEnumerable<WorkspaceArtifact> artifacts)
    {
        return messages.Select(ThreadTimelineItem.FromMessage)
            .Concat(transcripts.Select(ThreadTimelineItem.FromMsp))
            .Concat(artifacts.Select(ThreadTimelineItem.FromArtifact))
            .OrderBy(item => item.CreatedAt)
            .ThenBy(item => item.Kind)
            .ToArray();
    }
}
