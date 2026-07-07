using ReadOS.Msp.Audit;
using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Runtime;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Hosting.Tests.Runtime;

public sealed class MspRuntimeHostFactoryTests
{
    [Fact]
    public void Build_creates_runtime_context_from_supplied_components()
    {
        var workspace = new InMemoryMspWorkspace();
        var registry = new MspCommandRegistry().Register(new TestCommand("host"));
        var policy = new AllowAllMspPolicy();
        var audit = new InMemoryMspAuditSink();
        var services = new TestServiceProvider();
        var factory = new MspRuntimeHostFactory();

        var host = factory.Build(workspace, registry, policy, audit, "/documents", services);

        Assert.Same(workspace, host.Context.Workspace);
        Assert.Same(registry, host.Context.Registry);
        Assert.Same(policy, host.Context.Policy);
        Assert.Same(audit, host.Context.Audit);
        Assert.Same(services, host.Context.Services);
        Assert.Equal("/documents", host.Context.WorkingDirectory);
        Assert.NotNull(host.Runtime);
        Assert.NotNull(host.CommandHost);
    }

    [Fact]
    public async Task Runtime_uses_composed_context_for_execution_and_audit()
    {
        var workspace = new InMemoryMspWorkspace();
        var registry = new MspCommandRegistry().Register(new TestCommand("host"));
        var audit = new InMemoryMspAuditSink();
        var factory = new MspRuntimeHostFactory();
        var host = factory.Build(workspace, registry, new AllowAllMspPolicy(), audit);

        var result = await host.Runtime.ExecuteAsync(new MspCommandRequest
        {
            Actor = "tester",
            SessionId = "session-1",
            CommandText = "host alpha"
        });

        Assert.True(result.Succeeded, result.Stderr);
        Assert.Equal("alpha", result.Stdout);
        var record = Assert.Single(audit.Records);
        Assert.Equal("tester", record.Actor);
        Assert.Equal("session-1", record.SessionId);
        Assert.Equal("host", record.CommandName);
        Assert.Equal(MspPolicyDecision.Allow, record.Decision);
    }

    [Fact]
    public void Build_uses_default_audit_sink_factory_when_audit_is_not_supplied()
    {
        var expectedAudit = new InMemoryMspAuditSink();
        var factory = new MspRuntimeHostFactory(() => expectedAudit);

        var host = factory.Build(
            new InMemoryMspWorkspace(),
            new MspCommandRegistry(),
            new AllowAllMspPolicy());

        Assert.Same(expectedAudit, host.Context.Audit);
    }

    [Fact]
    public void Build_uses_supplied_audit_without_invoking_default_factory()
    {
        var suppliedAudit = new InMemoryMspAuditSink();
        var factoryInvoked = false;
        var factory = new MspRuntimeHostFactory(() =>
        {
            factoryInvoked = true;
            return new InMemoryMspAuditSink();
        });

        var host = factory.Build(
            new InMemoryMspWorkspace(),
            new MspCommandRegistry(),
            new AllowAllMspPolicy(),
            suppliedAudit);

        Assert.Same(suppliedAudit, host.Context.Audit);
        Assert.False(factoryInvoked);
    }

    [Fact]
    public void Build_normalizes_working_directory_through_context()
    {
        var factory = new MspRuntimeHostFactory();

        var host = factory.Build(
            new InMemoryMspWorkspace(),
            new MspCommandRegistry(),
            new AllowAllMspPolicy(),
            workingDirectory: " /documents/../sessions/ ");

        Assert.Equal("/sessions", host.Context.WorkingDirectory);
    }

    [Fact]
    public void Build_uses_root_working_directory_for_empty_working_directory()
    {
        var factory = new MspRuntimeHostFactory();

        var host = factory.Build(
            new InMemoryMspWorkspace(),
            new MspCommandRegistry(),
            new AllowAllMspPolicy(),
            workingDirectory: " ");

        Assert.Equal("/", host.Context.WorkingDirectory);
    }

    [Fact]
    public void Constructor_rejects_null_audit_sink_factory()
    {
        var exception = Assert.Throws<ArgumentNullException>(() =>
            new MspRuntimeHostFactory(null!));

        Assert.Equal("auditSinkFactory", exception.ParamName);
    }

    [Fact]
    public void Build_rejects_null_required_dependencies()
    {
        var workspace = new InMemoryMspWorkspace();
        var registry = new MspCommandRegistry();
        var policy = new AllowAllMspPolicy();
        var factory = new MspRuntimeHostFactory();

        Assert.Equal(
            "workspace",
            Assert.Throws<ArgumentNullException>(() =>
                factory.Build(null!, registry, policy)).ParamName);
        Assert.Equal(
            "registry",
            Assert.Throws<ArgumentNullException>(() =>
                factory.Build(workspace, null!, policy)).ParamName);
        Assert.Equal(
            "policy",
            Assert.Throws<ArgumentNullException>(() =>
                factory.Build(workspace, registry, null!)).ParamName);
    }

    [Fact]
    public void Build_rejects_null_audit_sink_from_factory()
    {
        var factory = new MspRuntimeHostFactory(() => null!);

        var exception = Assert.Throws<InvalidOperationException>(() =>
            factory.Build(
                new InMemoryMspWorkspace(),
                new MspCommandRegistry(),
                new AllowAllMspPolicy()));

        Assert.Contains("Audit sink factory returned null", exception.Message);
    }

    private sealed class TestCommand : IMspCommand
    {
        public TestCommand(string name)
        {
            Name = name;
        }

        public string Name { get; }

        public string Summary => "Test command";

        public ValueTask<MspCommandResult> ExecuteAsync(
            MspCommandContext context,
            IReadOnlyList<string> arguments,
            CancellationToken cancellationToken = default)
        {
            return ValueTask.FromResult(MspCommandResult.Success(string.Join(' ', arguments)));
        }
    }

    private sealed class TestServiceProvider : IServiceProvider
    {
        public object? GetService(Type serviceType)
        {
            return null;
        }
    }
}
