using ReadOS.App.Models;
using ReadOS.Msp.Audit;
using ReadOS.Msp.Commands;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Runtime;

namespace ReadOS.App.Services.Msp;

public sealed class ReadOsMspHost
{
    private readonly MspRuntime runtime;

    public ReadOsMspHost(
        IWorkspaceStore workspaceStore,
        IPdfDocumentService pdfService,
        Func<WorkspaceState?> workspaceProvider,
        Func<LibraryItem?> selectedDocumentProvider)
    {
        var registry = MspRuntime.CreateDefaultRegistry();
        var workspace = new ReadOsVirtualWorkspace(workspaceStore, pdfService, workspaceProvider);
        registry
            .Register(new ReadOsWorkspaceCommand(workspaceStore, workspaceProvider))
            .Register(new ReadOsLibraryCommand(workspaceProvider))
            .Register(new ReadOsPdfCommand(workspaceStore, pdfService, workspaceProvider, selectedDocumentProvider))
            .Register(new ReadOsWindowsCommand(workspaceStore, selectedDocumentProvider))
            .Register(new ReadOsPageLabelCommand(workspaceStore, workspaceProvider, selectedDocumentProvider))
            .Register(new ReadOsOutlineCommand(workspaceStore, workspaceProvider, selectedDocumentProvider));

        var context = new MspCommandContext(
            workspace,
            registry,
            new AllowAllMspPolicy(),
            new InMemoryMspAuditSink());
        runtime = new MspRuntime(context);
    }

    public ValueTask<MspCommandResult> ExecuteAsync(
        string commandText,
        string actor = "reados-agent",
        CancellationToken cancellationToken = default)
    {
        return runtime.ExecuteAsync(new MspCommandRequest
        {
            Actor = actor,
            CommandText = commandText
        }, cancellationToken);
    }
}
