using System.Runtime.CompilerServices;
using ReadOS.Msp.Hosting.Native;
using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Models;

namespace ReadOS.App.Services.Msp;

public sealed class ReadOsMspHost : IMspCommandHost, IDisposable
{
    public const string DefaultSessionId = "reados-workbench";

    private readonly IMspCommandHost commandHost;
    private readonly MspCommandHostFacade commandFacade;
    private readonly ReadOsMspHostRuntime runtimeHost;

    public ReadOsMspHost(ReadOsMspHostDependencies dependencies)
        : this(dependencies, nativeAdapterProvider: null)
    {
    }

    internal ReadOsMspHost(
        ReadOsMspHostDependencies dependencies,
        IMspNativeAdapterProvider? nativeAdapterProvider)
    {
        ArgumentNullException.ThrowIfNull(dependencies);

        runtimeHost = new ReadOsMspHostRuntimeFactory().Create(
            dependencies,
            DefaultSessionId,
            "reados-agent",
            nativeAdapterProvider);
        commandHost = runtimeHost.CommandHost;
        commandFacade = new MspCommandHostFacade(
            commandHost,
            runtimeHost.RequestFactory,
            runtimeHost.ApprovalGrants);
    }

    public ValueTask<MspCommandResult> ExecuteAsync(
        string commandText,
        string actor = "reados-agent",
        CancellationToken cancellationToken = default)
    {
        return commandFacade.ExecuteAsync(commandText, actor, cancellationToken);
    }

    public ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandRequest request,
        CancellationToken cancellationToken = default)
    {
        return commandHost.ExecuteAsync(request, cancellationToken);
    }

    public IAsyncEnumerable<MspCommandEvent> ExecuteStreamingAsync(
        string commandText,
        string actor = "reados-agent",
        CancellationToken cancellationToken = default)
    {
        return commandFacade.ExecuteStreamingAsync(commandText, actor, cancellationToken);
    }

    public IAsyncEnumerable<MspCommandEvent> ExecuteStreamingAsync(
        MspCommandRequest request,
        CancellationToken cancellationToken = default)
    {
        return commandHost.ExecuteStreamingAsync(request, cancellationToken);
    }

    public async ValueTask<MspCommandResult> ExecuteApprovedAsync(
        string commandText,
        string actor = "reados-agent",
        CancellationToken cancellationToken = default)
    {
        return await commandFacade.ExecuteApprovedAsync(
            commandText,
            actor,
            cancellationToken);
    }

    public async IAsyncEnumerable<MspCommandEvent> ExecuteApprovedStreamingAsync(
        string commandText,
        string actor = "reados-agent",
        [EnumeratorCancellation] CancellationToken cancellationToken = default)
    {
        await foreach (var commandEvent in commandFacade.ExecuteApprovedStreamingAsync(
            commandText,
            actor,
            cancellationToken))
        {
            yield return commandEvent;
        }
    }

    public void Dispose()
    {
        runtimeHost.Dispose();
    }
}
