using ReadOS.Msp.Audit;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Runtime;

public sealed class MspCommandContext
{
    public MspCommandContext(
        IMspWorkspace workspace,
        MspCommandRegistry registry,
        IMspPolicy policy,
        IMspAuditSink audit,
        string workingDirectory = "/",
        IServiceProvider? services = null,
        MspCommandInvocation? invocation = null,
        IMspCommandEventSink? events = null)
    {
        ArgumentNullException.ThrowIfNull(workspace);
        ArgumentNullException.ThrowIfNull(registry);
        ArgumentNullException.ThrowIfNull(policy);
        ArgumentNullException.ThrowIfNull(audit);

        Workspace = workspace;
        Registry = registry;
        Policy = policy;
        Audit = audit;
        WorkingDirectory = workspace.NormalizePath(NormalizeWorkingDirectory(workingDirectory));
        Services = services;
        Invocation = invocation ?? MspCommandInvocation.Empty;
        Events = events;
    }

    public IMspWorkspace Workspace { get; }

    public MspCommandRegistry Registry { get; }

    public IMspPolicy Policy { get; }

    public IMspAuditSink Audit { get; }

    public string WorkingDirectory { get; }

    public IServiceProvider? Services { get; }

    public MspCommandInvocation Invocation { get; }

    public IMspCommandEventSink? Events { get; }

    public MspCommandContext WithWorkingDirectory(string workingDirectory)
    {
        return new MspCommandContext(Workspace, Registry, Policy, Audit, workingDirectory, Services, Invocation, Events);
    }

    public MspCommandContext WithInvocation(MspCommandInvocation invocation)
    {
        return new MspCommandContext(Workspace, Registry, Policy, Audit, WorkingDirectory, Services, invocation, Events);
    }

    public MspCommandContext WithEventSink(IMspCommandEventSink? events)
    {
        return new MspCommandContext(Workspace, Registry, Policy, Audit, WorkingDirectory, Services, Invocation, events);
    }

    private static string NormalizeWorkingDirectory(string? workingDirectory)
    {
        return string.IsNullOrWhiteSpace(workingDirectory)
            ? "/"
            : workingDirectory.Trim();
    }

    public ValueTask ReportProgressAsync(
        string message,
        int? percent = null,
        CancellationToken cancellationToken = default)
    {
        if (Events is null)
        {
            return ValueTask.CompletedTask;
        }

        return Events.PublishAsync(new MspCommandEvent
        {
            Kind = MspCommandEventKind.Progress,
            Actor = Invocation.Actor,
            SessionId = Invocation.SessionId,
            CommandText = Invocation.CommandText,
            CommandName = Invocation.CommandName,
            Message = message,
            Percent = percent
        }, cancellationToken);
    }
}
