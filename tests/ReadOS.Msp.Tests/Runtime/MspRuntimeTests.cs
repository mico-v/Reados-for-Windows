using ReadOS.Msp.Models;
using ReadOS.Msp.Parsing;
using ReadOS.Msp.Runtime;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Tests.Runtime;

public sealed class MspRuntimeTests
{
    [Fact]
    public void Parser_preserves_quoted_arguments()
    {
        var parsed = MspCommandLineParser.Parse("echo \"hello world\" '/docs/a b.txt'");

        Assert.Equal("echo", parsed.Name);
        Assert.Equal(new[] { "hello world", "/docs/a b.txt" }, parsed.Arguments);
    }

    [Fact]
    public async Task Runtime_executes_basic_workspace_commands()
    {
        var workspace = new InMemoryMspWorkspace();
        await workspace.WriteTextAsync("/notes/intro.txt", "hello MSP");
        var runtime = MspRuntime.CreateDefault(workspace);

        var list = await runtime.ExecuteAsync(new MspCommandRequest { CommandText = "ls /notes" });
        var cat = await runtime.ExecuteAsync(new MspCommandRequest { CommandText = "cat /notes/intro.txt" });

        Assert.True(list.Succeeded, list.Stderr);
        Assert.Contains("intro.txt", list.Stdout);
        Assert.True(cat.Succeeded, cat.Stderr);
        Assert.Equal("hello MSP" + Environment.NewLine, cat.Stdout);
    }

    [Fact]
    public async Task Runtime_records_audit_for_each_command()
    {
        var runtime = MspRuntime.CreateDefault();

        var result = await runtime.ExecuteAsync(new MspCommandRequest
        {
            Actor = "test-agent",
            CommandText = "echo audit"
        });

        var record = Assert.Single(result.AuditRecords);
        Assert.Equal("test-agent", record.Actor);
        Assert.Equal("echo", record.CommandName);
        Assert.Equal(0, record.ExitCode);
    }
}
