using ReadOS.Msp.Hosting.Native.RuntimeFfi;
using ReadOS.Msp.Hosting.Policy;
using ReadOS.Msp.Hosting.Native;
using ReadOS.Msp.Hosting.Runtime;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsMspHostRuntime : IDisposable
{
    private readonly IMspNativeAdapterProvider? ownedNativeAdapterProvider;
    private readonly IDisposable? ownedRuntimeFfiEchoCommandAdapter;
    private int disposed;

    public ReadOsMspHostRuntime(
        IMspCommandHost commandHost,
        MspCommandRequestFactory requestFactory,
        IMspApprovalGrantStore approvalGrants,
        MspCommandHostComposition composition,
        MspCommandHostDiagnostics diagnostics,
        MspVerifiedRuntimeProviderCatalog runtimeProviderCatalog,
        IMspNativeAdapterProvider? ownedNativeAdapterProvider,
        IDisposable? ownedRuntimeFfiEchoCommandAdapter)
    {
        CommandHost = commandHost;
        RequestFactory = requestFactory;
        ApprovalGrants = approvalGrants;
        Composition = composition;
        Diagnostics = diagnostics;
        RuntimeProviderCatalog = runtimeProviderCatalog;
        this.ownedNativeAdapterProvider = ownedNativeAdapterProvider;
        this.ownedRuntimeFfiEchoCommandAdapter = ownedRuntimeFfiEchoCommandAdapter;
    }

    public IMspCommandHost CommandHost { get; }

    public MspCommandRequestFactory RequestFactory { get; }

    public IMspApprovalGrantStore ApprovalGrants { get; }

    public MspCommandHostComposition Composition { get; }

    public MspCommandHostDiagnostics Diagnostics { get; }

    public MspVerifiedRuntimeProviderCatalog RuntimeProviderCatalog { get; }

    public void Dispose()
    {
        if (Interlocked.Exchange(ref disposed, 1) != 0)
        {
            return;
        }

        try
        {
            ownedRuntimeFfiEchoCommandAdapter?.Dispose();
        }
        finally
        {
            ownedNativeAdapterProvider?.Dispose();
        }
    }
}

internal sealed class ReadOsMspHostRuntimeFactory
{
    public ReadOsMspHostRuntime Create(
        ReadOsMspHostDependencies dependencies,
        string defaultSessionId,
        string defaultActor,
        IMspNativeAdapterProvider? nativeAdapterProvider = null,
        bool ownsNativeAdapterProvider = false,
        MspCommandRuntimeFfiEchoCommandAdapter? runtimeFfiEchoCommandAdapter = null,
        bool ownsRuntimeFfiEchoCommandAdapter = false)
    {
        ArgumentNullException.ThrowIfNull(dependencies);

        var createdNativeAdapterProvider = nativeAdapterProvider is null;
        var resolvedNativeAdapterProvider = nativeAdapterProvider ??
            new LazyMspNativeAdapterProvider(
                () => MspNativeAdapter.LoadPackagedWindows());

        var workspace = new ReadOsVirtualWorkspace(
            dependencies.WorkspaceStore,
            dependencies.PdfService,
            dependencies.WorkspaceProvider);
        var commandPack = new ReadOsMspCommandPackFactory(
            dependencies.WorkspaceStore,
            dependencies.PdfService,
            dependencies.AiChatService,
            dependencies.WorkspaceProvider,
            dependencies.SettingsProvider,
            dependencies.SelectedDocumentProvider,
            dependencies.PendingAttachmentsProvider,
            dependencies.AttachmentTextProvider,
            dependencies.AttachmentSink,
            dependencies.ClearAttachments,
            dependencies.ChatResultSink).CreateCommandPack();
        var nativeCoreRegistryFactory = new ReadOsNativeCoreRegistryFactory(
            resolvedNativeAdapterProvider,
            runtimeFfiEchoCommandAdapter ?? dependencies.RuntimeFfiEchoCommandAdapter);
        ReadOsNativeCoreRegistryFactory.ValidateHostCommandPack(commandPack);
        var composition = new MspCommandHostCompositionBuilder(
            nativeCoreRegistryFactory.Create).Build(commandPack);
        var requestFactory = new MspCommandRequestFactory(defaultSessionId, defaultActor);
        var approvalGrants = new MspApprovalGrantStore();
        var policy = new ReadOsOperatorApprovalPolicy(dependencies.SettingsProvider, approvalGrants);
        var runtimeHost = new MspRuntimeHostFactory().Build(
            workspace,
            composition.Registry,
            policy);
        var diagnostics = new MspCommandHostDiagnosticsService().Build(
            composition,
            requestFactory);
        var runtimeProviderCatalog = dependencies.RuntimeProviderCatalog ??
            new MspVerifiedRuntimeProviderCatalog();

        return new ReadOsMspHostRuntime(
            runtimeHost.CommandHost,
            requestFactory,
            approvalGrants,
            composition,
            diagnostics,
            runtimeProviderCatalog,
            createdNativeAdapterProvider || ownsNativeAdapterProvider
                ? resolvedNativeAdapterProvider
                : null,
            ownsRuntimeFfiEchoCommandAdapter
                ? runtimeFfiEchoCommandAdapter ?? dependencies.RuntimeFfiEchoCommandAdapter
                : null);
    }
}
