using System.Runtime.CompilerServices;
using ReadOS.Msp.Audit;
using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Runtime;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Hosting.Tests.Runtime;

public sealed class MspRuntimeHostTests
{
    [Fact]
    public void Constructor_creates_default_command_host()
    {
        var context = CreateContext();
        var runtime = new MspRuntime(context);

        var host = new MspRuntimeHost(context, runtime);

        Assert.Same(context, host.Context);
        Assert.Same(runtime, host.Runtime);
        Assert.IsType<MspRuntimeCommandHost>(host.CommandHost);
    }

    [Fact]
    public void Constructor_preserves_supplied_command_host()
    {
        var context = CreateContext();
        var runtime = new MspRuntime(context);
        var commandHost = new RecordingCommandHost();

        var host = new MspRuntimeHost(context, runtime, commandHost);

        Assert.Same(context, host.Context);
        Assert.Same(runtime, host.Runtime);
        Assert.Same(commandHost, host.CommandHost);
    }

    [Fact]
    public void Constructor_rejects_null_context_for_default_command_host()
    {
        var context = CreateContext();
        var runtime = new MspRuntime(context);

        var exception = Assert.Throws<ArgumentNullException>(() =>
            new MspRuntimeHost(null!, runtime));

        Assert.Equal("context", exception.ParamName);
    }

    [Fact]
    public void Constructor_rejects_null_runtime_for_default_command_host()
    {
        var context = CreateContext();

        var exception = Assert.Throws<ArgumentNullException>(() =>
            new MspRuntimeHost(context, null!));

        Assert.Equal("runtime", exception.ParamName);
    }

    [Fact]
    public void Constructor_rejects_null_dependencies_for_supplied_command_host()
    {
        var context = CreateContext();
        var runtime = new MspRuntime(context);
        var commandHost = new RecordingCommandHost();

        Assert.Equal(
            "context",
            Assert.Throws<ArgumentNullException>(() =>
                new MspRuntimeHost(null!, runtime, commandHost)).ParamName);
        Assert.Equal(
            "runtime",
            Assert.Throws<ArgumentNullException>(() =>
                new MspRuntimeHost(context, null!, commandHost)).ParamName);
        Assert.Equal(
            "commandHost",
            Assert.Throws<ArgumentNullException>(() =>
                new MspRuntimeHost(context, runtime, null!)).ParamName);
    }

    private static MspCommandContext CreateContext()
    {
        return new MspCommandContext(
            new InMemoryMspWorkspace(),
            new MspCommandRegistry(),
            new AllowAllMspPolicy(),
            new InMemoryMspAuditSink());
    }

    private sealed class RecordingCommandHost : IMspCommandHost
    {
        public ValueTask<MspCommandResult> ExecuteAsync(
            MspCommandRequest request,
            CancellationToken cancellationToken = default)
        {
            return ValueTask.FromResult(MspCommandResult.Success("ok"));
        }

        public async IAsyncEnumerable<MspCommandEvent> ExecuteStreamingAsync(
            MspCommandRequest request,
            [EnumeratorCancellation] CancellationToken cancellationToken = default)
        {
            await Task.Yield();
            yield return new MspCommandEvent
            {
                Kind = MspCommandEventKind.Completed,
                Actor = request.Actor,
                SessionId = request.SessionId,
                CommandText = request.CommandText,
                Result = MspCommandResult.Success("ok")
            };
        }
    }
}
