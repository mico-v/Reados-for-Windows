using ReadOS.Msp.Audit;
using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Runtime;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Hosting.Tests.Runtime;

public sealed class MspRuntimeCommandHostTests
{
    [Fact]
    public void Constructor_rejects_null_runtime()
    {
        var exception = Assert.Throws<ArgumentNullException>(() => new MspRuntimeCommandHost(null!));

        Assert.Equal("runtime", exception.ParamName);
    }

    [Fact]
    public async Task ExecuteAsync_rejects_null_request()
    {
        var commandHost = CreateCommandHost(new InMemoryMspAuditSink());

        var exception = await Assert.ThrowsAsync<ArgumentNullException>(async () =>
            await commandHost.ExecuteAsync(null!));

        Assert.Equal("request", exception.ParamName);
    }

    [Fact]
    public void ExecuteStreamingAsync_rejects_null_request()
    {
        var commandHost = CreateCommandHost(new InMemoryMspAuditSink());

        var exception = Assert.Throws<ArgumentNullException>(() =>
            commandHost.ExecuteStreamingAsync(null!));

        Assert.Equal("request", exception.ParamName);
    }

    [Fact]
    public async Task ExecuteAsync_delegates_request_to_runtime()
    {
        var audit = new InMemoryMspAuditSink();
        var commandHost = CreateCommandHost(audit);

        var result = await commandHost.ExecuteAsync(new MspCommandRequest
        {
            Actor = "tester",
            SessionId = "session-1",
            CommandText = "host alpha beta"
        });

        Assert.True(result.Succeeded, result.Stderr);
        Assert.Equal("alpha beta", result.Stdout);
        var record = Assert.Single(audit.Records);
        Assert.Equal("tester", record.Actor);
        Assert.Equal("session-1", record.SessionId);
        Assert.Equal("host", record.CommandName);
    }

    [Fact]
    public async Task ExecuteStreamingAsync_delegates_streaming_request_to_runtime()
    {
        var commandHost = CreateCommandHost(new InMemoryMspAuditSink());
        var events = new List<MspCommandEvent>();

        await foreach (var commandEvent in commandHost.ExecuteStreamingAsync(new MspCommandRequest
        {
            Actor = "tester",
            SessionId = "session-1",
            CommandText = "host streamed"
        }))
        {
            events.Add(commandEvent);
        }

        Assert.Contains(events, item => item.Kind == MspCommandEventKind.Started);
        var completed = Assert.Single(events, item => item.Kind == MspCommandEventKind.Completed);
        Assert.Equal(0, completed.ExitCode);
        Assert.NotNull(completed.Result);
        Assert.Equal("streamed", completed.Result.Stdout);
    }

    private static MspRuntimeCommandHost CreateCommandHost(InMemoryMspAuditSink audit)
    {
        var registry = new MspCommandRegistry().Register(new TestCommand("host"));
        var context = new MspCommandContext(
            new InMemoryMspWorkspace(),
            registry,
            new AllowAllMspPolicy(),
            audit);
        return new MspRuntimeCommandHost(new MspRuntime(context));
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
}
