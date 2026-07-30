using ReadOS.App.Models;
using ReadOS.Msp.Models;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsMspCommandTranscriptService
{
    private readonly string defaultSessionId;

    public ReadOsMspCommandTranscriptService(string defaultSessionId)
    {
        this.defaultSessionId = string.IsNullOrWhiteSpace(defaultSessionId)
            ? ReadOsMspHost.DefaultSessionId
            : defaultSessionId;
    }

    public MspTranscriptEntry CreateRunningEntry(string commandText, string actor, DateTimeOffset startedAt)
    {
        return new MspTranscriptEntry
        {
            Actor = actor,
            SessionId = defaultSessionId,
            CommandText = commandText,
            StartedAt = startedAt,
            CompletedAt = startedAt,
            Decision = "Running",
            IsRunning = true,
            ProgressMessage = "MSP 命令已开始。",
            ProgressPercent = 0
        };
    }

    public MspTranscriptEventProjection ApplyEvent(MspTranscriptEntry entry, MspCommandEvent commandEvent)
    {
        if (!string.IsNullOrWhiteSpace(commandEvent.SessionId))
        {
            entry.SessionId = commandEvent.SessionId;
        }

        var statusMessage = string.Empty;
        if (!string.IsNullOrWhiteSpace(commandEvent.Message))
        {
            entry.ProgressMessage = commandEvent.Message;
            statusMessage = commandEvent.Message;
        }

        if (commandEvent.Percent is not null)
        {
            entry.ProgressPercent = commandEvent.Percent;
        }

        var shouldRefreshTimeline = false;
        var shouldOpenPolicy = false;
        switch (commandEvent.Kind)
        {
            case MspCommandEventKind.Started:
                entry.IsRunning = true;
                entry.ProgressPercent ??= 0;
                break;
            case MspCommandEventKind.PolicyDecision:
                entry.Decision = commandEvent.Decision?.ToString() ?? entry.Decision;
                entry.Effects = commandEvent.Effects.ToString();
                entry.PolicyPreview = commandEvent.Preview.ToDisplayText();
                entry.ProgressPercent ??= 10;
                shouldOpenPolicy = entry.IsApprovalRequired;
                shouldRefreshTimeline = true;
                break;
            case MspCommandEventKind.Progress:
                break;
            case MspCommandEventKind.Canceled:
                entry.WasCanceled = true;
                entry.IsRunning = false;
                entry.ExitCode = commandEvent.ExitCode ?? 130;
                shouldRefreshTimeline = true;
                break;
            case MspCommandEventKind.Completed:
                entry.IsRunning = false;
                entry.ExitCode = commandEvent.ExitCode ?? entry.ExitCode;
                if (commandEvent.ExitCode == 0)
                {
                    entry.ProgressPercent = 100;
                }

                shouldRefreshTimeline = true;
                break;
        }

        return new MspTranscriptEventProjection(
            statusMessage,
            shouldRefreshTimeline,
            shouldOpenPolicy);
    }

    public void MarkOperatorCanceled(MspTranscriptEntry entry)
    {
        entry.WasCanceled = true;
        entry.ProgressMessage = "MSP 命令已取消。";
    }

    public void CompleteEntry(MspTranscriptEntry entry, MspCommandResult result, DateTimeOffset completedAt)
    {
        var auditRecord = result.AuditRecords.LastOrDefault();
        entry.IsRunning = false;
        entry.CompletedAt = completedAt;
        entry.ExitCode = result.ExitCode;
        entry.Stdout = result.Stdout;
        entry.Stderr = result.Stderr;
        entry.WasCanceled = entry.WasCanceled || result.ExitCode == 130;
        entry.Decision = entry.WasCanceled
            ? "Canceled"
            : auditRecord?.Decision.ToString() ?? (entry.Decision == "Running" ? "Allow" : entry.Decision);
        entry.Effects = auditRecord?.Effects.ToString() ?? entry.Effects;
        entry.ArtifactsSummary = string.Join(", ", result.Artifacts.Select(artifact => artifact.Path));
        entry.PolicyPreview = auditRecord?.Preview.ToDisplayText() ?? entry.PolicyPreview;
        entry.DiagnosticsSummary = FormatDiagnostics(result.Diagnostics);
        entry.RecoveryHint = GetRecoveryHint(result.Diagnostics);
        if (entry.WasCanceled)
        {
            entry.ProgressMessage = "MSP 命令已取消。";
        }
    }

    public string FormatDiagnostics(IReadOnlyList<MspCommandDiagnostic> diagnostics)
    {
        return string.Join(
            Environment.NewLine,
            diagnostics
                .Where(diagnostic => !string.IsNullOrWhiteSpace(diagnostic.Message) || !string.IsNullOrWhiteSpace(diagnostic.Code))
                .Select(diagnostic => diagnostic.ToDisplayText()));
    }

    public string GetRecoveryHint(IReadOnlyList<MspCommandDiagnostic> diagnostics)
    {
        return diagnostics
            .Select(diagnostic => diagnostic.RecoveryHint)
            .LastOrDefault(hint => !string.IsNullOrWhiteSpace(hint)) ?? string.Empty;
    }
}

internal sealed record MspTranscriptEventProjection(
    string StatusMessage,
    bool ShouldRefreshTimeline,
    bool ShouldOpenPolicy);
