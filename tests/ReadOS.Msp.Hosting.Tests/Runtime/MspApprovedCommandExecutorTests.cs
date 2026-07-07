using System.Runtime.CompilerServices;
using ReadOS.Msp.Hosting.Policy;
using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;

namespace ReadOS.Msp.Hosting.Tests.Runtime;

public sealed class MspApprovedCommandExecutorTests
{
    [Fact]
    public async Task ExecuteApprovedAsync_injects_one_shot_approval_environment_and_revokes_after_execution()
    {
        var host = new RecordingCommandHost();
        var approvalGrants = new MspApprovalGrantStore();
        var executor = new MspApprovedCommandExecutor(
            host,
            approvalGrants,
            new MspCommandRequestFactory("reados-workbench", "reados-agent"));

        var result = await executor.ExecuteApprovedAsync(
            "artifact write /artifacts/a.md a",
            "operator");

        Assert.True(result.Succeeded);
        Assert.NotNull(host.LastRequest);
        Assert.Equal("operator", host.LastRequest.Actor);
        Assert.Equal("reados-workbench", host.LastRequest.SessionId);
        Assert.True(host.LastRequest.Environment.ContainsKey(MspApprovalGrantStore.DefaultEnvironmentKey));
        Assert.False(approvalGrants.TryConsumeApproval(CreatePolicyRequest(host.LastRequest)));
    }

    [Fact]
    public async Task ExecuteApprovedAsync_revokes_approval_when_host_throws()
    {
        var host = new RecordingCommandHost
        {
            ThrowOnExecute = true
        };
        var approvalGrants = new MspApprovalGrantStore();
        var executor = new MspApprovedCommandExecutor(
            host,
            approvalGrants,
            new MspCommandRequestFactory("reados-workbench", "reados-agent"));

        await Assert.ThrowsAsync<InvalidOperationException>(async () =>
            await executor.ExecuteApprovedAsync("artifact write /artifacts/a.md a", "operator"));

        Assert.NotNull(host.LastRequest);
        Assert.False(approvalGrants.TryConsumeApproval(CreatePolicyRequest(host.LastRequest)));
    }

    [Fact]
    public async Task ExecuteApprovedStreamingAsync_injects_environment_and_revokes_after_enumeration()
    {
        var host = new RecordingCommandHost();
        var approvalGrants = new MspApprovalGrantStore();
        var executor = new MspApprovedCommandExecutor(
            host,
            approvalGrants,
            new MspCommandRequestFactory("reados-workbench", "reados-agent"));

        var events = new List<MspCommandEvent>();
        await foreach (var commandEvent in executor.ExecuteApprovedStreamingAsync(
            "artifact write /artifacts/a.md a",
            "operator"))
        {
            events.Add(commandEvent);
        }

        Assert.Single(events);
        Assert.NotNull(host.LastStreamingRequest);
        Assert.True(host.LastStreamingRequest.Environment.ContainsKey(MspApprovalGrantStore.DefaultEnvironmentKey));
        Assert.False(approvalGrants.TryConsumeApproval(CreatePolicyRequest(host.LastStreamingRequest)));
    }

    [Fact]
    public async Task ExecuteApprovedAsync_trims_actor_before_grant_and_request_creation()
    {
        var approvalGrants = new MspApprovalGrantStore();
        var host = new ApprovalConsumingCommandHost(approvalGrants);
        var executor = new MspApprovedCommandExecutor(
            host,
            approvalGrants,
            new MspCommandRequestFactory("reados-workbench", "reados-agent"));

        var result = await executor.ExecuteApprovedAsync(
            "artifact write /artifacts/a.md a",
            " operator ");

        Assert.True(result.Succeeded);
        Assert.NotNull(host.LastRequest);
        Assert.Equal("operator", host.LastRequest.Actor);
        Assert.True(host.ConsumedApproval);
    }

    [Fact]
    public void Constructor_rejects_null_dependencies()
    {
        var host = new RecordingCommandHost();
        var approvalGrants = new MspApprovalGrantStore();
        var requestFactory = new MspCommandRequestFactory("reados-workbench", "reados-agent");

        Assert.Equal(
            "commandHost",
            Assert.Throws<ArgumentNullException>(() =>
                new MspApprovedCommandExecutor(null!, approvalGrants, requestFactory)).ParamName);
        Assert.Equal(
            "approvalGrants",
            Assert.Throws<ArgumentNullException>(() =>
                new MspApprovedCommandExecutor(host, null!, requestFactory)).ParamName);
        Assert.Equal(
            "requestFactory",
            Assert.Throws<ArgumentNullException>(() =>
                new MspApprovedCommandExecutor(host, approvalGrants, null!)).ParamName);
    }

    private sealed class RecordingCommandHost : IMspCommandHost
    {
        public MspCommandRequest? LastRequest { get; private set; }

        public MspCommandRequest? LastStreamingRequest { get; private set; }

        public bool ThrowOnExecute { get; init; }

        public ValueTask<MspCommandResult> ExecuteAsync(
            MspCommandRequest request,
            CancellationToken cancellationToken = default)
        {
            LastRequest = request;
            if (ThrowOnExecute)
            {
                throw new InvalidOperationException("host failed");
            }

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

    private sealed class ApprovalConsumingCommandHost : IMspCommandHost
    {
        private readonly MspApprovalGrantStore approvalGrants;

        public ApprovalConsumingCommandHost(MspApprovalGrantStore approvalGrants)
        {
            this.approvalGrants = approvalGrants;
        }

        public MspCommandRequest? LastRequest { get; private set; }

        public bool ConsumedApproval { get; private set; }

        public ValueTask<MspCommandResult> ExecuteAsync(
            MspCommandRequest request,
            CancellationToken cancellationToken = default)
        {
            LastRequest = request;
            ConsumedApproval = approvalGrants.TryConsumeApproval(CreatePolicyRequest(request));
            return ValueTask.FromResult(ConsumedApproval
                ? MspCommandResult.Success("ok")
                : MspCommandResult.Failure("approval was not consumed"));
        }

        public async IAsyncEnumerable<MspCommandEvent> ExecuteStreamingAsync(
            MspCommandRequest request,
            [EnumeratorCancellation] CancellationToken cancellationToken = default)
        {
            LastRequest = request;
            ConsumedApproval = approvalGrants.TryConsumeApproval(CreatePolicyRequest(request));
            await Task.Yield();
            yield return new MspCommandEvent
            {
                Kind = MspCommandEventKind.Completed,
                Actor = request.Actor,
                SessionId = request.SessionId,
                CommandText = request.CommandText,
                Result = ConsumedApproval
                    ? MspCommandResult.Success("ok")
                    : MspCommandResult.Failure("approval was not consumed")
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
