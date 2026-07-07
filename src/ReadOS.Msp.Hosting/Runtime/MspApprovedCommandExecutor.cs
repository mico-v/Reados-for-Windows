using System.Runtime.CompilerServices;
using ReadOS.Msp.Hosting.Policy;
using ReadOS.Msp.Models;

namespace ReadOS.Msp.Hosting.Runtime;

public sealed class MspApprovedCommandExecutor
{
    private readonly IMspCommandHost commandHost;
    private readonly IMspApprovalGrantStore approvalGrants;
    private readonly MspCommandRequestFactory requestFactory;

    public MspApprovedCommandExecutor(
        IMspCommandHost commandHost,
        IMspApprovalGrantStore approvalGrants,
        MspCommandRequestFactory requestFactory)
    {
        this.commandHost = commandHost ?? throw new ArgumentNullException(nameof(commandHost));
        this.approvalGrants = approvalGrants ?? throw new ArgumentNullException(nameof(approvalGrants));
        this.requestFactory = requestFactory ?? throw new ArgumentNullException(nameof(requestFactory));
    }

    public async ValueTask<MspCommandResult> ExecuteApprovedAsync(
        string commandText,
        string? actor = null,
        CancellationToken cancellationToken = default)
    {
        var resolvedActor = ResolveActor(actor);
        var approvalToken = approvalGrants.ApproveNextCommand(commandText, resolvedActor);
        try
        {
            return await commandHost.ExecuteAsync(
                requestFactory.Create(
                    commandText,
                    resolvedActor,
                    environment: approvalGrants.CreateApprovalEnvironment(approvalToken)),
                cancellationToken);
        }
        finally
        {
            approvalGrants.RevokeApproval(approvalToken);
        }
    }

    public async IAsyncEnumerable<MspCommandEvent> ExecuteApprovedStreamingAsync(
        string commandText,
        string? actor = null,
        [EnumeratorCancellation] CancellationToken cancellationToken = default)
    {
        var resolvedActor = ResolveActor(actor);
        var approvalToken = approvalGrants.ApproveNextCommand(commandText, resolvedActor);
        try
        {
            await foreach (var commandEvent in commandHost.ExecuteStreamingAsync(
                requestFactory.Create(
                    commandText,
                    resolvedActor,
                    environment: approvalGrants.CreateApprovalEnvironment(approvalToken)),
                cancellationToken))
            {
                yield return commandEvent;
            }
        }
        finally
        {
            approvalGrants.RevokeApproval(approvalToken);
        }
    }

    private string ResolveActor(string? actor)
    {
        return string.IsNullOrWhiteSpace(actor)
            ? requestFactory.DefaultActor
            : actor.Trim();
    }
}
