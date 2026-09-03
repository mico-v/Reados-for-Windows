using System.Runtime.CompilerServices;
using ReadOS.Msp.Hosting.Native;
using ReadOS.Msp.Hosting.Native.RuntimeFfi;
using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Models;

namespace ReadOS.App.Services.Msp;

public sealed class ReadOsMspHost : IMspCommandHost, IDisposable
{
    public const string DefaultSessionId = "reados-workbench";

    private readonly IMspCommandHost commandHost;
    private readonly MspCommandHostFacade commandFacade;
    private readonly ReadOsMspHostRuntime runtimeHost;

    public ReadOsMspHost(
        ReadOsMspHostDependencies dependencies,
        MspCommandRuntimeFfiEchoCommandAdapter? runtimeFfiEchoCommandAdapter = null,
        bool ownsRuntimeFfiEchoCommandAdapter = false)
        : this(
            dependencies,
            nativeAdapterProvider: null,
            runtimeFfiEchoCommandAdapter,
            ownsRuntimeFfiEchoCommandAdapter)
    {
    }

    internal ReadOsMspHost(
        ReadOsMspHostDependencies dependencies,
        IMspNativeAdapterProvider? nativeAdapterProvider,
        MspCommandRuntimeFfiEchoCommandAdapter? runtimeFfiEchoCommandAdapter = null,
        bool ownsRuntimeFfiEchoCommandAdapter = false)
    {
        ArgumentNullException.ThrowIfNull(dependencies);

        runtimeHost = new ReadOsMspHostRuntimeFactory().Create(
            dependencies,
            DefaultSessionId,
            "reados-agent",
            nativeAdapterProvider,
            runtimeFfiEchoCommandAdapter: runtimeFfiEchoCommandAdapter,
            ownsRuntimeFfiEchoCommandAdapter: ownsRuntimeFfiEchoCommandAdapter);
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

    /// <summary>
    /// Host-owned verified provider registry. It only prepares bounded launch
    /// plans; policy, approval, backend execution, and product audit remain in
    /// their existing owners.
    /// </summary>
    public MspVerifiedRuntimeProviderCatalog RuntimeProviderCatalog =>
        runtimeHost.RuntimeProviderCatalog;

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
