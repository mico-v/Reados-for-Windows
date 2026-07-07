using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Models;
using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Hosting.Tests.Runtime;

public sealed class MspCommandHostDiagnosticsServiceTests
{
    [Fact]
    public void Build_projects_request_defaults_and_command_counts()
    {
        var service = new MspCommandHostDiagnosticsService();
        var composition = new MspCommandHostCompositionBuilder().Build(new MspCommandPack(
            "ReadOS app commands",
            new IMspCommand[]
            {
                new TestCommand("workspace"),
                new TestCommand("workflow")
            }));
        var requestFactory = new MspCommandRequestFactory("reados-workbench", "reados-agent", "/workspace");

        var diagnostics = service.Build(composition, requestFactory);

        Assert.Equal("reados-workbench", diagnostics.DefaultSessionId);
        Assert.Equal("reados-agent", diagnostics.DefaultActor);
        Assert.Equal("/workspace", diagnostics.DefaultWorkingDirectory);
        Assert.Equal("ReadOS app commands", diagnostics.HostCommandPackName);
        Assert.True(diagnostics.CoreCommandCount > 0);
        Assert.Equal(2, diagnostics.HostCommandCount);
        Assert.Equal(composition.CommandNames.Count, diagnostics.CommandCount);
        Assert.Equal(new[] { "workspace", "workflow" }, diagnostics.HostCommandNames);
        Assert.Equal(new[] { "workflow" }, diagnostics.OverriddenCoreCommandNames);
        Assert.True(diagnostics.HasCoreOverrides);
    }

    [Fact]
    public void Build_reports_no_overrides_for_host_only_commands()
    {
        var service = new MspCommandHostDiagnosticsService();
        var composition = new MspCommandHostCompositionBuilder().Build(new MspCommandPack(
            "Host commands",
            new IMspCommand[] { new TestCommand("custom") }));

        var diagnostics = service.Build(
            composition,
            new MspCommandRequestFactory("session", "agent"));

        Assert.Equal(new[] { "custom" }, diagnostics.HostCommandNames);
        Assert.Empty(diagnostics.OverriddenCoreCommandNames);
        Assert.False(diagnostics.HasCoreOverrides);
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
            return ValueTask.FromResult(MspCommandResult.Success(Name));
        }
    }
}
