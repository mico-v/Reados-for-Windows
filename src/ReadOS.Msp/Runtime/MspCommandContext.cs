using ReadOS.Msp.Audit;
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
        IServiceProvider? services = null)
    {
        Workspace = workspace;
        Registry = registry;
        Policy = policy;
        Audit = audit;
        WorkingDirectory = workspace.NormalizePath(workingDirectory);
        Services = services;
    }

    public IMspWorkspace Workspace { get; }

    public MspCommandRegistry Registry { get; }

    public IMspPolicy Policy { get; }

    public IMspAuditSink Audit { get; }

    public string WorkingDirectory { get; }

    public IServiceProvider? Services { get; }

    public MspCommandContext WithWorkingDirectory(string workingDirectory)
    {
        return new MspCommandContext(Workspace, Registry, Policy, Audit, workingDirectory, Services);
    }
}
