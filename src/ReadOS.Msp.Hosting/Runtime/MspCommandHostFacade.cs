using ReadOS.Msp.Hosting.Policy;
using ReadOS.Msp.Models;

namespace ReadOS.Msp.Hosting.Runtime;

public sealed class MspCommandHostFacade
{
    private readonly IMspCommandHost commandHost;
    private readonly MspCommandRequestFactory requestFactory;
    private readonly MspApprovedCommandExecutor approvedCommandExecutor;

    public MspCommandHostFacade(
        IMspCommandHost commandHost,
        MspCommandRequestFactory requestFactory,
        IMspApprovalGrantStore approvalGrants)
    {
        this.commandHost = commandHost ?? throw new ArgumentNullException(nameof(commandHost));
        this.requestFactory = requestFactory ?? throw new ArgumentNullException(nameof(requestFactory));
        ArgumentNullException.ThrowIfNull(approvalGrants);

        approvedCommandExecutor = new MspApprovedCommandExecutor(
            commandHost,
            approvalGrants,
            requestFactory);
    }

    public ValueTask<MspCommandResult> ExecuteAsync(
        string commandText,
        string? actor = null,
        CancellationToken cancellationToken = default)
    {
        return commandHost.ExecuteAsync(
            requestFactory.Create(commandText, actor),
            cancellationToken);
    }

    public IAsyncEnumerable<MspCommandEvent> ExecuteStreamingAsync(
        string commandText,
        string? actor = null,
        CancellationToken cancellationToken = default)
    {
        return commandHost.ExecuteStreamingAsync(
            requestFactory.Create(commandText, actor),
            cancellationToken);
    }

    public ValueTask<MspCommandResult> ExecuteApprovedAsync(
        string commandText,
        string? actor = null,
        CancellationToken cancellationToken = default)
    {
        return approvedCommandExecutor.ExecuteApprovedAsync(
            commandText,
            actor,
            cancellationToken);
    }

    public IAsyncEnumerable<MspCommandEvent> ExecuteApprovedStreamingAsync(
        string commandText,
        string? actor = null,
        CancellationToken cancellationToken = default)
    {
        return approvedCommandExecutor.ExecuteApprovedStreamingAsync(
            commandText,
            actor,
            cancellationToken);
    }
}
