using System.Runtime.InteropServices;
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

    public MspCommandMetadata Metadata => MspCommandMetadata.Create(
        Name,
        Summary,
        "workspace info",
        MspCommandEffects.ReadWorkspace,
        new[] { "reados.workspace.read" });

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

    public MspCommandMetadata Metadata => MspCommandMetadata.Create(
        Name,
        Summary,
        "library list",
        MspCommandEffects.ReadWorkspace,
        new[] { "reados.library.read" });

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

    public MspCommandMetadata Metadata => MspCommandMetadata.Create(
        Name,
        Summary,
        "pdf inspect|text|search [current|documentId] ...",
        MspCommandEffects.ReadWorkspace,
        new[] { "reados.pdf.read" });

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

internal sealed class ReadOsWindowsCommand : IMspCommand
{
    private readonly IWorkspaceStore workspaceStore;
    private readonly Func<LibraryItem?> selectedDocumentProvider;

    public ReadOsWindowsCommand(
        IWorkspaceStore workspaceStore,
        Func<LibraryItem?> selectedDocumentProvider)
    {
        this.workspaceStore = workspaceStore;
        this.selectedDocumentProvider = selectedDocumentProvider;
    }

    public string Name => "windows";

    public string Summary => "Inspect safe Windows host context exposed by ReadOS. Usage: windows info|path [workspace|library|current]";

    public MspCommandMetadata Metadata => MspCommandMetadata.Create(
        Name,
        Summary,
        "windows info|path [workspace|library|current]",
        MspCommandEffects.ReadWorkspace,
        new[] { "reados.windows.read" });

    public ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default)
    {
        var subcommand = arguments.FirstOrDefault() ?? "info";
        return ValueTask.FromResult(subcommand.ToLowerInvariant() switch
        {
            "info" => Info(),
            "path" => PathInfo(arguments.Skip(1).FirstOrDefault() ?? "workspace"),
            _ => MspCommandResult.Failure("Usage: windows info|path [workspace|library|current]", exitCode: 2)
        });
    }

    private static MspCommandResult Info()
    {
        var builder = new StringBuilder();
        builder.AppendLine($"os\t{RuntimeInformation.OSDescription}");
        builder.AppendLine($"architecture\t{RuntimeInformation.OSArchitecture}");
        builder.AppendLine($"framework\t{RuntimeInformation.FrameworkDescription}");
        builder.AppendLine($"processArchitecture\t{RuntimeInformation.ProcessArchitecture}");
        return MspCommandResult.Success(builder.ToString());
    }

    private MspCommandResult PathInfo(string target)
    {
        var normalized = target.ToLowerInvariant();
        var path = normalized switch
        {
            "workspace" => workspaceStore.WorkspaceRoot,
            "library" => workspaceStore.LibraryRoot,
            "current" => CurrentDocumentPath(),
            _ => null
        };

        return path is null
            ? MspCommandResult.Failure("Usage: windows path [workspace|library|current]", exitCode: 2)
            : MspCommandResult.Success(path + Environment.NewLine);
    }

    private string? CurrentDocumentPath()
    {
        var selected = selectedDocumentProvider();
        return selected is null ? null : workspaceStore.GetAbsolutePath(selected);
    }
}

internal sealed class ReadOsPageLabelCommand : IMspCommand
{
    private readonly IWorkspaceStore workspaceStore;
    private readonly Func<WorkspaceState?> workspaceProvider;
    private readonly Func<LibraryItem?> selectedDocumentProvider;

    public ReadOsPageLabelCommand(
        IWorkspaceStore workspaceStore,
        Func<WorkspaceState?> workspaceProvider,
        Func<LibraryItem?> selectedDocumentProvider)
    {
        this.workspaceStore = workspaceStore;
        this.workspaceProvider = workspaceProvider;
        this.selectedDocumentProvider = selectedDocumentProvider;
    }

    public string Name => "page-label";

    public string Summary => "Set a ReadOS PDF page label. Usage: page-label set [current|documentId] <page> <label>";

    public MspCommandMetadata Metadata => MspCommandMetadata.Create(
        Name,
        Summary,
        "page-label set [current|documentId] <page> <label>",
        MspCommandEffects.ReadWorkspace | MspCommandEffects.WriteWorkspace,
        new[] { "reados.pdf.write" });

    public async ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default)
    {
        if (arguments.Count < 4 || !string.Equals(arguments[0], "set", StringComparison.OrdinalIgnoreCase))
        {
            return MspCommandResult.Failure("Usage: page-label set [current|documentId] <page> <label>", exitCode: 2);
        }

        var workspace = workspaceProvider();
        if (workspace is null)
        {
            return MspCommandResult.Failure("ReadOS workspace is not loaded.");
        }

        var document = ReadOsMspCommandHelpers.ResolvePdfDocument(workspace, arguments[1], selectedDocumentProvider);
        if (document is null)
        {
            return MspCommandResult.Failure("PDF document not found.");
        }

        if (!int.TryParse(arguments[2], out var page) || page < 1 || page > document.PageCount)
        {
            return MspCommandResult.Failure($"page must be between 1 and {document.PageCount}.", exitCode: 2);
        }

        var labelText = string.Join(' ', arguments.Skip(3)).Trim();
        if (string.IsNullOrWhiteSpace(labelText))
        {
            return MspCommandResult.Failure("label must not be empty.", exitCode: 2);
        }

        var label = document.PageLabels.FirstOrDefault(item => item.PdfPage == page);
        if (label is null)
        {
            label = new PageLabelRule { PdfPage = page };
            document.PageLabels.Add(label);
        }

        label.Label = labelText;
        document.UpdatedAt = DateTimeOffset.Now;
        await workspaceStore.SaveAsync(workspace, cancellationToken);
        return MspCommandResult.Success($"page-label\t{document.Id}\t{page}\t{labelText}{Environment.NewLine}");
    }
}

internal sealed class ReadOsOutlineCommand : IMspCommand
{
    private readonly IWorkspaceStore workspaceStore;
    private readonly Func<WorkspaceState?> workspaceProvider;
    private readonly Func<LibraryItem?> selectedDocumentProvider;

    public ReadOsOutlineCommand(
        IWorkspaceStore workspaceStore,
        Func<WorkspaceState?> workspaceProvider,
        Func<LibraryItem?> selectedDocumentProvider)
    {
        this.workspaceStore = workspaceStore;
        this.workspaceProvider = workspaceProvider;
        this.selectedDocumentProvider = selectedDocumentProvider;
    }

    public string Name => "outline";

    public string Summary => "Add or delete ReadOS PDF outline items.";

    public MspCommandMetadata Metadata => MspCommandMetadata.Create(
        Name,
        Summary,
        "outline add [current|documentId] <page> <title> [--level N] | outline delete [current|documentId] <id|title>",
        MspCommandEffects.ReadWorkspace | MspCommandEffects.WriteWorkspace,
        new[] { "reados.pdf.write" });

    public async ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default)
    {
        if (arguments.Count == 0)
        {
            return MspCommandResult.Failure(Usage(), exitCode: 2);
        }

        return arguments[0].ToLowerInvariant() switch
        {
            "add" => await AddAsync(arguments.Skip(1).ToArray(), cancellationToken),
            "delete" => await DeleteAsync(arguments.Skip(1).ToArray(), cancellationToken),
            _ => MspCommandResult.Failure(Usage(), exitCode: 2)
        };
    }

    private async ValueTask<MspCommandResult> AddAsync(IReadOnlyList<string> arguments, CancellationToken cancellationToken)
    {
        if (arguments.Count < 3)
        {
            return MspCommandResult.Failure("Usage: outline add [current|documentId] <page> <title> [--level N]", exitCode: 2);
        }

        var workspace = workspaceProvider();
        if (workspace is null)
        {
            return MspCommandResult.Failure("ReadOS workspace is not loaded.");
        }

        var document = ReadOsMspCommandHelpers.ResolvePdfDocument(workspace, arguments[0], selectedDocumentProvider);
        if (document is null)
        {
            return MspCommandResult.Failure("PDF document not found.");
        }

        if (!int.TryParse(arguments[1], out var page) || page < 1 || page > document.PageCount)
        {
            return MspCommandResult.Failure($"page must be between 1 and {document.PageCount}.", exitCode: 2);
        }

        var titleParts = new List<string>();
        var level = 1;
        for (var index = 2; index < arguments.Count; index++)
        {
            if (string.Equals(arguments[index], "--level", StringComparison.OrdinalIgnoreCase))
            {
                if (index + 1 >= arguments.Count || !int.TryParse(arguments[index + 1], out level))
                {
                    return MspCommandResult.Failure("--level requires a number.", exitCode: 2);
                }

                index++;
                continue;
            }

            titleParts.Add(arguments[index]);
        }

        var title = string.Join(' ', titleParts).Trim();
        if (string.IsNullOrWhiteSpace(title))
        {
            return MspCommandResult.Failure("title must not be empty.", exitCode: 2);
        }

        var item = new OutlineItem
        {
            Title = title,
            Page = page,
            Level = Math.Clamp(level, 1, 6)
        };
        document.Outline.Add(item);
        document.UpdatedAt = DateTimeOffset.Now;
        await workspaceStore.SaveAsync(workspace, cancellationToken);
        return MspCommandResult.Success($"outline-added\t{document.Id}\t{item.Id}\t{page}\t{item.Level}\t{title}{Environment.NewLine}");
    }

    private async ValueTask<MspCommandResult> DeleteAsync(IReadOnlyList<string> arguments, CancellationToken cancellationToken)
    {
        if (arguments.Count < 2)
        {
            return MspCommandResult.Failure("Usage: outline delete [current|documentId] <id|title>", exitCode: 2);
        }

        var workspace = workspaceProvider();
        if (workspace is null)
        {
            return MspCommandResult.Failure("ReadOS workspace is not loaded.");
        }

        var document = ReadOsMspCommandHelpers.ResolvePdfDocument(workspace, arguments[0], selectedDocumentProvider);
        if (document is null)
        {
            return MspCommandResult.Failure("PDF document not found.");
        }

        var selector = string.Join(' ', arguments.Skip(1)).Trim();
        var item = document.Outline.FirstOrDefault(item =>
            string.Equals(item.Id, selector, StringComparison.OrdinalIgnoreCase) ||
            string.Equals(item.Title, selector, StringComparison.OrdinalIgnoreCase));
        if (item is null)
        {
            return MspCommandResult.Failure($"Outline item not found: {selector}");
        }

        document.Outline.Remove(item);
        document.UpdatedAt = DateTimeOffset.Now;
        await workspaceStore.SaveAsync(workspace, cancellationToken);
        return MspCommandResult.Success($"outline-deleted\t{document.Id}\t{item.Id}\t{item.Title}{Environment.NewLine}");
    }

    private static string Usage()
    {
        return "Usage: outline add [current|documentId] <page> <title> [--level N] | outline delete [current|documentId] <id|title>";
    }
}

internal sealed class ReadOsAttachCommand : IMspCommand
{
    private readonly Func<WorkspaceState?> workspaceProvider;
    private readonly Func<LibraryItem?> selectedDocumentProvider;
    private readonly Action<ChatAttachment> attachmentSink;

    public ReadOsAttachCommand(
        Func<WorkspaceState?> workspaceProvider,
        Func<LibraryItem?> selectedDocumentProvider,
        Action<ChatAttachment> attachmentSink)
    {
        this.workspaceProvider = workspaceProvider;
        this.selectedDocumentProvider = selectedDocumentProvider;
        this.attachmentSink = attachmentSink;
    }

    public string Name => "attach";

    public string Summary => "Queue ReadOS evidence attachments for the current chat.";

    public MspCommandMetadata Metadata => MspCommandMetadata.Create(
        Name,
        Summary,
        "attach page [current|documentId] <page> | attach range [current|documentId] <startPage> <endPage>",
        MspCommandEffects.ReadWorkspace | MspCommandEffects.WriteWorkspace,
        new[] { "reados.attach.write" });

    public ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default)
    {
        if (arguments.Count == 0)
        {
            return ValueTask.FromResult(MspCommandResult.Failure(Usage(), exitCode: 2));
        }

        return ValueTask.FromResult(arguments[0].ToLowerInvariant() switch
        {
            "page" => AttachPage(arguments.Skip(1).ToArray()),
            "range" => AttachRange(arguments.Skip(1).ToArray()),
            _ => MspCommandResult.Failure(Usage(), exitCode: 2)
        });
    }

    private MspCommandResult AttachPage(IReadOnlyList<string> arguments)
    {
        if (arguments.Count < 2)
        {
            return MspCommandResult.Failure("Usage: attach page [current|documentId] <page>", exitCode: 2);
        }

        var resolved = ResolvePdfDocument(arguments[0]);
        if (resolved is null)
        {
            return MspCommandResult.Failure("PDF document not found.");
        }

        if (!TryParsePage(arguments[1], resolved, out var page, out var error))
        {
            return MspCommandResult.Failure(error, exitCode: 2);
        }

        QueueAttachment(resolved, page, page);
        return MspCommandResult.Success($"attached-page\t{resolved.Id}\t{page}{Environment.NewLine}");
    }

    private MspCommandResult AttachRange(IReadOnlyList<string> arguments)
    {
        if (arguments.Count < 3)
        {
            return MspCommandResult.Failure("Usage: attach range [current|documentId] <startPage> <endPage>", exitCode: 2);
        }

        var resolved = ResolvePdfDocument(arguments[0]);
        if (resolved is null)
        {
            return MspCommandResult.Failure("PDF document not found.");
        }

        if (!TryParsePage(arguments[1], resolved, out var startPage, out var startError))
        {
            return MspCommandResult.Failure(startError, exitCode: 2);
        }

        if (!TryParsePage(arguments[2], resolved, out var endPage, out var endError))
        {
            return MspCommandResult.Failure(endError, exitCode: 2);
        }

        var start = Math.Min(startPage, endPage);
        var end = Math.Max(startPage, endPage);
        QueueAttachment(resolved, start, end);
        return MspCommandResult.Success($"attached-range\t{resolved.Id}\t{start}\t{end}{Environment.NewLine}");
    }

    private LibraryItem? ResolvePdfDocument(string value)
    {
        var workspace = workspaceProvider();
        return workspace is null
            ? null
            : ReadOsMspCommandHelpers.ResolvePdfDocument(workspace, value, selectedDocumentProvider);
    }

    private void QueueAttachment(LibraryItem document, int startPage, int endPage)
    {
        attachmentSink(new ChatAttachment
        {
            Kind = startPage == endPage ? AttachmentKind.Page : AttachmentKind.PageRange,
            DocumentId = document.Id,
            Title = startPage == endPage
                ? $"{document.Name} · 第 {startPage} 页"
                : $"{document.Name} · 第 {startPage}-{endPage} 页",
            StartPage = startPage,
            EndPage = endPage
        });
    }

    private static bool TryParsePage(string value, LibraryItem document, out int page, out string error)
    {
        if (!int.TryParse(value, out page))
        {
            error = "page must be a number.";
            return false;
        }

        if (page < 1 || page > document.PageCount)
        {
            error = $"page must be between 1 and {document.PageCount}.";
            return false;
        }

        error = string.Empty;
        return true;
    }

    private static string Usage()
    {
        return "Usage: attach page [current|documentId] <page> | attach range [current|documentId] <startPage> <endPage>";
    }
}

internal static class ReadOsMspCommandHelpers
{
    public static LibraryItem? ResolvePdfDocument(
        WorkspaceState workspace,
        string value,
        Func<LibraryItem?> selectedDocumentProvider)
    {
        if (string.Equals(value, "current", StringComparison.OrdinalIgnoreCase))
        {
            var selected = selectedDocumentProvider();
            return selected?.Kind == LibraryItemKind.Pdf ? selected : null;
        }

        return workspace.Projects
            .SelectMany(project => project.LibraryItems)
            .Where(item => item.Kind == LibraryItemKind.Pdf)
            .FirstOrDefault(item =>
                string.Equals(item.Id, value, StringComparison.OrdinalIgnoreCase) ||
                string.Equals(item.Name, value, StringComparison.OrdinalIgnoreCase));
    }
}
