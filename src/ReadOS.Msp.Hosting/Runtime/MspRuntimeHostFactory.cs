using ReadOS.Msp.Audit;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Runtime;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Hosting.Runtime;

public sealed class MspRuntimeHostFactory
{
    private readonly Func<IMspAuditSink> auditSinkFactory;

    public MspRuntimeHostFactory()
        : this(() => new InMemoryMspAuditSink())
    {
    }

    public MspRuntimeHostFactory(Func<IMspAuditSink> auditSinkFactory)
    {
        ArgumentNullException.ThrowIfNull(auditSinkFactory);

        this.auditSinkFactory = auditSinkFactory;
    }

    public MspRuntimeHost Build(
        IMspWorkspace workspace,
        MspCommandRegistry registry,
        IMspPolicy policy,
        IMspAuditSink? audit = null,
        string workingDirectory = "/",
        IServiceProvider? services = null)
    {
        ArgumentNullException.ThrowIfNull(workspace);
        ArgumentNullException.ThrowIfNull(registry);
        ArgumentNullException.ThrowIfNull(policy);

        var resolvedAudit = audit ?? auditSinkFactory();
        if (resolvedAudit is null)
        {
            throw new InvalidOperationException("Audit sink factory returned null.");
        }

        var resolvedWorkingDirectory = string.IsNullOrWhiteSpace(workingDirectory)
            ? "/"
            : workingDirectory.Trim();

        var context = new MspCommandContext(
            workspace,
            registry,
            policy,
            resolvedAudit,
            resolvedWorkingDirectory,
            services);
        return new MspRuntimeHost(context, new MspRuntime(context));
    }
}
