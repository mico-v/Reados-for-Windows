using ReadOS.App.Models;
using ReadOS.App.Services.Msp;
using ReadOS.App.ViewModels;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsTimelineActionServiceTests
{
    [Fact]
    public void Resolve_ignores_null_items()
    {
        var service = new ReadOsTimelineActionService();

        var action = service.Resolve(null);

        Assert.False(action.ShouldOpenInspector);
        Assert.Equal(InspectorTab.Evidence, action.InspectorTab);
        Assert.Null(action.Artifact);
        Assert.Null(action.Transcript);
        Assert.Null(action.StatusMessage);
    }

    [Fact]
    public void Resolve_routes_artifacts_to_artifact_inspector()
    {
        var service = new ReadOsTimelineActionService();
        var artifact = new WorkspaceArtifact
        {
            Path = "/artifacts/report.md",
            Content = "report"
        };

        var action = service.Resolve(ThreadTimelineItem.FromArtifact(artifact));

        Assert.True(action.ShouldOpenInspector);
        Assert.Equal(InspectorTab.Artifacts, action.InspectorTab);
        Assert.Same(artifact, action.Artifact);
        Assert.Null(action.Transcript);
        Assert.Equal("已打开产物：/artifacts/report.md", action.StatusMessage);
    }

    [Fact]
    public void Resolve_routes_evidence_to_evidence_inspector()
    {
        var service = new ReadOsTimelineActionService();
        var evidence = ThreadTimelineItem.FromMessage(new ChatMessage
        {
            Role = ChatRole.System,
            Author = "Evidence",
            Content = "attached page"
        });

        var action = service.Resolve(evidence);

        Assert.True(action.ShouldOpenInspector);
        Assert.Equal(InspectorTab.Evidence, action.InspectorTab);
        Assert.Null(action.Artifact);
        Assert.Null(action.Transcript);
        Assert.Equal("已打开证据面板。", action.StatusMessage);
    }

    [Fact]
    public void Resolve_routes_failed_transcripts_to_policy_diagnostics()
    {
        var service = new ReadOsTimelineActionService();
        var failed = new MspTranscriptEntry
        {
            CommandText = "pdf inspect missing",
            Decision = "Allow",
            ExitCode = 1
        };

        var action = service.Resolve(ThreadTimelineItem.FromMsp(failed));

        Assert.True(action.ShouldOpenInspector);
        Assert.Equal(InspectorTab.Policy, action.InspectorTab);
        Assert.Same(failed, action.Transcript);
        Assert.Equal("已打开 MSP 诊断：pdf inspect missing", action.StatusMessage);
    }

    [Fact]
    public void Resolve_routes_approvals_to_policy_review()
    {
        var service = new ReadOsTimelineActionService();
        var approval = new MspTranscriptEntry
        {
            CommandText = "artifact write /artifacts/pending.md pending",
            Decision = "RequireConfirmation",
            ExitCode = 126
        };

        var action = service.Resolve(ThreadTimelineItem.FromMsp(approval));

        Assert.True(action.ShouldOpenInspector);
        Assert.Equal(InspectorTab.Policy, action.InspectorTab);
        Assert.Same(approval, action.Transcript);
        Assert.Equal("已打开 MSP 审批：artifact write /artifacts/pending.md pending", action.StatusMessage);
    }

    [Fact]
    public void Resolve_routes_msp_commands_to_run_inspector()
    {
        var service = new ReadOsTimelineActionService();
        var entry = new MspTranscriptEntry
        {
            CommandText = "workspace info",
            Decision = "Allow",
            ExitCode = 0
        };

        var action = service.Resolve(ThreadTimelineItem.FromMsp(entry));

        Assert.True(action.ShouldOpenInspector);
        Assert.Equal(InspectorTab.Run, action.InspectorTab);
        Assert.Same(entry, action.Transcript);
        Assert.Equal("已打开 MSP 记录：workspace info", action.StatusMessage);
    }

    [Fact]
    public void Resolve_routes_plain_messages_to_evidence_without_status_change()
    {
        var service = new ReadOsTimelineActionService();
        var message = ThreadTimelineItem.FromMessage(new ChatMessage
        {
            Role = ChatRole.User,
            Author = "user",
            Content = "question"
        });

        var action = service.Resolve(message);

        Assert.True(action.ShouldOpenInspector);
        Assert.Equal(InspectorTab.Evidence, action.InspectorTab);
        Assert.Null(action.Artifact);
        Assert.Null(action.Transcript);
        Assert.Null(action.StatusMessage);
    }
}
