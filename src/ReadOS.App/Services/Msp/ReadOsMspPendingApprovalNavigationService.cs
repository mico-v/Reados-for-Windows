using ReadOS.App.Models;
using ReadOS.App.ViewModels;

namespace ReadOS.App.Services.Msp;

internal readonly record struct ReadOsMspPendingApprovalNavigation(
    bool ShouldOpen,
    MspTranscriptEntry? Transcript,
    InspectorTab InspectorTab,
    string StatusMessage);

internal sealed class ReadOsMspPendingApprovalNavigationService
{
    public ReadOsMspPendingApprovalNavigation Resolve(IEnumerable<MspTranscriptEntry> transcripts)
    {
        var pendingEntry = transcripts.FirstOrDefault(entry => entry.IsApprovalRequired);
        if (pendingEntry is null)
        {
            return new ReadOsMspPendingApprovalNavigation(
                false,
                null,
                InspectorTab.Policy,
                "当前没有待审批 MSP 命令。");
        }

        return new ReadOsMspPendingApprovalNavigation(
            true,
            pendingEntry,
            InspectorTab.Policy,
            $"已打开待审批 MSP 命令：{pendingEntry.CommandText}");
    }
}
