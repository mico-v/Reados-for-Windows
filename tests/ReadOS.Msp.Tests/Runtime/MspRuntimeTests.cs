using ReadOS.Msp.Audit;
using ReadOS.Msp.Models;
using ReadOS.Msp.Parsing;
using ReadOS.Msp.Policy;
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

    [Fact]
    public async Task Artifact_command_writes_lists_and_shows_workspace_artifacts()
    {
        var workspace = new InMemoryMspWorkspace();
        var runtime = MspRuntime.CreateDefault(workspace);

        var write = await runtime.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "artifact write /artifacts/summary.md \"hello artifacts\""
        });
        var list = await runtime.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "artifact list /artifacts"
        });
        var show = await runtime.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "artifact show /artifacts/summary.md"
        });

        Assert.True(write.Succeeded, write.Stderr);
        Assert.Equal("/artifacts/summary.md", Assert.Single(write.Artifacts).Path);
        Assert.True(list.Succeeded, list.Stderr);
        Assert.Contains("/artifacts/summary.md", list.Stdout);
        Assert.True(show.Succeeded, show.Stderr);
        Assert.Equal("hello artifacts" + Environment.NewLine, show.Stdout);
    }

    [Fact]
    public async Task Runtime_passes_command_metadata_to_policy_and_audit()
    {
        var policy = new CapturingPolicy();
        var registry = new MspCommandRegistry().Register(new MutatingTestCommand());
        var context = new MspCommandContext(
            new InMemoryMspWorkspace(),
            registry,
            policy,
            new InMemoryMspAuditSink());
        var runtime = new MspRuntime(context);

        var result = await runtime.ExecuteAsync(new MspCommandRequest
        {
            Actor = "metadata-test",
            CommandText = "mutate"
        });

        Assert.True(result.Succeeded, result.Stderr);
        Assert.NotNull(policy.LastRequest);
        Assert.Equal(MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact, policy.LastRequest!.Effects);
        Assert.True(policy.LastRequest.RequiresConfirmation);
        Assert.Equal("msp.test.write", Assert.Single(policy.LastRequest.Capabilities));

        var record = Assert.Single(result.AuditRecords);
        Assert.Equal(policy.LastRequest.Effects, record.Effects);
    }

    private sealed class CapturingPolicy : IMspPolicy
    {
        public MspPolicyRequest? LastRequest { get; private set; }

        public ValueTask<MspPolicyDecision> AuthorizeAsync(
            MspPolicyRequest request,
            CancellationToken cancellationToken = default)
        {
            LastRequest = request;
            return ValueTask.FromResult(MspPolicyDecision.Allow);
        }
    }

    private sealed class MutatingTestCommand : IMspCommand
    {
        public string Name => "mutate";

        public string Summary => "Mutate test workspace.";

        public MspCommandMetadata Metadata => MspCommandMetadata.Create(
            Name,
            Summary,
            "mutate",
            MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact,
            new[] { "msp.test.write" });

        public ValueTask<MspCommandResult> ExecuteAsync(
            MspCommandContext context,
            IReadOnlyList<string> arguments,
            CancellationToken cancellationToken = default)
        {
            return ValueTask.FromResult(MspCommandResult.Success());
        }
    }
}
