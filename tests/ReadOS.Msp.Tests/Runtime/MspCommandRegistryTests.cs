using ReadOS.Msp.Models;
using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Tests.Runtime;

public sealed class MspCommandRegistryTests
{
    [Fact]
    public void Register_adds_command_and_lookup_is_case_insensitive()
    {
        var command = new TestCommand("artifact.write");
        var registry = new MspCommandRegistry();

        var returned = registry.Register(command);
        var found = registry.TryGet("ARTIFACT.WRITE", out var resolved);

        Assert.Same(registry, returned);
        Assert.True(found);
        Assert.Same(command, resolved);
        Assert.Contains(command, registry.Commands);
    }

    [Fact]
    public void Register_rejects_null_command()
    {
        var registry = new MspCommandRegistry();

        var exception = Assert.Throws<ArgumentNullException>(() => registry.Register(null!));

        Assert.Equal("command", exception.ParamName);
    }

    [Theory]
    [InlineData(" ", "Command name cannot be empty.")]
    [InlineData(" workspace", "leading or trailing whitespace")]
    [InlineData("work space", "cannot contain whitespace")]
    [InlineData("bad|name", "unsupported characters")]
    public void Register_rejects_invalid_command_names(string commandName, string expectedMessage)
    {
        var registry = new MspCommandRegistry();

        var exception = Assert.Throws<ArgumentException>(() =>
            registry.Register(new TestCommand(commandName)));

        Assert.Equal("command", exception.ParamName);
        Assert.Contains(expectedMessage, exception.Message);
    }

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData(" ")]
    public void TryGet_returns_false_for_empty_lookup_names(string? commandName)
    {
        var registry = new MspCommandRegistry().Register(new TestCommand("workspace"));

        var found = registry.TryGet(commandName, out var command);

        Assert.False(found);
        Assert.Null(command);
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
            return ValueTask.FromResult(MspCommandResult.Success("ok"));
        }
    }
}
