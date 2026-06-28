using System.Text;
using System.Text.Json;
using ReadOS.App.Models;
using ReadOS.Msp.Models;
using ReadOS.Msp.Runtime;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsWorkspaceCommand : IMspCommand
{
    private readonly IWorkspaceStore workspaceStore;
    private readonly Func<WorkspaceState?> workspaceProvider;

    public ReadOsWorkspaceCommand(IWorkspaceStore workspaceStore, Func<WorkspaceState?> workspaceProvider)
    {
        this.workspaceStore = workspaceStore;
        this.workspaceProvider = workspaceProvider;
    }

    public string Name => "workspace";

    public string Summary => "Inspect the ReadOS workspace. Usage: workspace info";

    public ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default)
    {
        var subcommand = arguments.FirstOrDefault() ?? "info";
        if (!string.Equals(subcommand, "info", StringComparison.OrdinalIgnoreCase))
        {
            return ValueTask.FromResult(MspCommandResult.Failure("Usage: workspace info", exitCode: 2));
        }

        var workspace = workspaceProvider();
        if (workspace is null)
        {
            return ValueTask.FromResult(MspCommandResult.Failure("ReadOS workspace is not loaded."));
        }

        var documents = workspace.Projects.SelectMany(project => project.LibraryItems).Where(item => item.Kind != LibraryItemKind.Folder).ToArray();
        var builder = new StringBuilder();
        builder.AppendLine($"workspaceRoot\t{workspaceStore.WorkspaceRoot}");
        builder.AppendLine($"libraryRoot\t{workspaceStore.LibraryRoot}");
        builder.AppendLine($"projects\t{workspace.Projects.Count}");
        builder.AppendLine($"documents\t{documents.Length}");
        builder.AppendLine($"pdfs\t{documents.Count(item => item.Kind == LibraryItemKind.Pdf)}");
        return ValueTask.FromResult(MspCommandResult.Success(builder.ToString()));
    }
}

internal sealed class ReadOsLibraryCommand : IMspCommand
{
    private readonly Func<WorkspaceState?> workspaceProvider;

    public ReadOsLibraryCommand(Func<WorkspaceState?> workspaceProvider)
    {
        this.workspaceProvider = workspaceProvider;
    }

    public string Name => "library";

    public string Summary => "List ReadOS projects and documents. Usage: library list";

    public ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default)
    {
        var subcommand = arguments.FirstOrDefault() ?? "list";
        if (!string.Equals(subcommand, "list", StringComparison.OrdinalIgnoreCase))
        {
            return ValueTask.FromResult(MspCommandResult.Failure("Usage: library list", exitCode: 2));
        }

        var workspace = workspaceProvider();
        if (workspace is null)
        {
            return ValueTask.FromResult(MspCommandResult.Failure("ReadOS workspace is not loaded."));
        }

        var builder = new StringBuilder();
        builder.AppendLine("projectId\tdocumentId\tkind\tpages\tname");
        foreach (var project in workspace.Projects)
        {
            foreach (var document in project.LibraryItems.Where(item => item.Kind != LibraryItemKind.Folder).OrderBy(item => item.Order))
            {
                builder.Append(project.Id);
                builder.Append('\t');
                builder.Append(document.Id);
                builder.Append('\t');
                builder.Append(document.Kind);
                builder.Append('\t');
                builder.Append(document.PageCount);
                builder.Append('\t');
                builder.AppendLine(document.Name);
            }
        }

        return ValueTask.FromResult(MspCommandResult.Success(builder.ToString()));
    }
}

internal sealed class ReadOsPdfCommand : IMspCommand
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        WriteIndented = true
    };

    private readonly IWorkspaceStore workspaceStore;
    private readonly IPdfDocumentService pdfService;
    private readonly Func<WorkspaceState?> workspaceProvider;
    private readonly Func<LibraryItem?> selectedDocumentProvider;

    public ReadOsPdfCommand(
        IWorkspaceStore workspaceStore,
        IPdfDocumentService pdfService,
        Func<WorkspaceState?> workspaceProvider,
        Func<LibraryItem?> selectedDocumentProvider)
    {
        this.workspaceStore = workspaceStore;
        this.pdfService = pdfService;
        this.workspaceProvider = workspaceProvider;
        this.selectedDocumentProvider = selectedDocumentProvider;
    }

    public string Name => "pdf";

    public string Summary => "Inspect PDF documents. Usage: pdf inspect|text|search [current|documentId] ...";

    public async ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default)
    {
        if (arguments.Count == 0)
        {
            return MspCommandResult.Failure("Usage: pdf inspect|text|search [current|documentId] ...", exitCode: 2);
        }

        return arguments[0].ToLowerInvariant() switch
        {
            "inspect" => Inspect(arguments.Skip(1).ToArray()),
            "text" => await TextAsync(arguments.Skip(1).ToArray(), cancellationToken),
            "search" => await SearchAsync(arguments.Skip(1).ToArray(), cancellationToken),
            _ => MspCommandResult.Failure("Usage: pdf inspect|text|search [current|documentId] ...", exitCode: 2)
        };
    }

    private MspCommandResult Inspect(IReadOnlyList<string> arguments)
    {
        var document = ResolveDocument(arguments.Count > 0 ? arguments[0] : "current");
        if (document is null)
        {
            return MspCommandResult.Failure("PDF document not found.");
        }

        var payload = new
        {
            document.Id,
            document.ProjectId,
            document.Name,
            document.PageCount,
            document.CurrentPage,
            document.CurrentPageLabel,
            Labels = document.PageLabels.Select(label => new { label.PdfPage, label.Label }),
            Outline = document.Outline.Select(item => new { item.Id, item.Title, item.Page, item.Level })
        };
        return MspCommandResult.Success(JsonSerializer.Serialize(payload, JsonOptions) + Environment.NewLine);
    }

    private async ValueTask<MspCommandResult> TextAsync(IReadOnlyList<string> arguments, CancellationToken cancellationToken)
    {
        if (arguments.Count < 2)
        {
            return MspCommandResult.Failure("Usage: pdf text [current|documentId] <startPage> [endPage]", exitCode: 2);
        }

        var document = ResolveDocument(arguments[0]);
        if (document is null)
        {
            return MspCommandResult.Failure("PDF document not found.");
        }

        if (!int.TryParse(arguments[1], out var startPage))
        {
            return MspCommandResult.Failure("startPage must be a number.", exitCode: 2);
        }

        var endPage = startPage;
        if (arguments.Count > 2 && !int.TryParse(arguments[2], out endPage))
        {
            return MspCommandResult.Failure("endPage must be a number.", exitCode: 2);
        }

        var text = await pdfService.ExtractPageTextAsync(
            workspaceStore.GetAbsolutePath(document),
            startPage,
            endPage,
            cancellationToken);
        return MspCommandResult.Success(text + Environment.NewLine);
    }

    private async ValueTask<MspCommandResult> SearchAsync(IReadOnlyList<string> arguments, CancellationToken cancellationToken)
    {
        if (arguments.Count < 2)
        {
            return MspCommandResult.Failure("Usage: pdf search [current|documentId] <query>", exitCode: 2);
        }

        var document = ResolveDocument(arguments[0]);
        if (document is null)
        {
            return MspCommandResult.Failure("PDF document not found.");
        }

        var query = string.Join(' ', arguments.Skip(1));
        var hits = await pdfService.SearchAsync(workspaceStore.GetAbsolutePath(document), query, cancellationToken);
        var builder = new StringBuilder();
        foreach (var hit in hits)
        {
            builder.Append(hit.PageNumber);
            builder.Append('\t');
            builder.AppendLine(hit.Preview);
        }

        return MspCommandResult.Success(builder.ToString());
    }

    private LibraryItem? ResolveDocument(string value)
    {
        if (string.Equals(value, "current", StringComparison.OrdinalIgnoreCase))
        {
            var selected = selectedDocumentProvider();
            return selected?.Kind == LibraryItemKind.Pdf ? selected : null;
        }

        var workspace = workspaceProvider();
        if (workspace is null)
        {
            return null;
        }

        return workspace.Projects
            .SelectMany(project => project.LibraryItems)
            .Where(item => item.Kind == LibraryItemKind.Pdf)
            .FirstOrDefault(item =>
                string.Equals(item.Id, value, StringComparison.OrdinalIgnoreCase) ||
                string.Equals(item.Name, value, StringComparison.OrdinalIgnoreCase));
    }
}
