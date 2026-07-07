using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Models;
using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Hosting.Tests.Runtime;

public sealed class MspCommandHostCompositionBuilderTests
{
    [Fact]
    public void Build_starts_with_core_registry_and_registers_host_commands()
    {
        var builder = new MspCommandHostCompositionBuilder();
        var workspace = new TestCommand("workspace");
        var library = new TestCommand("library");

        var composition = builder.Build(new IMspCommand[] { workspace, library });

        Assert.Contains("help", composition.CoreCommandNames);
        Assert.Contains("cat", composition.CoreCommandNames);
        Assert.Equal("Host commands", composition.HostCommandPackName);
        Assert.Equal(new[] { "workspace", "library" }, composition.HostCommandNames);
        Assert.Empty(composition.OverriddenCoreCommandNames);
        Assert.True(composition.Registry.TryGet("workspace", out var workspaceCommand));
        Assert.True(composition.Registry.TryGet("library", out var libraryCommand));
        Assert.Same(workspace, workspaceCommand);
        Assert.Same(library, libraryCommand);
        Assert.Contains("workspace", composition.CommandNames);
        Assert.Contains("library", composition.CommandNames);
    }

    [Fact]
    public void Build_allows_host_commands_to_override_core_commands()
    {
        var builder = new MspCommandHostCompositionBuilder();
        var hostWorkflow = new TestCommand("workflow");

        var composition = builder.Build(new IMspCommand[] { hostWorkflow });

        Assert.Contains("workflow", composition.CoreCommandNames);
        Assert.Equal(new[] { "workflow" }, composition.HostCommandNames);
        Assert.Equal(new[] { "workflow" }, composition.OverriddenCoreCommandNames);
        Assert.True(composition.Registry.TryGet("workflow", out var command));
        Assert.Same(hostWorkflow, command);
        Assert.Equal(1, composition.CommandNames.Count(name =>
            string.Equals(name, "workflow", StringComparison.OrdinalIgnoreCase)));
    }

    [Fact]
    public void Build_accepts_named_host_command_pack()
    {
        var builder = new MspCommandHostCompositionBuilder();
        var workspace = new TestCommand("workspace");
        var workflow = new TestCommand("workflow");
        var commandPack = new MspCommandPack("ReadOS app commands", new IMspCommand[] { workspace, workflow });

        var composition = builder.Build(commandPack);

        Assert.Equal("ReadOS app commands", commandPack.Name);
        Assert.Equal(new[] { "workspace", "workflow" }, commandPack.CommandNames);
        Assert.Equal("ReadOS app commands", composition.HostCommandPackName);
        Assert.Equal(new[] { "workspace", "workflow" }, composition.HostCommandNames);
        Assert.Equal(new[] { "workflow" }, composition.OverriddenCoreCommandNames);
        Assert.True(composition.Registry.TryGet("workspace", out var workspaceCommand));
        Assert.Same(workspace, workspaceCommand);
        Assert.True(composition.Registry.TryGet("workflow", out var workflowCommand));
        Assert.Same(workflow, workflowCommand);
    }

    [Fact]
    public void Build_can_use_custom_core_registry_factory_for_host_level_tests()
    {
        var core = new TestCommand("core");
        var host = new TestCommand("host");
        var builder = new MspCommandHostCompositionBuilder(() => new MspCommandRegistry().Register(core));

        var composition = builder.Build(new IMspCommand[] { host });

        Assert.Equal(new[] { "core" }, composition.CoreCommandNames);
        Assert.Equal(new[] { "host" }, composition.HostCommandNames);
        Assert.Equal(new[] { "core", "host" }, composition.CommandNames);
        Assert.True(composition.Registry.TryGet("core", out _));
        Assert.True(composition.Registry.TryGet("host", out _));
    }

    [Fact]
    public void Constructor_rejects_null_core_registry_factory()
    {
        var exception = Assert.Throws<ArgumentNullException>(() =>
            new MspCommandHostCompositionBuilder(null!));

        Assert.Equal("coreRegistryFactory", exception.ParamName);
    }

    [Fact]
    public void Build_rejects_null_host_command_pack()
    {
        var builder = new MspCommandHostCompositionBuilder();

        var exception = Assert.Throws<ArgumentNullException>(() =>
            builder.Build((MspCommandPack)null!));

        Assert.Equal("hostCommandPack", exception.ParamName);
    }

    [Fact]
    public void Build_rejects_null_core_registry_from_factory()
    {
        var builder = new MspCommandHostCompositionBuilder(() => null!);

        var exception = Assert.Throws<InvalidOperationException>(() =>
            builder.Build(Array.Empty<IMspCommand>()));

        Assert.Contains("Core registry factory returned null", exception.Message);
    }

    [Fact]
    public void Build_propagates_core_registry_empty_name_validation()
    {
        var builder = new MspCommandHostCompositionBuilder(() =>
            new MspCommandRegistry().Register(new TestCommand(" ")));

        var exception = Assert.Throws<ArgumentException>(() =>
            builder.Build(Array.Empty<IMspCommand>()));

        Assert.Equal("command", exception.ParamName);
        Assert.Contains("Command name cannot be empty", exception.Message);
    }

    [Fact]
    public void Build_propagates_core_registry_unsupported_character_validation()
    {
        var builder = new MspCommandHostCompositionBuilder(() =>
            new MspCommandRegistry().Register(new TestCommand("bad|core")));

        var exception = Assert.Throws<ArgumentException>(() =>
            builder.Build(Array.Empty<IMspCommand>()));

        Assert.Equal("command", exception.ParamName);
        Assert.Contains("unsupported characters", exception.Message);
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
