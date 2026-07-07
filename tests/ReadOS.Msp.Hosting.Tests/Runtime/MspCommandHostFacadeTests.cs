using System.Runtime.CompilerServices;
using ReadOS.Msp.Hosting.Policy;
using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;

namespace ReadOS.Msp.Hosting.Tests.Runtime;

public sealed class MspCommandHostFacadeTests
{
    [Fact]
    public void Constructor_rejects_null_dependencies()
    {
        var host = new RecordingCommandHost();
        var requestFactory = new MspCommandRequestFactory("reados-workbench");
        var approvalGrants = new MspApprovalGrantStore();

        Assert.Throws<ArgumentNullException>(() => new MspCommandHostFacade(null!, requestFactory, approvalGrants));
        Assert.Throws<ArgumentNullException>(() => new MspCommandHostFacade(host, null!, approvalGrants));
        Assert.Throws<ArgumentNullException>(() => new MspCommandHostFacade(host, requestFactory, null!));
    }

    [Fact]
    public async Task ExecuteAsync_creates_request_from_defaults()
    {
        var host = new RecordingCommandHost();
        var facade = CreateFacade(host);

        var result = await facade.ExecuteAsync("workspace info");

        Assert.True(result.Succeeded);
        Assert.NotNull(host.LastRequest);
        Assert.Equal("reados-agent", host.LastRequest.Actor);
        Assert.Equal("reados-workbench", host.LastRequest.SessionId);
        Assert.Equal("/", host.LastRequest.WorkingDirectory);
        Assert.Equal("workspace info", host.LastRequest.CommandText);
    }

    [Fact]
    public async Task ExecuteAsync_normalizes_actor_override()
    {
        var host = new RecordingCommandHost();
        var facade = CreateFacade(host);

        await facade.ExecuteAsync("workspace info", " operator ");

        Assert.NotNull(host.LastRequest);
        Assert.Equal("operator", host.LastRequest.Actor);
    }

    [Fact]
    public async Task ExecuteStreamingAsync_creates_streaming_request_from_actor_override()
    {
        var host = new RecordingCommandHost();
        var facade = CreateFacade(host);
        var events = new List<MspCommandEvent>();

        await foreach (var commandEvent in facade.ExecuteStreamingAsync("workspace info", "operator"))
        {
            events.Add(commandEvent);
        }

        Assert.Single(events);
        Assert.NotNull(host.LastStreamingRequest);
        Assert.Equal("operator", host.LastStreamingRequest.Actor);
        Assert.Equal("reados-workbench", host.LastStreamingRequest.SessionId);
        Assert.Equal("workspace info", host.LastStreamingRequest.CommandText);
    }

    [Fact]
    public async Task ExecuteStreamingAsync_uses_default_actor_for_empty_actor_override()
    {
        var host = new RecordingCommandHost();
        var facade = CreateFacade(host);
        var events = new List<MspCommandEvent>();

        await foreach (var commandEvent in facade.ExecuteStreamingAsync("workspace info", " "))
        {
            events.Add(commandEvent);
        }

        Assert.Single(events);
        Assert.NotNull(host.LastStreamingRequest);
        Assert.Equal("reados-agent", host.LastStreamingRequest.Actor);
    }

    [Fact]
    public async Task ExecuteApprovedAsync_injects_approval_environment()
    {
        var host = new RecordingCommandHost();
        var approvalGrants = new MspApprovalGrantStore();
        var facade = CreateFacade(host, approvalGrants);

        var result = await facade.ExecuteApprovedAsync("artifact write /artifacts/a.md a", "operator");

        Assert.True(result.Succeeded);
        Assert.NotNull(host.LastRequest);
        Assert.Equal("operator", host.LastRequest.Actor);
        Assert.True(host.LastRequest.Environment.ContainsKey(MspApprovalGrantStore.DefaultEnvironmentKey));
        Assert.False(approvalGrants.TryConsumeApproval(CreatePolicyRequest(host.LastRequest)));
    }

    [Fact]
    public async Task ExecuteApprovedAsync_normalizes_actor_override()
    {
        var host = new RecordingCommandHost();
        var approvalGrants = new MspApprovalGrantStore();
        var facade = CreateFacade(host, approvalGrants);

        var result = await facade.ExecuteApprovedAsync("artifact write /artifacts/a.md a", " operator ");

        Assert.True(result.Succeeded);
        Assert.NotNull(host.LastRequest);
        Assert.Equal("operator", host.LastRequest.Actor);
        Assert.False(approvalGrants.TryConsumeApproval(CreatePolicyRequest(host.LastRequest)));
    }

    [Fact]
    public async Task ExecuteApprovedStreamingAsync_injects_approval_environment()
    {
        var host = new RecordingCommandHost();
        var approvalGrants = new MspApprovalGrantStore();
        var facade = CreateFacade(host, approvalGrants);
        var events = new List<MspCommandEvent>();

        await foreach (var commandEvent in facade.ExecuteApprovedStreamingAsync("artifact write /artifacts/a.md a"))
        {
            events.Add(commandEvent);
        }

        Assert.Single(events);
        Assert.NotNull(host.LastStreamingRequest);
        Assert.Equal("reados-agent", host.LastStreamingRequest.Actor);
        Assert.True(host.LastStreamingRequest.Environment.ContainsKey(MspApprovalGrantStore.DefaultEnvironmentKey));
        Assert.False(approvalGrants.TryConsumeApproval(CreatePolicyRequest(host.LastStreamingRequest)));
    }

    private static MspCommandHostFacade CreateFacade(
        RecordingCommandHost host,
        MspApprovalGrantStore? approvalGrants = null)
    {
        return new MspCommandHostFacade(
            host,
            new MspCommandRequestFactory("reados-workbench", "reados-agent"),
            approvalGrants ?? new MspApprovalGrantStore());
    }

    private sealed class RecordingCommandHost : IMspCommandHost
    {
        public MspCommandRequest? LastRequest { get; private set; }

        public MspCommandRequest? LastStreamingRequest { get; private set; }

        public ValueTask<MspCommandResult> ExecuteAsync(
            MspCommandRequest request,
            CancellationToken cancellationToken = default)
        {
            LastRequest = request;
            return ValueTask.FromResult(MspCommandResult.Success("ok"));
        }

        public async IAsyncEnumerable<MspCommandEvent> ExecuteStreamingAsync(
            MspCommandRequest request,
            [EnumeratorCancellation] CancellationToken cancellationToken = default)
        {
            LastStreamingRequest = request;
            await Task.Yield();
            yield return new MspCommandEvent
            {
                Kind = MspCommandEventKind.Progress,
                Actor = request.Actor,
                SessionId = request.SessionId,
                CommandText = request.CommandText,
                Message = "running"
            };
        }
    }

    private static MspPolicyRequest CreatePolicyRequest(MspCommandRequest request)
    {
        return new MspPolicyRequest
        {
            CommandName = request.CommandText.Split(' ', 2)[0],
            CommandText = request.CommandText,
            Actor = request.Actor,
            SessionId = request.SessionId,
            WorkingDirectory = request.WorkingDirectory,
            DryRun = request.DryRun,
            Environment = request.Environment,
            CommandMetadata = MspCommandMetadata.Create(
                request.CommandText.Split(' ', 2)[0],
                "test command",
                effects: MspCommandEffects.WriteWorkspace)
        };
    }
}
