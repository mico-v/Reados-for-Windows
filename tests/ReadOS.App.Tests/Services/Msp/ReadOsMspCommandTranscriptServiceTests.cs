using ReadOS.App.Services.Msp;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsMspCommandTranscriptServiceTests
{
    [Fact]
    public void CreateRunningEntry_initializes_runtime_transcript_state()
    {
        var startedAt = new DateTimeOffset(2026, 7, 7, 10, 0, 0, TimeSpan.Zero);
        var service = new ReadOsMspCommandTranscriptService("default-session");

        var entry = service.CreateRunningEntry("workspace info", "test-agent", startedAt);

        Assert.Equal("test-agent", entry.Actor);
        Assert.Equal("default-session", entry.SessionId);
        Assert.Equal("workspace info", entry.CommandText);
        Assert.Equal(startedAt, entry.StartedAt);
        Assert.Equal(startedAt, entry.CompletedAt);
        Assert.Equal("Running", entry.Decision);
        Assert.True(entry.IsRunning);
        Assert.Equal("MSP 命令已开始。", entry.ProgressMessage);
        Assert.Equal(0, entry.ProgressPercent);
    }

    [Fact]
    public void ApplyEvent_projects_policy_decision_and_policy_routing_state()
    {
        var service = new ReadOsMspCommandTranscriptService("default-session");
        var entry = service.CreateRunningEntry("artifact write /artifacts/a.md a", "test-agent", DateTimeOffset.UtcNow);
        var commandEvent = new MspCommandEvent
        {
            Kind = MspCommandEventKind.PolicyDecision,
            SessionId = "session-1",
            Message = "Approval required",
            Decision = MspPolicyDecision.RequireConfirmation,
            Effects = MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact,
            Preview = MspCommandPreview.Create(
                "Write artifact",
                new[] { "/artifacts/a.md" },
                new[] { "contentLength: 1" })
        };

        var projection = service.ApplyEvent(entry, commandEvent);

        Assert.Equal("session-1", entry.SessionId);
        Assert.Equal("Approval required", entry.ProgressMessage);
        Assert.Equal("RequireConfirmation", entry.Decision);
        Assert.Equal("WriteWorkspace, CreateArtifact", entry.Effects);
        Assert.Contains("Write artifact", entry.PolicyPreview);
        Assert.Contains("/artifacts/a.md", entry.PolicyPreview);
        Assert.Equal(0, entry.ProgressPercent);
        Assert.Equal("Approval required", projection.StatusMessage);
        Assert.True(projection.ShouldRefreshTimeline);
        Assert.True(projection.ShouldOpenPolicy);
    }

    [Fact]
    public void ApplyEvent_projects_progress_completed_and_canceled_state()
    {
        var service = new ReadOsMspCommandTranscriptService("default-session");
        var entry = service.CreateRunningEntry("pdf text current 1 3", "test-agent", DateTimeOffset.UtcNow);

        var progressProjection = service.ApplyEvent(entry, new MspCommandEvent
        {
            Kind = MspCommandEventKind.Progress,
            Message = "Extracting text",
            Percent = 45
        });
        Assert.Equal("Extracting text", entry.ProgressMessage);
        Assert.Equal(45, entry.ProgressPercent);
        Assert.Equal("Extracting text", progressProjection.StatusMessage);
        Assert.False(progressProjection.ShouldRefreshTimeline);

        var completedProjection = service.ApplyEvent(entry, new MspCommandEvent
        {
            Kind = MspCommandEventKind.Completed,
            ExitCode = 0
        });
        var canceled = service.CreateRunningEntry("chat ask current hi", "test-agent", DateTimeOffset.UtcNow);
        var canceledProjection = service.ApplyEvent(canceled, new MspCommandEvent
        {
            Kind = MspCommandEventKind.Canceled
        });

        Assert.False(entry.IsRunning);
        Assert.Equal(0, entry.ExitCode);
        Assert.Equal(100, entry.ProgressPercent);
        Assert.True(completedProjection.ShouldRefreshTimeline);

        Assert.True(canceled.WasCanceled);
        Assert.False(canceled.IsRunning);
        Assert.Equal(130, canceled.ExitCode);
        Assert.True(canceledProjection.ShouldRefreshTimeline);
    }

    [Fact]
    public void CompleteEntry_projects_audit_artifacts_diagnostics_and_recovery()
    {
        var completedAt = new DateTimeOffset(2026, 7, 7, 10, 1, 0, TimeSpan.Zero);
        var service = new ReadOsMspCommandTranscriptService("default-session");
        var entry = service.CreateRunningEntry("artifact write /artifacts/a.md a", "test-agent", DateTimeOffset.UtcNow);
        var result = MspCommandResult.Success(
            "artifact\t/artifacts/a.md",
            new[]
            {
                new MspArtifact
                {
                    Path = "/artifacts/a.md"
                }
            },
            new[]
            {
                MspCommandDiagnostic.Error(
                    "reados.test.warning",
                    "diagnostic message",
                    target: "target",
                    recoveryHint: "Try again.")
            }) with
        {
            AuditRecords = new[]
            {
                new MspAuditRecord
                {
                    CommandName = "artifact",
                    CommandText = "artifact write /artifacts/a.md a",
                    Decision = MspPolicyDecision.Allow,
                    Effects = MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact,
                    Preview = MspCommandPreview.Create("Write artifact", new[] { "/artifacts/a.md" })
                }
            }
        };

        service.CompleteEntry(entry, result, completedAt);

        Assert.False(entry.IsRunning);
        Assert.Equal(completedAt, entry.CompletedAt);
        Assert.Equal(0, entry.ExitCode);
        Assert.Equal("artifact\t/artifacts/a.md", entry.Stdout);
        Assert.Equal("Allow", entry.Decision);
        Assert.Equal("WriteWorkspace, CreateArtifact", entry.Effects);
        Assert.Equal("/artifacts/a.md", entry.ArtifactsSummary);
        Assert.Contains("Write artifact", entry.PolicyPreview);
        Assert.Contains("reados.test.warning", entry.DiagnosticsSummary);
        Assert.Contains("diagnostic message", entry.DiagnosticsSummary);
        Assert.Equal("Try again.", entry.RecoveryHint);
    }

    [Fact]
    public void CompleteEntry_preserves_confirmation_decision_without_audit_and_maps_canceled_result()
    {
        var service = new ReadOsMspCommandTranscriptService("default-session");
        var pending = service.CreateRunningEntry("artifact write /artifacts/a.md a", "test-agent", DateTimeOffset.UtcNow);
        pending.Decision = "RequireConfirmation";

        service.CompleteEntry(
            pending,
            MspCommandResult.Failure(
                "Policy confirmation required.",
                exitCode: 126,
                code: "msp.policy.require_confirmation",
                recoveryHint: "Approve or deny."),
            DateTimeOffset.UtcNow);

        var canceled = service.CreateRunningEntry("chat ask current hi", "test-agent", DateTimeOffset.UtcNow);
        service.MarkOperatorCanceled(canceled);
        service.CompleteEntry(
            canceled,
            MspCommandResult.Failure(
                "Operator canceled MSP command.",
                exitCode: 130,
                code: "msp.canceled",
                recoveryHint: "Rerun."),
            DateTimeOffset.UtcNow);

        Assert.Equal("RequireConfirmation", pending.Decision);
        Assert.Equal(126, pending.ExitCode);
        Assert.Contains("msp.policy.require_confirmation", pending.DiagnosticsSummary);
        Assert.Equal("Approve or deny.", pending.RecoveryHint);

        Assert.Equal("Canceled", canceled.Decision);
        Assert.True(canceled.WasCanceled);
        Assert.Equal(130, canceled.ExitCode);
        Assert.Equal("MSP 命令已取消。", canceled.ProgressMessage);
    }

    [Fact]
    public void FormatDiagnostics_ignores_empty_items_and_uses_last_recovery_hint()
    {
        var service = new ReadOsMspCommandTranscriptService("default-session");
        var diagnostics = new[]
        {
            new MspCommandDiagnostic
            {
                Code = string.Empty,
                Message = string.Empty
            },
            MspCommandDiagnostic.Error("first", "first message", recoveryHint: "first recovery"),
            MspCommandDiagnostic.Error("second", "second message", recoveryHint: "second recovery")
        };

        var summary = service.FormatDiagnostics(diagnostics);
        var recoveryHint = service.GetRecoveryHint(diagnostics);

        Assert.Contains("first message", summary);
        Assert.Contains("second message", summary);
        Assert.Equal("second recovery", recoveryHint);
    }
}
