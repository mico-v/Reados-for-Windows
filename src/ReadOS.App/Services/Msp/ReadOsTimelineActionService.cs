using ReadOS.App.Models;
using ReadOS.App.ViewModels;

namespace ReadOS.App.Services.Msp;

internal readonly record struct ReadOsTimelineAction(
    bool ShouldOpenInspector,
    InspectorTab InspectorTab,
    WorkspaceArtifact? Artifact,
    MspTranscriptEntry? Transcript,
    string? StatusMessage);

internal sealed class ReadOsTimelineActionService
{
    public ReadOsTimelineAction Resolve(ThreadTimelineItem? item)
    {
        if (item is null)
        {
            return new ReadOsTimelineAction(false, InspectorTab.Evidence, null, null, null);
        }

        return item.Kind switch
        {
            TimelineItemKind.Artifact when item.Artifact is not null =>
                new ReadOsTimelineAction(
                    true,
                    InspectorTab.Artifacts,
                    item.Artifact,
                    null,
                    $"已打开产物：{item.Artifact.Path}"),
            TimelineItemKind.Evidence =>
                new ReadOsTimelineAction(
                    true,
                    InspectorTab.Evidence,
                    null,
                    null,
                    "已打开证据面板。"),
            TimelineItemKind.Error when item.MspEntry is not null =>
                new ReadOsTimelineAction(
                    true,
                    InspectorTab.Policy,
                    null,
                    item.MspEntry,
                    $"已打开 MSP 诊断：{item.MspEntry.CommandText}"),
            TimelineItemKind.Approval when item.MspEntry is not null =>
                new ReadOsTimelineAction(
                    true,
                    InspectorTab.Policy,
                    null,
                    item.MspEntry,
                    $"已打开 MSP 审批：{item.MspEntry.CommandText}"),
            TimelineItemKind.MspCommand when item.MspEntry is not null =>
                new ReadOsTimelineAction(
                    true,
                    InspectorTab.Run,
                    null,
                    item.MspEntry,
                    $"已打开 MSP 记录：{item.MspEntry.CommandText}"),
            _ => new ReadOsTimelineAction(true, InspectorTab.Evidence, null, null, null)
        };
    }
}
