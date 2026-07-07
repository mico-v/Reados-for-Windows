using ReadOS.Msp.Audit;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Runtime;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Tests.Runtime;

public sealed class MspCommandContextTests
{
    [Fact]
    public void Constructor_preserves_supplied_components()
    {
        var workspace = new InMemoryMspWorkspace();
        var registry = new MspCommandRegistry();
        var policy = new AllowAllMspPolicy();
        var audit = new InMemoryMspAuditSink();
        var services = new TestServiceProvider();

        var context = new MspCommandContext(
            workspace,
            registry,
            policy,
            audit,
            "/documents",
            services);

        Assert.Same(workspace, context.Workspace);
        Assert.Same(registry, context.Registry);
        Assert.Same(policy, context.Policy);
        Assert.Same(audit, context.Audit);
        Assert.Same(services, context.Services);
        Assert.Equal("/documents", context.WorkingDirectory);
        Assert.Same(MspCommandInvocation.Empty, context.Invocation);
    }

    [Fact]
    public void Constructor_normalizes_working_directory_metadata()
    {
        var context = CreateContext(" /documents/../sessions/ ");

        Assert.Equal("/sessions", context.WorkingDirectory);
    }

    [Fact]
    public void Constructor_uses_root_for_empty_working_directory_metadata()
    {
        var context = CreateContext(" ");

        Assert.Equal("/", context.WorkingDirectory);
    }

    [Fact]
    public void Constructor_rejects_null_required_dependencies()
    {
        var workspace = new InMemoryMspWorkspace();
        var registry = new MspCommandRegistry();
        var policy = new AllowAllMspPolicy();
        var audit = new InMemoryMspAuditSink();

        Assert.Equal(
            "workspace",
            Assert.Throws<ArgumentNullException>(() =>
                new MspCommandContext(null!, registry, policy, audit)).ParamName);
        Assert.Equal(
            "registry",
            Assert.Throws<ArgumentNullException>(() =>
                new MspCommandContext(workspace, null!, policy, audit)).ParamName);
        Assert.Equal(
            "policy",
            Assert.Throws<ArgumentNullException>(() =>
                new MspCommandContext(workspace, registry, null!, audit)).ParamName);
        Assert.Equal(
            "audit",
            Assert.Throws<ArgumentNullException>(() =>
                new MspCommandContext(workspace, registry, policy, null!)).ParamName);
    }

    [Fact]
    public void WithWorkingDirectory_normalizes_working_directory_metadata()
    {
        var context = CreateContext("/documents");

        var updated = context.WithWorkingDirectory(" ../sessions/ ");

        Assert.Equal("/sessions", updated.WorkingDirectory);
    }

    private static MspCommandContext CreateContext(string workingDirectory)
    {
        return new MspCommandContext(
            new InMemoryMspWorkspace(),
            new MspCommandRegistry(),
            new AllowAllMspPolicy(),
            new InMemoryMspAuditSink(),
            workingDirectory);
    }

    private sealed class TestServiceProvider : IServiceProvider
    {
        public object? GetService(Type serviceType)
        {
            return null;
        }
    }
}
