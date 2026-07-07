using ReadOS.Msp.Hosting.Policy;
using ReadOS.Msp.Hosting.Runtime;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsMspHostRuntime
{
    public ReadOsMspHostRuntime(
        IMspCommandHost commandHost,
        MspCommandRequestFactory requestFactory,
        IMspApprovalGrantStore approvalGrants,
        MspCommandHostComposition composition,
        MspCommandHostDiagnostics diagnostics)
    {
        CommandHost = commandHost;
        RequestFactory = requestFactory;
        ApprovalGrants = approvalGrants;
        Composition = composition;
        Diagnostics = diagnostics;
    }

    public IMspCommandHost CommandHost { get; }

    public MspCommandRequestFactory RequestFactory { get; }

    public IMspApprovalGrantStore ApprovalGrants { get; }

    public MspCommandHostComposition Composition { get; }

    public MspCommandHostDiagnostics Diagnostics { get; }
}

internal sealed class ReadOsMspHostRuntimeFactory
{
    public ReadOsMspHostRuntime Create(
        ReadOsMspHostDependencies dependencies,
        string defaultSessionId,
        string defaultActor)
    {
        ArgumentNullException.ThrowIfNull(dependencies);

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
        var composition = new MspCommandHostCompositionBuilder().Build(commandPack);
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

        return new ReadOsMspHostRuntime(
            runtimeHost.CommandHost,
            requestFactory,
            approvalGrants,
            composition,
            diagnostics);
    }
}
