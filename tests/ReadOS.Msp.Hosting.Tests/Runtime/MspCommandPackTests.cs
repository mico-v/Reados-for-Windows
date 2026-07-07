using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Models;
using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Hosting.Tests.Runtime;

public sealed class MspCommandPackTests
{
    [Fact]
    public void Constructor_trims_pack_name_and_preserves_command_order()
    {
        var workspace = new TestCommand("workspace");
        var library = new TestCommand("library");

        var commandPack = new MspCommandPack(
            " ReadOS app commands ",
            new IMspCommand[] { workspace, library });

        Assert.Equal("ReadOS app commands", commandPack.Name);
        Assert.Equal(new[] { workspace, library }, commandPack.Commands);
        Assert.Equal(new[] { "workspace", "library" }, commandPack.CommandNames);
    }

    [Fact]
    public void Constructor_accepts_token_safe_command_name_characters()
    {
        var pageLabel = new TestCommand("page-label");
        var artifactWrite = new TestCommand("artifact.write");
        var artifactPreview = new TestCommand("artifact_preview");

        var commandPack = new MspCommandPack(
            "Host commands",
            new IMspCommand[] { pageLabel, artifactWrite, artifactPreview });

        Assert.Equal(
            new[] { "page-label", "artifact.write", "artifact_preview" },
            commandPack.CommandNames);
    }

    [Fact]
    public void Constructor_rejects_empty_pack_name()
    {
        var exception = Assert.Throws<ArgumentException>(() =>
            new MspCommandPack(" ", Array.Empty<IMspCommand>()));

        Assert.Equal("name", exception.ParamName);
        Assert.Contains("must not be empty", exception.Message);
    }

    [Fact]
    public void Constructor_rejects_null_command()
    {
        var exception = Assert.Throws<ArgumentException>(() =>
            new MspCommandPack(
                "Host commands",
                new[] { (IMspCommand)null! }));

        Assert.Equal("commands", exception.ParamName);
        Assert.Contains("null command", exception.Message);
    }

    [Fact]
    public void Constructor_rejects_empty_command_name()
    {
        var exception = Assert.Throws<ArgumentException>(() =>
            new MspCommandPack(
                "Host commands",
                new IMspCommand[] { new TestCommand(" ") }));

        Assert.Equal("commands", exception.ParamName);
        Assert.Contains("empty name", exception.Message);
    }

    [Fact]
    public void Constructor_rejects_command_name_with_leading_or_trailing_whitespace()
    {
        var exception = Assert.Throws<ArgumentException>(() =>
            new MspCommandPack(
                "Host commands",
                new IMspCommand[] { new TestCommand(" workspace") }));

        Assert.Equal("commands", exception.ParamName);
        Assert.Contains("leading or trailing whitespace", exception.Message);
    }

    [Fact]
    public void Constructor_rejects_command_name_with_internal_whitespace()
    {
        var exception = Assert.Throws<ArgumentException>(() =>
            new MspCommandPack(
                "Host commands",
                new IMspCommand[] { new TestCommand("work space") }));

        Assert.Equal("commands", exception.ParamName);
        Assert.Contains("with whitespace", exception.Message);
    }

    [Fact]
    public void Constructor_rejects_command_name_with_unsupported_token_characters()
    {
        var exception = Assert.Throws<ArgumentException>(() =>
            new MspCommandPack(
                "Host commands",
                new IMspCommand[] { new TestCommand("bad|name") }));

        Assert.Equal("commands", exception.ParamName);
        Assert.Contains("unsupported characters", exception.Message);
    }

    [Fact]
    public void Constructor_rejects_duplicate_command_names_case_insensitively()
    {
        var exception = Assert.Throws<ArgumentException>(() =>
            new MspCommandPack(
                "Host commands",
                new IMspCommand[]
                {
                    new TestCommand("workspace"),
                    new TestCommand("Workspace")
                }));

        Assert.Equal("commands", exception.ParamName);
        Assert.Contains("duplicate command name", exception.Message);
    }

    [Fact]
    public void Constructor_allows_host_command_to_share_core_command_name()
    {
        var commandPack = new MspCommandPack(
            "ReadOS app commands",
            new IMspCommand[] { new TestCommand("workflow") });

        var composition = new MspCommandHostCompositionBuilder().Build(commandPack);

        Assert.Equal(new[] { "workflow" }, commandPack.CommandNames);
        Assert.Equal(new[] { "workflow" }, composition.OverriddenCoreCommandNames);
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
