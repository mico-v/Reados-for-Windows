using ReadOS.App.Models;
using ReadOS.App.Services.Msp;
using ReadOS.App.ViewModels;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsMspPendingApprovalNavigationServiceTests
{
    [Fact]
    public void Resolve_reports_no_pending_approval_when_transcript_is_empty()
    {
        var service = new ReadOsMspPendingApprovalNavigationService();

        var navigation = service.Resolve(Array.Empty<MspTranscriptEntry>());

        Assert.False(navigation.ShouldOpen);
        Assert.Null(navigation.Transcript);
        Assert.Equal(InspectorTab.Policy, navigation.InspectorTab);
        Assert.Equal("当前没有待审批 MSP 命令。", navigation.StatusMessage);
    }

    [Fact]
    public void Resolve_ignores_non_approval_entries()
    {
        var service = new ReadOsMspPendingApprovalNavigationService();
        var transcript = new[]
        {
            new MspTranscriptEntry
            {
                Id = "completed",
                CommandText = "workspace info",
                Decision = "Allow"
            },
            new MspTranscriptEntry
            {
                Id = "failed",
                CommandText = "pdf inspect missing",
                Decision = "Allow",
                ExitCode = 1
            }
        };

        var navigation = service.Resolve(transcript);

        Assert.False(navigation.ShouldOpen);
        Assert.Null(navigation.Transcript);
        Assert.Equal("当前没有待审批 MSP 命令。", navigation.StatusMessage);
    }

    [Fact]
    public void Resolve_selects_first_pending_approval_in_visible_order()
    {
        var service = new ReadOsMspPendingApprovalNavigationService();
        var firstApproval = CreateApprovalEntry("first", "artifact write /artifacts/first.md first");
        var secondApproval = CreateApprovalEntry("second", "artifact write /artifacts/second.md second");
        var transcript = new[]
        {
            new MspTranscriptEntry
            {
                Id = "completed",
                CommandText = "workspace info",
                Decision = "Allow"
            },
            firstApproval,
            secondApproval
        };

        var navigation = service.Resolve(transcript);

        Assert.True(navigation.ShouldOpen);
        Assert.Same(firstApproval, navigation.Transcript);
    }

    [Fact]
    public void Resolve_routes_pending_approval_to_policy_inspector_with_status()
    {
        var service = new ReadOsMspPendingApprovalNavigationService();
        var approval = CreateApprovalEntry("approval", "artifact write /artifacts/pending.md pending");

        var navigation = service.Resolve(new[] { approval });

        Assert.True(navigation.ShouldOpen);
        Assert.Same(approval, navigation.Transcript);
        Assert.Equal(InspectorTab.Policy, navigation.InspectorTab);
        Assert.Equal(
            "已打开待审批 MSP 命令：artifact write /artifacts/pending.md pending",
            navigation.StatusMessage);
    }

    private static MspTranscriptEntry CreateApprovalEntry(string id, string commandText)
    {
        return new MspTranscriptEntry
        {
            Id = id,
            CommandText = commandText,
            Decision = "RequireConfirmation"
        };
    }
}
