using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using ReadOS.App.Models;
using ReadOS.Msp.Commands;
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

    public MspCommandMetadata GetMetadata(IReadOnlyList<string> arguments)
    {
        if (arguments.Count > 0 &&
            HasArtifactOutput(arguments.Skip(1)))
        {
            return arguments[0].ToLowerInvariant() switch
            {
                "text" => MspCommandMetadata.Create(
                    Name,
                    "Extract PDF text into a durable artifact.",
                    "pdf text [current|documentId] <startPage> [endPage] --artifact <path>",
                    MspCommandEffects.ReadWorkspace | MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact,
                    new[] { "reados.pdf.read", "msp.artifact.write" }),
                "search" => MspCommandMetadata.Create(
                    Name,
                    "Write PDF search results into a durable artifact.",
                    "pdf search [current|documentId] <query> --artifact <path>",
                    MspCommandEffects.ReadWorkspace | MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact,
                    new[] { "reados.pdf.read", "msp.artifact.write" }),
                _ => Metadata
            };
        }

        return Metadata;
    }

    public MspCommandPreview GetPreview(IReadOnlyList<string> arguments)
    {
        if (arguments.Count == 0)
        {
            return MspCommandPreview.Empty;
        }

        if (string.Equals(arguments[0], "text", StringComparison.OrdinalIgnoreCase))
        {
            return PreviewTextArtifact(arguments.Skip(1).ToArray());
        }

        if (string.Equals(arguments[0], "search", StringComparison.OrdinalIgnoreCase))
        {
            return PreviewSearchArtifact(arguments.Skip(1).ToArray());
        }

        return MspCommandPreview.Empty;
    }

    private MspCommandPreview PreviewTextArtifact(IReadOnlyList<string> arguments)
    {
        if (!TryParsePdfTextArguments(arguments, out var parsed, out _) ||
            string.IsNullOrWhiteSpace(parsed.ArtifactPath))
        {
            return MspCommandPreview.Empty;
        }

        var document = ResolveDocument(parsed.DocumentSelector);
        var targets = new[]
        {
            NormalizeArtifactPath(parsed.ArtifactPath),
            document?.Id ?? parsed.DocumentSelector
        };
        var details = new[]
        {
            $"pages: {parsed.StartPage}-{parsed.EndPage}",
            document is null ? "document: unresolved until execution" : $"document: {document.Name}"
        };
        return MspCommandPreview.Create(
            "Extract PDF text into a durable artifact with source page provenance.",
            targets,
            details);
    }

    private MspCommandPreview PreviewSearchArtifact(IReadOnlyList<string> arguments)
    {
        if (!TryParsePdfSearchArguments(arguments, out var parsed, out _) ||
            string.IsNullOrWhiteSpace(parsed.ArtifactPath))
        {
            return MspCommandPreview.Empty;
        }

        var document = ResolveDocument(parsed.DocumentSelector);
        var targets = new[]
        {
            NormalizeArtifactPath(parsed.ArtifactPath),
            document?.Id ?? parsed.DocumentSelector
        };
        var details = new[]
        {
            $"query: {parsed.Query}",
            document is null ? "document: unresolved until execution" : $"document: {document.Name}"
        };
        return MspCommandPreview.Create(
            "Write PDF search results into a durable artifact with source page provenance.",
            targets,
            details);
    }

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
            "text" => await TextAsync(context, arguments.Skip(1).ToArray(), cancellationToken),
            "search" => await SearchAsync(context, arguments.Skip(1).ToArray(), cancellationToken),
            _ => MspCommandResult.Failure("Usage: pdf inspect|text|search [current|documentId] ...", exitCode: 2)
        };
    }

    private MspCommandResult Inspect(IReadOnlyList<string> arguments)
    {
        var selector = arguments.Count > 0 ? arguments[0] : "current";
        var document = ResolveDocument(selector);
        if (document is null)
        {
            return ReadOsMspCommandHelpers.PdfDocumentNotFound(selector);
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

    private async ValueTask<MspCommandResult> TextAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken)
    {
        if (!TryParsePdfTextArguments(arguments, out var parsed, out var parseError))
        {
            return MspCommandResult.Failure(parseError, exitCode: 2);
        }

        var document = ResolveDocument(parsed.DocumentSelector);
        if (document is null)
        {
            return ReadOsMspCommandHelpers.PdfDocumentNotFound(parsed.DocumentSelector);
        }

        if (!ReadOsMspCommandHelpers.TryValidatePdfPage(document, parsed.StartPage.ToString(), out _, out var startPageFailure))
        {
            return startPageFailure;
        }

        if (!ReadOsMspCommandHelpers.TryValidatePdfPage(document, parsed.EndPage.ToString(), out _, out var endPageFailure))
        {
            return endPageFailure;
        }

        await context.ReportProgressAsync(
            $"Extracting PDF text from {document.Name}, pages {parsed.StartPage}-{parsed.EndPage}.",
            20,
            cancellationToken);
        var text = await pdfService.ExtractPageTextAsync(
            workspaceStore.GetAbsolutePath(document),
            parsed.StartPage,
            parsed.EndPage,
            cancellationToken);
        await context.ReportProgressAsync(
            $"Extracted {text.Length} characters from {document.Name}.",
            90,
            cancellationToken);
        if (!string.IsNullOrWhiteSpace(parsed.ArtifactPath))
        {
            var artifactPath = NormalizeArtifactPath(parsed.ArtifactPath);
            if (!IsArtifactFilePath(artifactPath))
            {
                return MspCommandResult.Failure("pdf text --artifact must target a file under /artifacts.", exitCode: 2);
            }

            var sourcePages = GetSourcePages(parsed.StartPage, parsed.EndPage).ToArray();
            var artifact = new MspArtifact
            {
                Path = artifactPath,
                MediaType = "text/plain",
                SizeBytes = text.Length,
                Description = $"PDF text extracted from {document.Name}, pages {parsed.StartPage}-{parsed.EndPage}.",
                SourceCommand = context.Invocation.CommandText,
                Actor = context.Invocation.Actor,
                SessionId = context.Invocation.SessionId,
                SourcePaths = sourcePages
                    .Select(page => $"/documents/{document.Id}/pages/{page}.txt")
                    .ToArray(),
                SourceDocuments = new[] { document.Id },
                SourcePages = new[] { $"{document.Id}:{parsed.StartPage}-{parsed.EndPage}" },
                Preview = $"document: {document.Name}; pages: {parsed.StartPage}-{parsed.EndPage}; contentLength: {text.Length}"
            };
            await context.Workspace.WriteTextAsync(artifactPath, text, artifact, cancellationToken);
            return MspCommandResult.Success(
                $"artifact\t{artifactPath}\t{text.Length}{Environment.NewLine}",
                new[] { artifact });
        }

        return MspCommandResult.Success(text + Environment.NewLine);
    }

    private async ValueTask<MspCommandResult> SearchAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken)
    {
        if (!TryParsePdfSearchArguments(arguments, out var parsed, out var parseError))
        {
            return MspCommandResult.Failure(parseError, exitCode: 2);
        }

        var document = ResolveDocument(parsed.DocumentSelector);
        if (document is null)
        {
            return ReadOsMspCommandHelpers.PdfDocumentNotFound(parsed.DocumentSelector);
        }

        await context.ReportProgressAsync(
            $"Searching {document.Name} for \"{parsed.Query}\".",
            20,
            cancellationToken);
        var hits = await pdfService.SearchAsync(workspaceStore.GetAbsolutePath(document), parsed.Query, cancellationToken);
        await context.ReportProgressAsync(
            $"Found {hits.Count} matching PDF pages.",
            90,
            cancellationToken);
        var builder = new StringBuilder();
        foreach (var hit in hits)
        {
            builder.Append(hit.PageNumber);
            builder.Append('\t');
            builder.AppendLine(hit.Preview);
        }

        var content = builder.ToString();
        if (!string.IsNullOrWhiteSpace(parsed.ArtifactPath))
        {
            var artifactPath = NormalizeArtifactPath(parsed.ArtifactPath);
            if (!IsArtifactFilePath(artifactPath))
            {
                return MspCommandResult.Failure("pdf search --artifact must target a file under /artifacts.", exitCode: 2);
            }

            var sourcePages = hits
                .Select(hit => hit.PageNumber)
                .Distinct()
                .OrderBy(page => page)
                .ToArray();
            var artifact = new MspArtifact
            {
                Path = artifactPath,
                MediaType = "text/tab-separated-values",
                SizeBytes = content.Length,
                Description = $"PDF search results for \"{parsed.Query}\" in {document.Name}.",
                SourceCommand = context.Invocation.CommandText,
                Actor = context.Invocation.Actor,
                SessionId = context.Invocation.SessionId,
                SourcePaths = sourcePages
                    .Select(page => $"/documents/{document.Id}/pages/{page}.txt")
                    .ToArray(),
                SourceDocuments = new[] { document.Id },
                SourcePages = sourcePages
                    .Select(page => $"{document.Id}:{page}")
                    .ToArray(),
                Preview = $"document: {document.Name}; query: {parsed.Query}; hits: {hits.Count}; contentLength: {content.Length}"
            };
            await context.Workspace.WriteTextAsync(artifactPath, content, artifact, cancellationToken);
            return MspCommandResult.Success(
                $"artifact\t{artifactPath}\t{hits.Count}{Environment.NewLine}",
                new[] { artifact });
        }

        return MspCommandResult.Success(content);
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

    private static bool HasArtifactOutput(IEnumerable<string> arguments)
    {
        return arguments.Any(argument =>
            string.Equals(argument, "--artifact", StringComparison.OrdinalIgnoreCase) ||
            argument.StartsWith("--artifact=", StringComparison.OrdinalIgnoreCase));
    }

    private static bool TryParsePdfSearchArguments(
        IReadOnlyList<string> arguments,
        out PdfSearchArguments parsed,
        out string error)
    {
        parsed = new PdfSearchArguments(string.Empty, string.Empty, null);
        error = string.Empty;
        if (arguments.Count < 2)
        {
            error = "Usage: pdf search [current|documentId] <query> [--artifact <path>]";
            return false;
        }

        var queryParts = new List<string>();
        string? artifactPath = null;
        var index = 1;
        while (index < arguments.Count)
        {
            var argument = arguments[index];
            if (string.Equals(argument, "--artifact", StringComparison.OrdinalIgnoreCase))
            {
                if (index + 1 >= arguments.Count || string.IsNullOrWhiteSpace(arguments[index + 1]))
                {
                    error = "--artifact requires a path.";
                    return false;
                }

                artifactPath = arguments[index + 1];
                index += 2;
                continue;
            }

            if (argument.StartsWith("--artifact=", StringComparison.OrdinalIgnoreCase))
            {
                artifactPath = argument["--artifact=".Length..];
                if (string.IsNullOrWhiteSpace(artifactPath))
                {
                    error = "--artifact requires a path.";
                    return false;
                }

                index++;
                continue;
            }

            queryParts.Add(argument);
            index++;
        }

        var query = string.Join(' ', queryParts).Trim();
        if (string.IsNullOrWhiteSpace(query))
        {
            error = "query must not be empty.";
            return false;
        }

        parsed = new PdfSearchArguments(arguments[0], query, artifactPath);
        return true;
    }

    private static bool TryParsePdfTextArguments(
        IReadOnlyList<string> arguments,
        out PdfTextArguments parsed,
        out string error)
    {
        parsed = new PdfTextArguments(string.Empty, 0, 0, null);
        error = string.Empty;
        if (arguments.Count < 2)
        {
            error = "Usage: pdf text [current|documentId] <startPage> [endPage] [--artifact <path>]";
            return false;
        }

        if (!int.TryParse(arguments[1], out var startPage))
        {
            error = "startPage must be a number.";
            return false;
        }

        var endPage = startPage;
        string? artifactPath = null;
        var index = 2;
        if (index < arguments.Count && !arguments[index].StartsWith("--", StringComparison.Ordinal))
        {
            if (!int.TryParse(arguments[index], out endPage))
            {
                error = "endPage must be a number.";
                return false;
            }

            index++;
        }

        while (index < arguments.Count)
        {
            var argument = arguments[index];
            if (string.Equals(argument, "--artifact", StringComparison.OrdinalIgnoreCase))
            {
                if (index + 1 >= arguments.Count || string.IsNullOrWhiteSpace(arguments[index + 1]))
                {
                    error = "--artifact requires a path.";
                    return false;
                }

                artifactPath = arguments[index + 1];
                index += 2;
                continue;
            }

            if (argument.StartsWith("--artifact=", StringComparison.OrdinalIgnoreCase))
            {
                artifactPath = argument["--artifact=".Length..];
                if (string.IsNullOrWhiteSpace(artifactPath))
                {
                    error = "--artifact requires a path.";
                    return false;
                }

                index++;
                continue;
            }

            error = $"Unknown pdf text option: {argument}";
            return false;
        }

        parsed = new PdfTextArguments(arguments[0], startPage, endPage, artifactPath);
        return true;
    }

    private static string NormalizeArtifactPath(string path)
    {
        if (path.StartsWith("/artifacts", StringComparison.Ordinal))
        {
            return path;
        }

        return "/artifacts/" + path.TrimStart('/');
    }

    private static bool IsArtifactFilePath(string path)
    {
        return path.StartsWith("/artifacts/", StringComparison.Ordinal);
    }

    private static IEnumerable<int> GetSourcePages(int startPage, int endPage)
    {
        var start = Math.Min(startPage, endPage);
        var end = Math.Max(startPage, endPage);
        return Enumerable.Range(start, end - start + 1);
    }

    private sealed record PdfTextArguments(
        string DocumentSelector,
        int StartPage,
        int EndPage,
        string? ArtifactPath);

    private sealed record PdfSearchArguments(
        string DocumentSelector,
        string Query,
        string? ArtifactPath);
}

internal sealed class ReadOsWorkflowCommand : IMspCommand
{
    private const string ExplainSectionWorkflowName = "explain-section";
    private const string ExtractEvidenceWorkflowName = "extract-evidence";
    private const string ReviewEvidenceWorkflowName = "review-evidence";
    private const string SynthesizeEvidenceWorkflowName = "synthesize-evidence";
    private const string RefineArtifactWorkflowName = "refine-artifact";
    private const string UsageText = "workflow summary <session-id|current> [--artifact <path>] | workflow run summarize-current|review-failures [--artifact <path>] | workflow run explain-section|extract-evidence [--document current|documentId] --outline <id|title> [--artifact <path>] | workflow run review-evidence|synthesize-evidence --evidence <artifact> [--artifact <path>] | workflow run refine-artifact --source <artifact> --instruction <text> [--artifact <path>]";

    private static readonly JsonSerializerOptions EvidenceJsonOptions = new(JsonSerializerDefaults.Web)
    {
        WriteIndented = true
    };

    private readonly WorkflowCommand fallback = new();
    private readonly IWorkspaceStore workspaceStore;
    private readonly IPdfDocumentService pdfService;
    private readonly IAiChatService aiChatService;
    private readonly Func<WorkspaceState?> workspaceProvider;
    private readonly Func<WorkspaceSettings> settingsProvider;
    private readonly Func<LibraryItem?> selectedDocumentProvider;

    public ReadOsWorkflowCommand(
        IWorkspaceStore workspaceStore,
        IPdfDocumentService pdfService,
        IAiChatService aiChatService,
        Func<WorkspaceState?> workspaceProvider,
        Func<WorkspaceSettings> settingsProvider,
        Func<LibraryItem?> selectedDocumentProvider)
    {
        this.workspaceStore = workspaceStore;
        this.pdfService = pdfService;
        this.aiChatService = aiChatService;
        this.workspaceProvider = workspaceProvider;
        this.settingsProvider = settingsProvider;
        this.selectedDocumentProvider = selectedDocumentProvider;
    }

    public string Name => "workflow";

    public string Summary => "Summarize MSP sessions and run ReadOS document workflows.";

    public MspCommandMetadata Metadata => MspCommandMetadata.Create(
        Name,
        Summary,
        UsageText,
        MspCommandEffects.ReadWorkspace,
        new[] { "msp.workflow.read" });

    public MspCommandMetadata GetMetadata(IReadOnlyList<string> arguments)
    {
        if (!IsReadOsAppWorkflowInvocation(arguments))
        {
            return fallback.GetMetadata(arguments);
        }

        var isExplainSection = IsWorkflowInvocation(arguments, ExplainSectionWorkflowName);
        var isReviewEvidence = IsWorkflowInvocation(arguments, ReviewEvidenceWorkflowName);
        var isSynthesizeEvidence = IsWorkflowInvocation(arguments, SynthesizeEvidenceWorkflowName);
        var isRefineArtifact = IsWorkflowInvocation(arguments, RefineArtifactWorkflowName);
        var effects = isExplainSection || isSynthesizeEvidence || isRefineArtifact
            ? MspCommandEffects.ReadWorkspace | MspCommandEffects.ExternalModel
            : MspCommandEffects.ReadWorkspace;
        var capabilities = new List<string>
        {
            "msp.workflow.read"
        };
        if (isExplainSection)
        {
            capabilities.Add("reados.pdf.read");
            capabilities.Add("reados.chat.ask");
        }
        else if (isReviewEvidence)
        {
            capabilities.Add("msp.artifact.read");
        }
        else if (isSynthesizeEvidence)
        {
            capabilities.Add("msp.artifact.read");
            capabilities.Add("reados.chat.ask");
        }
        else if (isRefineArtifact)
        {
            capabilities.Add("msp.artifact.read");
            capabilities.Add("reados.chat.ask");
        }
        else
        {
            capabilities.Add("reados.pdf.read");
        }

        if (HasArtifactOutput(arguments))
        {
            effects |= MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact;
            capabilities.Add("msp.artifact.write");
        }

        return MspCommandMetadata.Create(
            Name,
            isExplainSection
                ? "Explain a ReadOS PDF outline section with the configured chat model."
                : isReviewEvidence
                    ? "Review structured evidence artifacts and produce Markdown notes."
                    : isSynthesizeEvidence
                        ? "Synthesize structured evidence artifacts with the configured chat model."
                        : isRefineArtifact
                            ? "Refine an existing artifact with an operator instruction and the configured chat model."
                            : "Extract structured ReadOS PDF section evidence into JSON.",
            "workflow run explain-section|extract-evidence [--document current|documentId] --outline <id|title> [--artifact <path>] | workflow run review-evidence|synthesize-evidence --evidence <artifact> [--artifact <path>] | workflow run refine-artifact --source <artifact> --instruction <text> [--artifact <path>]",
            effects,
            capabilities);
    }

    public MspCommandPreview GetPreview(IReadOnlyList<string> arguments)
    {
        if (!IsReadOsAppWorkflowInvocation(arguments))
        {
            return fallback.GetPreview(arguments);
        }

        var workflowName = arguments[1];
        if (IsWorkflowInvocation(arguments, ReviewEvidenceWorkflowName))
        {
            return PreviewReviewEvidence(arguments);
        }

        if (IsWorkflowInvocation(arguments, SynthesizeEvidenceWorkflowName))
        {
            return PreviewSynthesizeEvidence(arguments);
        }

        if (IsWorkflowInvocation(arguments, RefineArtifactWorkflowName))
        {
            return PreviewRefineArtifact(arguments);
        }

        if (!TryParseDocumentWorkflow(arguments, workflowName, out var parsed, out var error))
        {
            return MspCommandPreview.Create(
                GetDocumentWorkflowPreviewSummary(workflowName),
                details: new[] { error });
        }

        var workspace = workspaceProvider();
        var document = workspace is null
            ? null
            : ReadOsMspCommandHelpers.ResolvePdfDocument(workspace, parsed.DocumentSelector, selectedDocumentProvider);
        var targets = new List<string>
        {
            ReadOsMspCommandHelpers.DescribePdfTarget(workspace, parsed.DocumentSelector, selectedDocumentProvider)
        };
        if (!string.IsNullOrWhiteSpace(parsed.ArtifactPath))
        {
            targets.Add(NormalizeArtifactPath(parsed.ArtifactPath));
        }

        var details = new List<string>
        {
            $"workflow: {workflowName}",
            $"outline: {parsed.OutlineSelector}"
        };
        if (string.Equals(workflowName, ExplainSectionWorkflowName, StringComparison.OrdinalIgnoreCase))
        {
            details.Add($"provider: {settingsProvider().ProviderName}");
            details.Add($"model: {settingsProvider().ModelName}");
        }

        if (document is not null &&
            TryResolveOutlineRange(document, parsed.OutlineSelector, out var section, out var startPage, out var endPage))
        {
            details.Add($"section: {section.Title}");
            details.Add($"pages: {startPage}-{endPage}");
        }

        return MspCommandPreview.Create(
            GetDocumentWorkflowPreviewSummary(workflowName),
            targets,
            details);
    }

    public async ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default)
    {
        if (!IsReadOsAppWorkflowInvocation(arguments))
        {
            return await fallback.ExecuteAsync(context, arguments, cancellationToken);
        }

        if (IsWorkflowInvocation(arguments, ReviewEvidenceWorkflowName))
        {
            return await ExecuteReviewEvidenceAsync(context, arguments, cancellationToken);
        }

        if (IsWorkflowInvocation(arguments, SynthesizeEvidenceWorkflowName))
        {
            return await ExecuteSynthesizeEvidenceAsync(context, arguments, cancellationToken);
        }

        if (IsWorkflowInvocation(arguments, RefineArtifactWorkflowName))
        {
            return await ExecuteRefineArtifactAsync(context, arguments, cancellationToken);
        }

        return IsWorkflowInvocation(arguments, ExtractEvidenceWorkflowName)
            ? await ExecuteExtractEvidenceAsync(context, arguments, cancellationToken)
            : await ExecuteExplainSectionAsync(context, arguments, cancellationToken);
    }

    private async ValueTask<MspCommandResult> ExecuteExplainSectionAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken)
    {
        if (!TryParseDocumentWorkflow(arguments, ExplainSectionWorkflowName, out var parsed, out var parseError))
        {
            return MspCommandResult.Failure(parseError, exitCode: 2);
        }

        var workspace = workspaceProvider();
        if (workspace is null)
        {
            return MspCommandResult.Failure("ReadOS workspace is not loaded.");
        }

        var document = ReadOsMspCommandHelpers.ResolvePdfDocument(workspace, parsed.DocumentSelector, selectedDocumentProvider);
        if (document is null)
        {
            return ReadOsMspCommandHelpers.PdfDocumentNotFound(parsed.DocumentSelector);
        }

        string? artifactPath = null;
        if (!string.IsNullOrWhiteSpace(parsed.ArtifactPath))
        {
            artifactPath = NormalizeArtifactPath(parsed.ArtifactPath);
            if (!IsArtifactFilePath(artifactPath))
            {
                return MspCommandResult.Failure(
                    "workflow run explain-section --artifact must target a file under /artifacts.",
                    exitCode: 2,
                    code: "reados.workflow.invalid_artifact_path",
                    target: parsed.ArtifactPath,
                    recoveryHint: "Use a file path such as /artifacts/workflows/explain-section.md.");
            }
        }

        if (!TryResolveOutlineRange(document, parsed.OutlineSelector, out var section, out var startPage, out var endPage))
        {
            return ReadOsMspCommandHelpers.OutlineItemNotFound(document, parsed.OutlineSelector);
        }

        if (!ReadOsMspCommandHelpers.TryValidatePdfPage(document, startPage.ToString(), out _, out var startPageFailure))
        {
            return startPageFailure;
        }

        if (!ReadOsMspCommandHelpers.TryValidatePdfPage(document, endPage.ToString(), out _, out var endPageFailure))
        {
            return endPageFailure;
        }

        await context.ReportProgressAsync(
            $"Resolved outline section \"{section.Title}\" to pages {startPage}-{endPage}.",
            15,
            cancellationToken);
        var sectionText = await pdfService.ExtractPageTextAsync(
            workspaceStore.GetAbsolutePath(document),
            startPage,
            endPage,
            cancellationToken);
        await context.ReportProgressAsync(
            $"Extracted {sectionText.Length} characters for section \"{section.Title}\".",
            40,
            cancellationToken);

        var settings = settingsProvider();
        var prompt = BuildSectionPrompt(settings, document, section, startPage, endPage);
        var attachment = new ChatAttachment
        {
            Kind = startPage == endPage ? AttachmentKind.Page : AttachmentKind.PageRange,
            DocumentId = document.Id,
            Title = $"{document.Name} · {section.Title} · 第 {startPage}-{endPage} 页",
            StartPage = startPage,
            EndPage = endPage
        };
        await context.ReportProgressAsync(
            $"Calling chat model {settings.ModelName} for section explanation.",
            65,
            cancellationToken);
        string answer;
        try
        {
            answer = await aiChatService.SendAsync(
                settings,
                document,
                Array.Empty<ChatMessage>(),
                prompt,
                new[] { attachment },
                _ => Task.FromResult(sectionText),
                allowMspCommandRequests: false,
                cancellationToken: cancellationToken);
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            return ReadOsMspCommandHelpers.ChatModelProviderFailed(settings, ex);
        }

        var report = BuildSectionReport(document, section, startPage, endPage, prompt, answer);
        var output = report.EndsWith(Environment.NewLine, StringComparison.Ordinal)
            ? report
            : report + Environment.NewLine;

        MspArtifact[] artifacts = Array.Empty<MspArtifact>();
        if (!string.IsNullOrWhiteSpace(artifactPath))
        {
            await context.ReportProgressAsync(
                $"Writing section explanation artifact {artifactPath}.",
                90,
                cancellationToken);
            var sourcePaths = GetSourcePaths(document.Id, startPage, endPage).ToArray();
            var sourcePages = new[] { $"{document.Id}:{startPage}-{endPage}" };
            var now = DateTimeOffset.UtcNow;
            var artifact = new MspArtifact
            {
                Path = artifactPath,
                MediaType = "text/markdown",
                SizeBytes = report.Length,
                Description = $"Section explanation for {section.Title} in {document.Name}.",
                SourceCommand = context.Invocation.CommandText,
                Actor = context.Invocation.Actor,
                SessionId = context.Invocation.SessionId,
                SourcePaths = sourcePaths,
                SourceDocuments = new[] { document.Id },
                SourcePages = sourcePages,
                CreatedAt = now,
                UpdatedAt = now,
                Preview = $"document: {document.Name}; section: {section.Title}; pages: {startPage}-{endPage}; contentLength: {report.Length}"
            };
            await context.Workspace.WriteTextAsync(artifactPath, report, artifact, cancellationToken);
            artifacts = new[] { artifact };
            output += $"artifact\t{artifactPath}\t{report.Length}{Environment.NewLine}";
        }

        await context.ReportProgressAsync("Section explanation workflow complete.", 100, cancellationToken);
        return MspCommandResult.Success(output, artifacts);
    }

    private async ValueTask<MspCommandResult> ExecuteExtractEvidenceAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken)
    {
        if (!TryParseDocumentWorkflow(arguments, ExtractEvidenceWorkflowName, out var parsed, out var parseError))
        {
            return MspCommandResult.Failure(parseError, exitCode: 2);
        }

        var workspace = workspaceProvider();
        if (workspace is null)
        {
            return MspCommandResult.Failure("ReadOS workspace is not loaded.");
        }

        var document = ReadOsMspCommandHelpers.ResolvePdfDocument(workspace, parsed.DocumentSelector, selectedDocumentProvider);
        if (document is null)
        {
            return ReadOsMspCommandHelpers.PdfDocumentNotFound(parsed.DocumentSelector);
        }

        string? artifactPath = null;
        if (!string.IsNullOrWhiteSpace(parsed.ArtifactPath))
        {
            artifactPath = NormalizeArtifactPath(parsed.ArtifactPath);
            if (!IsArtifactFilePath(artifactPath))
            {
                return MspCommandResult.Failure(
                    "workflow run extract-evidence --artifact must target a file under /artifacts.",
                    exitCode: 2,
                    code: "reados.workflow.invalid_artifact_path",
                    target: parsed.ArtifactPath,
                    recoveryHint: "Use a file path such as /artifacts/workflows/evidence.json.");
            }
        }

        if (!TryResolveOutlineRange(document, parsed.OutlineSelector, out var section, out var startPage, out var endPage))
        {
            return ReadOsMspCommandHelpers.OutlineItemNotFound(document, parsed.OutlineSelector);
        }

        if (!ReadOsMspCommandHelpers.TryValidatePdfPage(document, startPage.ToString(), out _, out var startPageFailure))
        {
            return startPageFailure;
        }

        if (!ReadOsMspCommandHelpers.TryValidatePdfPage(document, endPage.ToString(), out _, out var endPageFailure))
        {
            return endPageFailure;
        }

        await context.ReportProgressAsync(
            $"Resolved evidence section \"{section.Title}\" to pages {startPage}-{endPage}.",
            15,
            cancellationToken);
        var pages = new List<EvidencePageRecord>();
        for (var page = startPage; page <= endPage; page++)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var text = await pdfService.ExtractPageTextAsync(
                workspaceStore.GetAbsolutePath(document),
                page,
                page,
                cancellationToken);
            pages.Add(new EvidencePageRecord(
                page,
                $"/documents/{document.Id}/pages/{page}.txt",
                text.Length,
                text));
            var percent = 15 + (int)Math.Round((double)(page - startPage + 1) / (endPage - startPage + 1) * 70);
            await context.ReportProgressAsync(
                $"Extracted evidence page {page}.",
                percent,
                cancellationToken);
        }

        var record = new EvidenceArtifactRecord(
            ExtractEvidenceWorkflowName,
            DateTimeOffset.UtcNow,
            new EvidenceDocumentRecord(document.Id, document.Name),
            new EvidenceSectionRecord(section.Id, section.Title, section.Level, startPage, endPage),
            pages);
        var content = JsonSerializer.Serialize(record, EvidenceJsonOptions) + Environment.NewLine;
        var output = content;

        MspArtifact[] artifacts = Array.Empty<MspArtifact>();
        if (!string.IsNullOrWhiteSpace(artifactPath))
        {
            await context.ReportProgressAsync(
                $"Writing structured evidence artifact {artifactPath}.",
                95,
                cancellationToken);
            var sourcePaths = GetSourcePaths(document.Id, startPage, endPage).ToArray();
            var sourcePages = new[] { $"{document.Id}:{startPage}-{endPage}" };
            var now = DateTimeOffset.UtcNow;
            var artifact = new MspArtifact
            {
                Path = artifactPath,
                MediaType = "application/json",
                SizeBytes = content.Length,
                Description = $"Structured evidence for {section.Title} in {document.Name}.",
                SourceCommand = context.Invocation.CommandText,
                Actor = context.Invocation.Actor,
                SessionId = context.Invocation.SessionId,
                SourcePaths = sourcePaths,
                SourceDocuments = new[] { document.Id },
                SourcePages = sourcePages,
                CreatedAt = now,
                UpdatedAt = now,
                Preview = $"document: {document.Name}; section: {section.Title}; pages: {startPage}-{endPage}; entries: {pages.Count}; contentLength: {content.Length}"
            };
            await context.Workspace.WriteTextAsync(artifactPath, content, artifact, cancellationToken);
            artifacts = new[] { artifact };
            output += $"artifact\t{artifactPath}\t{pages.Count}{Environment.NewLine}";
        }

        await context.ReportProgressAsync("Evidence extraction workflow complete.", 100, cancellationToken);
        return MspCommandResult.Success(output, artifacts);
    }

    private async ValueTask<MspCommandResult> ExecuteReviewEvidenceAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken)
    {
        if (!TryParseReviewEvidence(arguments, out var parsed, out var parseError))
        {
            return MspCommandResult.Failure(parseError, exitCode: 2);
        }

        if (!TryNormalizeArtifactFilePath(parsed.EvidencePath, out var evidencePath))
        {
            return MspCommandResult.Failure(
                "workflow run review-evidence --evidence must target a file under /artifacts.",
                exitCode: 2,
                code: "reados.workflow.invalid_evidence_path",
                target: parsed.EvidencePath,
                recoveryHint: "Use an evidence artifact path such as /artifacts/workflows/evidence.json.");
        }

        string? artifactPath = null;
        if (!string.IsNullOrWhiteSpace(parsed.ArtifactPath))
        {
            artifactPath = NormalizeArtifactPath(parsed.ArtifactPath);
            if (!IsArtifactFilePath(artifactPath))
            {
                return MspCommandResult.Failure(
                    "workflow run review-evidence --artifact must target a file under /artifacts.",
                    exitCode: 2,
                    code: "reados.workflow.invalid_artifact_path",
                    target: parsed.ArtifactPath,
                    recoveryHint: "Use a file path such as /artifacts/workflows/evidence-review.md.");
            }
        }

        await context.ReportProgressAsync(
            $"Reading structured evidence artifact {evidencePath}.",
            20,
            cancellationToken);
        var evidenceContent = await context.Workspace.TryReadTextAsync(evidencePath, cancellationToken);
        if (evidenceContent is null)
        {
            return MspCommandResult.Failure(
                $"Evidence artifact not found: {evidencePath}",
                code: "reados.workflow.evidence_artifact_not_found",
                target: evidencePath,
                recoveryHint: "Run workflow run extract-evidence with --artifact first, then pass that artifact path to review-evidence.");
        }

        if (!TryParseEvidenceArtifact(evidenceContent, out var evidence, out var evidenceError))
        {
            return MspCommandResult.Failure(
                $"Evidence artifact is not valid extract-evidence JSON: {evidencePath}",
                exitCode: 2,
                code: "reados.workflow.invalid_evidence_artifact",
                target: evidencePath,
                recoveryHint: evidenceError);
        }

        await context.ReportProgressAsync(
            $"Building evidence review for {evidence.Pages.Count} source pages.",
            60,
            cancellationToken);
        var diagnostics = new List<MspCommandDiagnostic>();
        var manifest = await ReadArtifactManifestAsync(context, evidencePath, diagnostics, cancellationToken);
        var report = BuildEvidenceReviewReport(evidencePath, evidence);
        var output = report.EndsWith(Environment.NewLine, StringComparison.Ordinal)
            ? report
            : report + Environment.NewLine;

        MspArtifact[] artifacts = Array.Empty<MspArtifact>();
        if (!string.IsNullOrWhiteSpace(artifactPath))
        {
            await context.ReportProgressAsync(
                $"Writing evidence review artifact {artifactPath}.",
                90,
                cancellationToken);
            var now = DateTimeOffset.UtcNow;
            var sourcePaths = BuildReviewSourcePaths(evidencePath, manifest, evidence).ToArray();
            var sourceDocuments = manifest?.SourceDocuments.Count > 0
                ? manifest.SourceDocuments
                : new[] { evidence.Document.Id };
            var sourcePages = manifest?.SourcePages.Count > 0
                ? manifest.SourcePages
                : new[] { $"{evidence.Document.Id}:{evidence.Section.StartPage}-{evidence.Section.EndPage}" };
            var artifact = new MspArtifact
            {
                Path = artifactPath,
                MediaType = "text/markdown",
                SizeBytes = report.Length,
                Description = $"Evidence review for {evidence.Section.Title} in {evidence.Document.Name}.",
                SourceCommand = context.Invocation.CommandText,
                Actor = context.Invocation.Actor,
                SessionId = context.Invocation.SessionId,
                SourcePaths = sourcePaths,
                SourceDocuments = sourceDocuments,
                SourcePages = sourcePages,
                CreatedAt = now,
                UpdatedAt = now,
                Preview = $"evidence: {evidencePath}; document: {evidence.Document.Name}; section: {evidence.Section.Title}; citations: {evidence.Pages.Count}; contentLength: {report.Length}"
            };
            await context.Workspace.WriteTextAsync(artifactPath, report, artifact, cancellationToken);
            artifacts = new[] { artifact };
            output += $"artifact\t{artifactPath}\t{report.Length}{Environment.NewLine}";
        }

        await context.ReportProgressAsync("Evidence review workflow complete.", 100, cancellationToken);
        return MspCommandResult.Success(output, artifacts, diagnostics);
    }

    private async ValueTask<MspCommandResult> ExecuteSynthesizeEvidenceAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken)
    {
        if (!TryParseSynthesizeEvidence(arguments, out var parsed, out var parseError))
        {
            return MspCommandResult.Failure(parseError, exitCode: 2);
        }

        if (!TryNormalizeArtifactFilePath(parsed.EvidencePath, out var evidencePath))
        {
            return MspCommandResult.Failure(
                "workflow run synthesize-evidence --evidence must target a file under /artifacts.",
                exitCode: 2,
                code: "reados.workflow.invalid_evidence_path",
                target: parsed.EvidencePath,
                recoveryHint: "Use an evidence artifact path such as /artifacts/workflows/evidence.json.");
        }

        string? artifactPath = null;
        if (!string.IsNullOrWhiteSpace(parsed.ArtifactPath))
        {
            artifactPath = NormalizeArtifactPath(parsed.ArtifactPath);
            if (!IsArtifactFilePath(artifactPath))
            {
                return MspCommandResult.Failure(
                    "workflow run synthesize-evidence --artifact must target a file under /artifacts.",
                    exitCode: 2,
                    code: "reados.workflow.invalid_artifact_path",
                    target: parsed.ArtifactPath,
                    recoveryHint: "Use a file path such as /artifacts/workflows/evidence-synthesis.md.");
            }
        }

        await context.ReportProgressAsync(
            $"Reading structured evidence artifact {evidencePath}.",
            20,
            cancellationToken);
        var evidenceContent = await context.Workspace.TryReadTextAsync(evidencePath, cancellationToken);
        if (evidenceContent is null)
        {
            return MspCommandResult.Failure(
                $"Evidence artifact not found: {evidencePath}",
                code: "reados.workflow.evidence_artifact_not_found",
                target: evidencePath,
                recoveryHint: "Run workflow run extract-evidence with --artifact first, then pass that artifact path to synthesize-evidence.");
        }

        if (!TryParseEvidenceArtifact(evidenceContent, out var evidence, out var evidenceError))
        {
            return MspCommandResult.Failure(
                $"Evidence artifact is not valid extract-evidence JSON: {evidencePath}",
                exitCode: 2,
                code: "reados.workflow.invalid_evidence_artifact",
                target: evidencePath,
                recoveryHint: evidenceError);
        }

        var diagnostics = new List<MspCommandDiagnostic>();
        var manifest = await ReadArtifactManifestAsync(context, evidencePath, diagnostics, cancellationToken);
        var settings = settingsProvider();
        var document = ResolveEvidenceDocument(evidence);
        var prompt = BuildEvidenceSynthesisPrompt(evidencePath, evidence);
        var evidenceAttachment = new ChatAttachment
        {
            Kind = AttachmentKind.File,
            DocumentId = evidence.Document.Id,
            Title = $"Evidence · {evidence.Section.Title}",
            FilePath = evidencePath,
            StartPage = evidence.Section.StartPage,
            EndPage = evidence.Section.EndPage
        };
        var evidenceContext = BuildEvidenceModelContext(evidencePath, evidence);
        await context.ReportProgressAsync(
            $"Calling chat model {settings.ModelName} for evidence synthesis.",
            65,
            cancellationToken);
        string synthesis;
        try
        {
            synthesis = await aiChatService.SendAsync(
                settings,
                document,
                Array.Empty<ChatMessage>(),
                prompt,
                new[] { evidenceAttachment },
                _ => Task.FromResult(evidenceContext),
                allowMspCommandRequests: false,
                cancellationToken: cancellationToken);
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            return ReadOsMspCommandHelpers.ChatModelProviderFailed(settings, ex);
        }

        var report = BuildEvidenceSynthesisReport(evidencePath, evidence, prompt, synthesis);
        var output = report.EndsWith(Environment.NewLine, StringComparison.Ordinal)
            ? report
            : report + Environment.NewLine;

        MspArtifact[] artifacts = Array.Empty<MspArtifact>();
        if (!string.IsNullOrWhiteSpace(artifactPath))
        {
            await context.ReportProgressAsync(
                $"Writing evidence synthesis artifact {artifactPath}.",
                90,
                cancellationToken);
            var now = DateTimeOffset.UtcNow;
            var sourcePaths = BuildReviewSourcePaths(evidencePath, manifest, evidence).ToArray();
            var sourceDocuments = manifest?.SourceDocuments.Count > 0
                ? manifest.SourceDocuments
                : new[] { evidence.Document.Id };
            var sourcePages = manifest?.SourcePages.Count > 0
                ? manifest.SourcePages
                : new[] { $"{evidence.Document.Id}:{evidence.Section.StartPage}-{evidence.Section.EndPage}" };
            var artifact = new MspArtifact
            {
                Path = artifactPath,
                MediaType = "text/markdown",
                SizeBytes = report.Length,
                Description = $"Evidence synthesis for {evidence.Section.Title} in {evidence.Document.Name}.",
                SourceCommand = context.Invocation.CommandText,
                Actor = context.Invocation.Actor,
                SessionId = context.Invocation.SessionId,
                SourcePaths = sourcePaths,
                SourceDocuments = sourceDocuments,
                SourcePages = sourcePages,
                CreatedAt = now,
                UpdatedAt = now,
                Preview = $"evidence: {evidencePath}; document: {evidence.Document.Name}; section: {evidence.Section.Title}; citations: {evidence.Pages.Count}; contentLength: {report.Length}"
            };
            await context.Workspace.WriteTextAsync(artifactPath, report, artifact, cancellationToken);
            artifacts = new[] { artifact };
            output += $"artifact\t{artifactPath}\t{report.Length}{Environment.NewLine}";
        }

        await context.ReportProgressAsync("Evidence synthesis workflow complete.", 100, cancellationToken);
        return MspCommandResult.Success(output, artifacts, diagnostics);
    }

    private async ValueTask<MspCommandResult> ExecuteRefineArtifactAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken)
    {
        if (!TryParseRefineArtifact(arguments, out var parsed, out var parseError))
        {
            return MspCommandResult.Failure(parseError, exitCode: 2);
        }

        if (!TryNormalizeArtifactFilePath(parsed.SourcePath, out var sourcePath))
        {
            return MspCommandResult.Failure(
                "workflow run refine-artifact --source must target a file under /artifacts.",
                exitCode: 2,
                code: "reados.workflow.invalid_source_artifact_path",
                target: parsed.SourcePath,
                recoveryHint: "Use an existing artifact path such as /artifacts/workflows/evidence-synthesis.md.");
        }

        string? artifactPath = null;
        if (!string.IsNullOrWhiteSpace(parsed.ArtifactPath))
        {
            artifactPath = NormalizeArtifactPath(parsed.ArtifactPath);
            if (!IsArtifactFilePath(artifactPath))
            {
                return MspCommandResult.Failure(
                    "workflow run refine-artifact --artifact must target a file under /artifacts.",
                    exitCode: 2,
                    code: "reados.workflow.invalid_artifact_path",
                    target: parsed.ArtifactPath,
                    recoveryHint: "Use a file path such as /artifacts/workflows/evidence-synthesis-refined.md.");
            }
        }

        await context.ReportProgressAsync(
            $"Reading source artifact {sourcePath}.",
            20,
            cancellationToken);
        var sourceContent = await context.Workspace.TryReadTextAsync(sourcePath, cancellationToken);
        if (sourceContent is null)
        {
            return MspCommandResult.Failure(
                $"Source artifact not found: {sourcePath}",
                code: "reados.workflow.source_artifact_not_found",
                target: sourcePath,
                recoveryHint: "Run the workflow that creates the source artifact first, then pass that artifact path to refine-artifact.");
        }

        var diagnostics = new List<MspCommandDiagnostic>();
        var manifest = await ReadArtifactManifestAsync(context, sourcePath, diagnostics, cancellationToken);
        var settings = settingsProvider();
        var document = ResolveArtifactDocument(manifest);
        var prompt = BuildArtifactRefinementPrompt(sourcePath, parsed.Instruction, manifest);
        var attachment = new ChatAttachment
        {
            Kind = AttachmentKind.File,
            DocumentId = manifest?.SourceDocuments.FirstOrDefault() ?? string.Empty,
            Title = $"Source Artifact · {Path.GetFileName(sourcePath)}",
            FilePath = sourcePath
        };
        await context.ReportProgressAsync(
            $"Calling chat model {settings.ModelName} for artifact refinement.",
            65,
            cancellationToken);
        string refined;
        try
        {
            refined = await aiChatService.SendAsync(
                settings,
                document,
                Array.Empty<ChatMessage>(),
                prompt,
                new[] { attachment },
                _ => Task.FromResult(sourceContent),
                allowMspCommandRequests: false,
                cancellationToken: cancellationToken);
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            return ReadOsMspCommandHelpers.ChatModelProviderFailed(settings, ex);
        }

        var report = BuildArtifactRefinementReport(sourcePath, parsed.Instruction, prompt, refined);
        var output = report.EndsWith(Environment.NewLine, StringComparison.Ordinal)
            ? report
            : report + Environment.NewLine;

        MspArtifact[] artifacts = Array.Empty<MspArtifact>();
        if (!string.IsNullOrWhiteSpace(artifactPath))
        {
            await context.ReportProgressAsync(
                $"Writing refined artifact {artifactPath}.",
                90,
                cancellationToken);
            var now = DateTimeOffset.UtcNow;
            var sourcePaths = BuildArtifactDerivativeSourcePaths(sourcePath, manifest).ToArray();
            var sourceDocuments = manifest?.SourceDocuments ?? Array.Empty<string>();
            var sourcePages = manifest?.SourcePages ?? Array.Empty<string>();
            var artifact = new MspArtifact
            {
                Path = artifactPath,
                MediaType = "text/markdown",
                SizeBytes = report.Length,
                Description = $"Refined artifact derived from {sourcePath}.",
                SourceCommand = context.Invocation.CommandText,
                Actor = context.Invocation.Actor,
                SessionId = context.Invocation.SessionId,
                SourcePaths = sourcePaths,
                SourceDocuments = sourceDocuments,
                SourcePages = sourcePages,
                CreatedAt = now,
                UpdatedAt = now,
                Preview = $"source: {sourcePath}; instruction: {TrimSingleLine(parsed.Instruction, 120)}; contentLength: {report.Length}"
            };
            await context.Workspace.WriteTextAsync(artifactPath, report, artifact, cancellationToken);
            artifacts = new[] { artifact };
            output += $"artifact\t{artifactPath}\t{report.Length}{Environment.NewLine}";
        }

        await context.ReportProgressAsync("Artifact refinement workflow complete.", 100, cancellationToken);
        return MspCommandResult.Success(output, artifacts, diagnostics);
    }

    private static bool IsReadOsAppWorkflowInvocation(IReadOnlyList<string> arguments)
    {
        return IsWorkflowInvocation(arguments, ExplainSectionWorkflowName) ||
            IsWorkflowInvocation(arguments, ExtractEvidenceWorkflowName) ||
            IsWorkflowInvocation(arguments, ReviewEvidenceWorkflowName) ||
            IsWorkflowInvocation(arguments, SynthesizeEvidenceWorkflowName) ||
            IsWorkflowInvocation(arguments, RefineArtifactWorkflowName);
    }

    private static bool IsWorkflowInvocation(IReadOnlyList<string> arguments, string workflowName)
    {
        return arguments.Count >= 2 &&
            string.Equals(arguments[0], "run", StringComparison.OrdinalIgnoreCase) &&
            string.Equals(arguments[1], workflowName, StringComparison.OrdinalIgnoreCase);
    }

    private static string GetDocumentWorkflowPreviewSummary(string workflowName)
    {
        return string.Equals(workflowName, ExtractEvidenceWorkflowName, StringComparison.OrdinalIgnoreCase)
            ? "Extract structured PDF section evidence and optionally write a provenance-backed JSON artifact."
            : "Explain a PDF outline section and optionally write a provenance-backed artifact.";
    }

    private static MspCommandPreview PreviewReviewEvidence(IReadOnlyList<string> arguments)
    {
        if (!TryParseReviewEvidence(arguments, out var parsed, out var error))
        {
            return MspCommandPreview.Create(
                "Review structured evidence and optionally write a provenance-backed Markdown artifact.",
                details: new[] { error });
        }

        var targets = new List<string>
        {
            NormalizeArtifactPath(parsed.EvidencePath)
        };
        if (!string.IsNullOrWhiteSpace(parsed.ArtifactPath))
        {
            targets.Add(NormalizeArtifactPath(parsed.ArtifactPath));
        }

        return MspCommandPreview.Create(
            "Review structured evidence and optionally write a provenance-backed Markdown artifact.",
            targets,
            new[]
            {
                $"workflow: {ReviewEvidenceWorkflowName}",
                $"evidence: {NormalizeArtifactPath(parsed.EvidencePath)}"
            });
    }

    private static MspCommandPreview PreviewSynthesizeEvidence(IReadOnlyList<string> arguments)
    {
        if (!TryParseSynthesizeEvidence(arguments, out var parsed, out var error))
        {
            return MspCommandPreview.Create(
                "Synthesize structured evidence with the configured chat model and optionally write a Markdown artifact.",
                details: new[] { error });
        }

        var targets = new List<string>
        {
            NormalizeArtifactPath(parsed.EvidencePath)
        };
        if (!string.IsNullOrWhiteSpace(parsed.ArtifactPath))
        {
            targets.Add(NormalizeArtifactPath(parsed.ArtifactPath));
        }

        return MspCommandPreview.Create(
            "Synthesize structured evidence with the configured chat model and optionally write a Markdown artifact.",
            targets,
            new[]
            {
                $"workflow: {SynthesizeEvidenceWorkflowName}",
                $"evidence: {NormalizeArtifactPath(parsed.EvidencePath)}"
            });
    }

    private static MspCommandPreview PreviewRefineArtifact(IReadOnlyList<string> arguments)
    {
        if (!TryParseRefineArtifact(arguments, out var parsed, out var error))
        {
            return MspCommandPreview.Create(
                "Refine an existing artifact with the configured chat model.",
                details: new[] { error });
        }

        var targets = new List<string>
        {
            NormalizeArtifactPath(parsed.SourcePath)
        };
        if (!string.IsNullOrWhiteSpace(parsed.ArtifactPath))
        {
            targets.Add(NormalizeArtifactPath(parsed.ArtifactPath));
        }

        return MspCommandPreview.Create(
            "Refine an existing artifact with the configured chat model.",
            targets,
            new[]
            {
                $"workflow: {RefineArtifactWorkflowName}",
                $"source: {NormalizeArtifactPath(parsed.SourcePath)}",
                $"instruction: {TrimSingleLine(parsed.Instruction, 160)}"
            });
    }

    private static bool TryParseDocumentWorkflow(
        IReadOnlyList<string> arguments,
        string workflowName,
        out DocumentWorkflowArguments parsed,
        out string error)
    {
        parsed = new DocumentWorkflowArguments("current", string.Empty, null);
        error = string.Empty;
        if (!IsWorkflowInvocation(arguments, workflowName))
        {
            error = UsageText;
            return false;
        }

        var documentSelector = "current";
        string? outlineSelector = null;
        string? artifactPath = null;
        for (var index = 2; index < arguments.Count; index++)
        {
            var argument = arguments[index];
            if (string.Equals(argument, "--document", StringComparison.OrdinalIgnoreCase))
            {
                if (index + 1 >= arguments.Count || string.IsNullOrWhiteSpace(arguments[index + 1]))
                {
                    error = "--document requires current or a document id.";
                    return false;
                }

                documentSelector = arguments[++index];
                continue;
            }

            if (argument.StartsWith("--document=", StringComparison.OrdinalIgnoreCase))
            {
                documentSelector = argument["--document=".Length..];
                if (string.IsNullOrWhiteSpace(documentSelector))
                {
                    error = "--document requires current or a document id.";
                    return false;
                }

                continue;
            }

            if (string.Equals(argument, "--outline", StringComparison.OrdinalIgnoreCase))
            {
                if (index + 1 >= arguments.Count || string.IsNullOrWhiteSpace(arguments[index + 1]))
                {
                    error = "--outline requires an outline id or title.";
                    return false;
                }

                outlineSelector = arguments[++index];
                continue;
            }

            if (argument.StartsWith("--outline=", StringComparison.OrdinalIgnoreCase))
            {
                outlineSelector = argument["--outline=".Length..];
                if (string.IsNullOrWhiteSpace(outlineSelector))
                {
                    error = "--outline requires an outline id or title.";
                    return false;
                }

                continue;
            }

            if (string.Equals(argument, "--artifact", StringComparison.OrdinalIgnoreCase))
            {
                if (!string.IsNullOrWhiteSpace(artifactPath))
                {
                    error = "--artifact can only be specified once.";
                    return false;
                }

                if (index + 1 >= arguments.Count || string.IsNullOrWhiteSpace(arguments[index + 1]))
                {
                    error = "--artifact requires a path.";
                    return false;
                }

                artifactPath = arguments[++index];
                continue;
            }

            if (argument.StartsWith("--artifact=", StringComparison.OrdinalIgnoreCase))
            {
                if (!string.IsNullOrWhiteSpace(artifactPath))
                {
                    error = "--artifact can only be specified once.";
                    return false;
                }

                artifactPath = argument["--artifact=".Length..];
                if (string.IsNullOrWhiteSpace(artifactPath))
                {
                    error = "--artifact requires a path.";
                    return false;
                }

                continue;
            }

            error = $"Unexpected workflow {workflowName} argument: {argument}";
            return false;
        }

        if (string.IsNullOrWhiteSpace(outlineSelector))
        {
            error = $"workflow run {workflowName} requires --outline <id|title>.";
            return false;
        }

        parsed = new DocumentWorkflowArguments(documentSelector, outlineSelector, artifactPath);
        return true;
    }

    private static bool TryParseReviewEvidence(
        IReadOnlyList<string> arguments,
        out ReviewEvidenceArguments parsed,
        out string error)
    {
        return TryParseEvidenceArtifactWorkflow(arguments, ReviewEvidenceWorkflowName, out parsed, out error);
    }

    private static bool TryParseSynthesizeEvidence(
        IReadOnlyList<string> arguments,
        out ReviewEvidenceArguments parsed,
        out string error)
    {
        return TryParseEvidenceArtifactWorkflow(arguments, SynthesizeEvidenceWorkflowName, out parsed, out error);
    }

    private static bool TryParseEvidenceArtifactWorkflow(
        IReadOnlyList<string> arguments,
        string workflowName,
        out ReviewEvidenceArguments parsed,
        out string error)
    {
        parsed = new ReviewEvidenceArguments(string.Empty, null);
        error = string.Empty;
        if (!IsWorkflowInvocation(arguments, workflowName))
        {
            error = UsageText;
            return false;
        }

        string? evidencePath = null;
        string? artifactPath = null;
        for (var index = 2; index < arguments.Count; index++)
        {
            var argument = arguments[index];
            if (string.Equals(argument, "--evidence", StringComparison.OrdinalIgnoreCase))
            {
                if (index + 1 >= arguments.Count || string.IsNullOrWhiteSpace(arguments[index + 1]))
                {
                    error = "--evidence requires an artifact path.";
                    return false;
                }

                evidencePath = arguments[++index];
                continue;
            }

            if (argument.StartsWith("--evidence=", StringComparison.OrdinalIgnoreCase))
            {
                evidencePath = argument["--evidence=".Length..];
                if (string.IsNullOrWhiteSpace(evidencePath))
                {
                    error = "--evidence requires an artifact path.";
                    return false;
                }

                continue;
            }

            if (string.Equals(argument, "--artifact", StringComparison.OrdinalIgnoreCase))
            {
                if (!string.IsNullOrWhiteSpace(artifactPath))
                {
                    error = "--artifact can only be specified once.";
                    return false;
                }

                if (index + 1 >= arguments.Count || string.IsNullOrWhiteSpace(arguments[index + 1]))
                {
                    error = "--artifact requires a path.";
                    return false;
                }

                artifactPath = arguments[++index];
                continue;
            }

            if (argument.StartsWith("--artifact=", StringComparison.OrdinalIgnoreCase))
            {
                if (!string.IsNullOrWhiteSpace(artifactPath))
                {
                    error = "--artifact can only be specified once.";
                    return false;
                }

                artifactPath = argument["--artifact=".Length..];
                if (string.IsNullOrWhiteSpace(artifactPath))
                {
                    error = "--artifact requires a path.";
                    return false;
                }

                continue;
            }

            error = $"Unexpected workflow {workflowName} argument: {argument}";
            return false;
        }

        if (string.IsNullOrWhiteSpace(evidencePath))
        {
            error = $"workflow run {workflowName} requires --evidence <artifact>.";
            return false;
        }

        parsed = new ReviewEvidenceArguments(evidencePath, artifactPath);
        return true;
    }

    private static bool TryParseRefineArtifact(
        IReadOnlyList<string> arguments,
        out RefineArtifactArguments parsed,
        out string error)
    {
        parsed = new RefineArtifactArguments(string.Empty, string.Empty, null);
        error = string.Empty;
        if (!IsWorkflowInvocation(arguments, RefineArtifactWorkflowName))
        {
            error = UsageText;
            return false;
        }

        string? sourcePath = null;
        string? instruction = null;
        string? artifactPath = null;
        for (var index = 2; index < arguments.Count; index++)
        {
            var argument = arguments[index];
            if (string.Equals(argument, "--source", StringComparison.OrdinalIgnoreCase))
            {
                if (index + 1 >= arguments.Count || string.IsNullOrWhiteSpace(arguments[index + 1]))
                {
                    error = "--source requires an artifact path.";
                    return false;
                }

                sourcePath = arguments[++index];
                continue;
            }

            if (argument.StartsWith("--source=", StringComparison.OrdinalIgnoreCase))
            {
                sourcePath = argument["--source=".Length..];
                if (string.IsNullOrWhiteSpace(sourcePath))
                {
                    error = "--source requires an artifact path.";
                    return false;
                }

                continue;
            }

            if (string.Equals(argument, "--instruction", StringComparison.OrdinalIgnoreCase))
            {
                if (index + 1 >= arguments.Count || string.IsNullOrWhiteSpace(arguments[index + 1]))
                {
                    error = "--instruction requires text.";
                    return false;
                }

                instruction = arguments[++index];
                continue;
            }

            if (argument.StartsWith("--instruction=", StringComparison.OrdinalIgnoreCase))
            {
                instruction = argument["--instruction=".Length..];
                if (string.IsNullOrWhiteSpace(instruction))
                {
                    error = "--instruction requires text.";
                    return false;
                }

                continue;
            }

            if (string.Equals(argument, "--artifact", StringComparison.OrdinalIgnoreCase))
            {
                if (!string.IsNullOrWhiteSpace(artifactPath))
                {
                    error = "--artifact can only be specified once.";
                    return false;
                }

                if (index + 1 >= arguments.Count || string.IsNullOrWhiteSpace(arguments[index + 1]))
                {
                    error = "--artifact requires a path.";
                    return false;
                }

                artifactPath = arguments[++index];
                continue;
            }

            if (argument.StartsWith("--artifact=", StringComparison.OrdinalIgnoreCase))
            {
                if (!string.IsNullOrWhiteSpace(artifactPath))
                {
                    error = "--artifact can only be specified once.";
                    return false;
                }

                artifactPath = argument["--artifact=".Length..];
                if (string.IsNullOrWhiteSpace(artifactPath))
                {
                    error = "--artifact requires a path.";
                    return false;
                }

                continue;
            }

            error = $"Unexpected workflow {RefineArtifactWorkflowName} argument: {argument}";
            return false;
        }

        if (string.IsNullOrWhiteSpace(sourcePath))
        {
            error = "workflow run refine-artifact requires --source <artifact>.";
            return false;
        }

        if (string.IsNullOrWhiteSpace(instruction))
        {
            error = "workflow run refine-artifact requires --instruction <text>.";
            return false;
        }

        parsed = new RefineArtifactArguments(sourcePath, instruction, artifactPath);
        return true;
    }

    private static bool TryResolveOutlineRange(
        LibraryItem document,
        string selector,
        out OutlineItem section,
        out int startPage,
        out int endPage)
    {
        section = ResolveOutlineItem(document, selector) ?? new OutlineItem();
        if (string.IsNullOrWhiteSpace(section.Id) ||
            !document.Outline.Contains(section))
        {
            startPage = 0;
            endPage = 0;
            return false;
        }

        startPage = section.Page;
        var ordered = document.Outline
            .Select((item, index) => new OutlineEntry(item, index))
            .OrderBy(entry => entry.Item.Page)
            .ThenBy(entry => entry.Index)
            .ToArray();
        var resolvedSection = section;
        var nextPeer = ordered.FirstOrDefault(entry =>
            entry.Item.Page > resolvedSection.Page &&
            entry.Item.Level <= resolvedSection.Level);
        endPage = nextPeer is null
            ? document.PageCount
            : Math.Max(section.Page, nextPeer.Item.Page - 1);
        endPage = Math.Min(document.PageCount, endPage);
        return true;
    }

    private static OutlineItem? ResolveOutlineItem(LibraryItem document, string selector)
    {
        return document.Outline.FirstOrDefault(item =>
                string.Equals(item.Id, selector, StringComparison.OrdinalIgnoreCase) ||
                string.Equals(item.Title, selector, StringComparison.OrdinalIgnoreCase))
            ?? document.Outline.FirstOrDefault(item =>
                item.Title.StartsWith(selector, StringComparison.OrdinalIgnoreCase));
    }

    private static string BuildSectionPrompt(
        WorkspaceSettings settings,
        LibraryItem document,
        OutlineItem section,
        int startPage,
        int endPage)
    {
        var basePrompt = string.IsNullOrWhiteSpace(settings.ChapterExplainPrompt)
            ? "请围绕我附加的这一整节内容进行系统讲解。"
            : settings.ChapterExplainPrompt.Trim();
        var builder = new StringBuilder();
        builder.AppendLine(basePrompt);
        builder.AppendLine();
        builder.AppendLine($"Document: {document.Name} ({document.Id})");
        builder.AppendLine($"Section: {section.Title} ({section.Id})");
        builder.AppendLine($"Pages: {startPage}-{endPage}");
        builder.AppendLine("Use the attached section text as the source of truth. Call out uncertainty instead of inventing missing details.");
        return builder.ToString().Trim();
    }

    private static string BuildSectionReport(
        LibraryItem document,
        OutlineItem section,
        int startPage,
        int endPage,
        string prompt,
        string answer)
    {
        var builder = new StringBuilder();
        builder.AppendLine("# MSP Section Explanation");
        builder.AppendLine();
        builder.AppendLine($"- workflow: {ExplainSectionWorkflowName}");
        builder.AppendLine($"- document: {document.Name} ({document.Id})");
        builder.AppendLine($"- section: {section.Title} ({section.Id})");
        builder.AppendLine($"- pages: {startPage}-{endPage}");
        builder.AppendLine();
        builder.AppendLine("## Prompt");
        builder.AppendLine();
        builder.AppendLine(prompt);
        builder.AppendLine();
        builder.AppendLine("## Explanation");
        builder.AppendLine();
        builder.AppendLine(answer.Trim());
        return builder.ToString().TrimEnd();
    }

    private static string BuildEvidenceReviewReport(string evidencePath, EvidenceArtifactRecord evidence)
    {
        var builder = new StringBuilder();
        builder.AppendLine("# MSP Evidence Review");
        builder.AppendLine();
        builder.AppendLine($"- workflow: {ReviewEvidenceWorkflowName}");
        builder.AppendLine($"- evidence: {evidencePath}");
        builder.AppendLine($"- document: {evidence.Document.Name} ({evidence.Document.Id})");
        builder.AppendLine($"- section: {evidence.Section.Title} ({evidence.Section.Id})");
        builder.AppendLine($"- pages: {evidence.Section.StartPage}-{evidence.Section.EndPage}");
        builder.AppendLine($"- citations: {evidence.Pages.Count}");
        builder.AppendLine();
        builder.AppendLine("## Review Notes");
        builder.AppendLine();
        if (evidence.Pages.Count == 0)
        {
            builder.AppendLine("No evidence pages are available for review.");
        }
        else
        {
            foreach (var page in evidence.Pages)
            {
                builder.Append("- Page ");
                builder.Append(page.Page);
                builder.Append(" (`");
                builder.Append(page.SourcePath);
                builder.Append("`): ");
                builder.AppendLine(TrimSingleLine(page.Text, 220));
            }
        }

        builder.AppendLine();
        builder.AppendLine("## Citation Table");
        builder.AppendLine();
        builder.AppendLine("| page | source | textLength | excerpt |");
        builder.AppendLine("| ---: | --- | ---: | --- |");
        foreach (var page in evidence.Pages)
        {
            builder.Append("| ");
            builder.Append(page.Page);
            builder.Append(" | `");
            builder.Append(EscapeTableText(page.SourcePath));
            builder.Append("` | ");
            builder.Append(page.TextLength);
            builder.Append(" | ");
            builder.Append(EscapeTableText(TrimSingleLine(page.Text, 160)));
            builder.AppendLine(" |");
        }

        return builder.ToString().TrimEnd();
    }

    private LibraryItem? ResolveEvidenceDocument(EvidenceArtifactRecord evidence)
    {
        var workspace = workspaceProvider();
        return workspace is null || string.IsNullOrWhiteSpace(evidence.Document.Id)
            ? null
            : ReadOsMspCommandHelpers.ResolvePdfDocument(workspace, evidence.Document.Id, selectedDocumentProvider);
    }

    private static string BuildEvidenceSynthesisPrompt(string evidencePath, EvidenceArtifactRecord evidence)
    {
        var builder = new StringBuilder();
        builder.AppendLine("Synthesize the structured ReadOS evidence into a concise, source-grounded review.");
        builder.AppendLine("Use only the attached evidence text. Identify likely conclusions, caveats, and pages worth citing.");
        builder.AppendLine("Call out uncertainty when the evidence is insufficient.");
        builder.AppendLine();
        builder.AppendLine($"Evidence artifact: {evidencePath}");
        builder.AppendLine($"Document: {evidence.Document.Name} ({evidence.Document.Id})");
        builder.AppendLine($"Section: {evidence.Section.Title} ({evidence.Section.Id})");
        builder.AppendLine($"Pages: {evidence.Section.StartPage}-{evidence.Section.EndPage}");
        return builder.ToString().Trim();
    }

    private static string BuildEvidenceModelContext(string evidencePath, EvidenceArtifactRecord evidence)
    {
        var builder = new StringBuilder();
        builder.AppendLine($"# Evidence Artifact: {evidencePath}");
        builder.AppendLine($"Document: {evidence.Document.Name} ({evidence.Document.Id})");
        builder.AppendLine($"Section: {evidence.Section.Title} ({evidence.Section.Id})");
        builder.AppendLine($"Pages: {evidence.Section.StartPage}-{evidence.Section.EndPage}");
        builder.AppendLine();
        foreach (var page in evidence.Pages)
        {
            builder.AppendLine($"## Page {page.Page} ({page.SourcePath})");
            builder.AppendLine(page.Text);
            builder.AppendLine();
        }

        return builder.ToString().Trim();
    }

    private static string BuildEvidenceSynthesisReport(
        string evidencePath,
        EvidenceArtifactRecord evidence,
        string prompt,
        string synthesis)
    {
        var builder = new StringBuilder();
        builder.AppendLine("# MSP Evidence Synthesis");
        builder.AppendLine();
        builder.AppendLine($"- workflow: {SynthesizeEvidenceWorkflowName}");
        builder.AppendLine($"- evidence: {evidencePath}");
        builder.AppendLine($"- document: {evidence.Document.Name} ({evidence.Document.Id})");
        builder.AppendLine($"- section: {evidence.Section.Title} ({evidence.Section.Id})");
        builder.AppendLine($"- pages: {evidence.Section.StartPage}-{evidence.Section.EndPage}");
        builder.AppendLine($"- citations: {evidence.Pages.Count}");
        builder.AppendLine();
        builder.AppendLine("## Prompt");
        builder.AppendLine();
        builder.AppendLine(prompt);
        builder.AppendLine();
        builder.AppendLine("## Synthesis");
        builder.AppendLine();
        builder.AppendLine(synthesis.Trim());
        builder.AppendLine();
        builder.AppendLine("## Source Pages");
        builder.AppendLine();
        builder.AppendLine("| page | source | textLength | excerpt |");
        builder.AppendLine("| ---: | --- | ---: | --- |");
        foreach (var page in evidence.Pages)
        {
            builder.Append("| ");
            builder.Append(page.Page);
            builder.Append(" | `");
            builder.Append(EscapeTableText(page.SourcePath));
            builder.Append("` | ");
            builder.Append(page.TextLength);
            builder.Append(" | ");
            builder.Append(EscapeTableText(TrimSingleLine(page.Text, 160)));
            builder.AppendLine(" |");
        }

        return builder.ToString().TrimEnd();
    }

    private LibraryItem? ResolveArtifactDocument(MspArtifact? manifest)
    {
        var documentId = manifest?.SourceDocuments.FirstOrDefault();
        if (string.IsNullOrWhiteSpace(documentId))
        {
            return null;
        }

        var workspace = workspaceProvider();
        return workspace is null
            ? null
            : ReadOsMspCommandHelpers.ResolvePdfDocument(workspace, documentId, selectedDocumentProvider);
    }

    private static string BuildArtifactRefinementPrompt(
        string sourcePath,
        string instruction,
        MspArtifact? manifest)
    {
        var builder = new StringBuilder();
        builder.AppendLine("Refine the attached ReadOS artifact according to the operator instruction.");
        builder.AppendLine("Preserve source-grounded claims, keep citations/page references when present, and do not introduce facts not supported by the artifact.");
        builder.AppendLine();
        builder.AppendLine($"Source artifact: {sourcePath}");
        if (manifest is not null)
        {
            builder.AppendLine($"Source description: {manifest.Description}");
            if (manifest.SourcePages.Count > 0)
            {
                builder.AppendLine($"Source pages: {string.Join(", ", manifest.SourcePages)}");
            }
        }

        builder.AppendLine();
        builder.AppendLine("Instruction:");
        builder.AppendLine(instruction.Trim());
        return builder.ToString().Trim();
    }

    private static string BuildArtifactRefinementReport(
        string sourcePath,
        string instruction,
        string prompt,
        string refined)
    {
        var builder = new StringBuilder();
        builder.AppendLine("# MSP Artifact Refinement");
        builder.AppendLine();
        builder.AppendLine($"- workflow: {RefineArtifactWorkflowName}");
        builder.AppendLine($"- source: {sourcePath}");
        builder.AppendLine($"- instruction: {instruction.Trim()}");
        builder.AppendLine();
        builder.AppendLine("## Prompt");
        builder.AppendLine();
        builder.AppendLine(prompt);
        builder.AppendLine();
        builder.AppendLine("## Refined Output");
        builder.AppendLine();
        builder.AppendLine(refined.Trim());
        return builder.ToString().TrimEnd();
    }

    private static async ValueTask<MspArtifact?> ReadArtifactManifestAsync(
        MspCommandContext context,
        string artifactPath,
        ICollection<MspCommandDiagnostic> diagnostics,
        CancellationToken cancellationToken)
    {
        var manifestPath = GetArtifactManifestPath(artifactPath);
        var content = await context.Workspace.TryReadTextAsync(manifestPath, cancellationToken);
        if (content is null)
        {
            diagnostics.Add(new MspCommandDiagnostic
            {
                Severity = MspDiagnosticSeverity.Warning,
                Code = "reados.workflow.evidence_manifest_missing",
                Target = manifestPath,
                Message = "Evidence review could not read the evidence artifact manifest.",
                RecoveryHint = "Refresh artifact projections or rerun extract-evidence with --artifact before reviewing."
            });
            return null;
        }

        try
        {
            return JsonSerializer.Deserialize<MspArtifact>(content, EvidenceJsonOptions);
        }
        catch (JsonException)
        {
            diagnostics.Add(new MspCommandDiagnostic
            {
                Severity = MspDiagnosticSeverity.Warning,
                Code = "reados.workflow.evidence_manifest_invalid",
                Target = manifestPath,
                Message = "Evidence review could not parse the evidence artifact manifest.",
                RecoveryHint = "Inspect the artifact manifest and rerun extract-evidence before reviewing."
            });
            return null;
        }
    }

    private static bool TryParseEvidenceArtifact(
        string content,
        out EvidenceArtifactRecord evidence,
        out string error)
    {
        evidence = new EvidenceArtifactRecord(
            string.Empty,
            DateTimeOffset.MinValue,
            new EvidenceDocumentRecord(string.Empty, string.Empty),
            new EvidenceSectionRecord(string.Empty, string.Empty, 0, 0, 0),
            Array.Empty<EvidencePageRecord>());
        error = string.Empty;
        try
        {
            using var jsonDocument = JsonDocument.Parse(content);
            var root = jsonDocument.RootElement;
            var workflow = GetStringProperty(root, "workflow");
            if (!string.Equals(workflow, ExtractEvidenceWorkflowName, StringComparison.OrdinalIgnoreCase))
            {
                error = "Expected JSON generated by workflow run extract-evidence.";
                return false;
            }

            var document = root.GetProperty("document");
            var section = root.GetProperty("section");
            var pages = new List<EvidencePageRecord>();
            if (root.TryGetProperty("pages", out var pagesElement) &&
                pagesElement.ValueKind == JsonValueKind.Array)
            {
                foreach (var pageElement in pagesElement.EnumerateArray())
                {
                    var text = GetStringProperty(pageElement, "text");
                    var textLength = GetIntProperty(pageElement, "textLength");
                    pages.Add(new EvidencePageRecord(
                        GetIntProperty(pageElement, "page"),
                        GetStringProperty(pageElement, "sourcePath"),
                        textLength <= 0 ? text.Length : textLength,
                        text));
                }
            }

            evidence = new EvidenceArtifactRecord(
                workflow,
                GetDateTimeOffsetProperty(root, "generatedAt"),
                new EvidenceDocumentRecord(
                    GetStringProperty(document, "id"),
                    GetStringProperty(document, "name")),
                new EvidenceSectionRecord(
                    GetStringProperty(section, "id"),
                    GetStringProperty(section, "title"),
                    GetIntProperty(section, "level"),
                    GetIntProperty(section, "startPage"),
                    GetIntProperty(section, "endPage")),
                pages);
            return true;
        }
        catch (JsonException)
        {
            error = "Run workflow run extract-evidence again; the evidence artifact is not valid JSON.";
            return false;
        }
        catch (InvalidOperationException)
        {
            error = "Run workflow run extract-evidence again; the evidence artifact JSON has an unexpected shape.";
            return false;
        }
        catch (KeyNotFoundException)
        {
            error = "Run workflow run extract-evidence again; the evidence artifact JSON is missing required fields.";
            return false;
        }
    }

    private static IEnumerable<string> BuildReviewSourcePaths(
        string evidencePath,
        MspArtifact? manifest,
        EvidenceArtifactRecord evidence)
    {
        yield return evidencePath;
        yield return GetArtifactManifestPath(evidencePath);
        var sourcePaths = manifest?.SourcePaths.Count > 0
            ? manifest.SourcePaths
            : evidence.Pages.Select(page => page.SourcePath).ToArray();
        foreach (var path in sourcePaths.Where(path => !string.IsNullOrWhiteSpace(path)))
        {
            yield return path;
        }
    }

    private static IEnumerable<string> BuildArtifactDerivativeSourcePaths(
        string sourcePath,
        MspArtifact? manifest)
    {
        yield return sourcePath;
        yield return GetArtifactManifestPath(sourcePath);
        if (manifest is null)
        {
            yield break;
        }

        foreach (var path in manifest.SourcePaths.Where(path => !string.IsNullOrWhiteSpace(path)))
        {
            yield return path;
        }
    }

    private static IEnumerable<string> GetSourcePaths(string documentId, int startPage, int endPage)
    {
        for (var page = startPage; page <= endPage; page++)
        {
            yield return $"/documents/{documentId}/pages/{page}.txt";
        }
    }

    private static bool HasArtifactOutput(IEnumerable<string> arguments)
    {
        return arguments.Any(argument =>
            string.Equals(argument, "--artifact", StringComparison.OrdinalIgnoreCase) ||
            argument.StartsWith("--artifact=", StringComparison.OrdinalIgnoreCase));
    }

    private static bool TryNormalizeArtifactFilePath(string path, out string normalized)
    {
        normalized = NormalizeArtifactPath(path);
        return IsArtifactFilePath(normalized);
    }

    private static string NormalizeArtifactPath(string path)
    {
        if (path.StartsWith("/artifacts", StringComparison.Ordinal))
        {
            return path;
        }

        return "/artifacts/" + path.TrimStart('/');
    }

    private static bool IsArtifactFilePath(string path)
    {
        return path.StartsWith("/artifacts/", StringComparison.Ordinal);
    }

    private static string GetArtifactManifestPath(string artifactPath)
    {
        return artifactPath + ".manifest.json";
    }

    private static string EscapeTableText(string value)
    {
        return TrimSingleLine(value, 240).Replace("|", "\\|", StringComparison.Ordinal);
    }

    private static string TrimSingleLine(string value, int maxLength)
    {
        var normalized = value
            .Replace("\r", " ", StringComparison.Ordinal)
            .Replace("\n", " ", StringComparison.Ordinal)
            .Trim();
        return normalized.Length <= maxLength ? normalized : normalized[..maxLength] + "...";
    }

    private static string GetStringProperty(JsonElement element, string name)
    {
        return element.TryGetProperty(name, out var value)
            ? value.GetString() ?? string.Empty
            : string.Empty;
    }

    private static int GetIntProperty(JsonElement element, string name)
    {
        return element.TryGetProperty(name, out var value) && value.TryGetInt32(out var number)
            ? number
            : 0;
    }

    private static DateTimeOffset GetDateTimeOffsetProperty(JsonElement element, string name)
    {
        return element.TryGetProperty(name, out var value) &&
            value.TryGetDateTimeOffset(out var timestamp)
            ? timestamp
            : DateTimeOffset.MinValue;
    }

    private sealed record DocumentWorkflowArguments(
        string DocumentSelector,
        string OutlineSelector,
        string? ArtifactPath);

    private sealed record ReviewEvidenceArguments(
        string EvidencePath,
        string? ArtifactPath);

    private sealed record RefineArtifactArguments(
        string SourcePath,
        string Instruction,
        string? ArtifactPath);

    private sealed record EvidenceArtifactRecord(
        string Workflow,
        DateTimeOffset GeneratedAt,
        EvidenceDocumentRecord Document,
        EvidenceSectionRecord Section,
        IReadOnlyList<EvidencePageRecord> Pages);

    private sealed record EvidenceDocumentRecord(string Id, string Name);

    private sealed record EvidenceSectionRecord(
        string Id,
        string Title,
        int Level,
        int StartPage,
        int EndPage);

    private sealed record EvidencePageRecord(
        int Page,
        string SourcePath,
        int TextLength,
        string Text);

    private sealed record OutlineEntry(OutlineItem Item, int Index);
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

    public MspCommandPreview GetPreview(IReadOnlyList<string> arguments)
    {
        if (arguments.Count < 4 || !string.Equals(arguments[0], "set", StringComparison.OrdinalIgnoreCase))
        {
            return MspCommandPreview.Create("Set a PDF page label.");
        }

        return MspCommandPreview.Create(
            "Set a PDF page label.",
            new[] { ReadOsMspCommandHelpers.DescribePdfTarget(workspaceProvider(), arguments[1], selectedDocumentProvider) },
            new[]
            {
                $"page: {arguments[2]}",
                $"label: {string.Join(' ', arguments.Skip(3)).Trim()}"
            });
    }

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
            return ReadOsMspCommandHelpers.PdfDocumentNotFound(arguments[1]);
        }

        if (!ReadOsMspCommandHelpers.TryValidatePdfPage(document, arguments[2], out var page, out var pageFailure))
        {
            return pageFailure;
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

    public MspCommandPreview GetPreview(IReadOnlyList<string> arguments)
    {
        if (arguments.Count == 0)
        {
            return MspCommandPreview.Create("Modify PDF outline entries.");
        }

        return arguments[0].ToLowerInvariant() switch
        {
            "add" => PreviewAdd(arguments.Skip(1).ToArray()),
            "delete" => PreviewDelete(arguments.Skip(1).ToArray()),
            _ => MspCommandPreview.Create("Modify PDF outline entries.")
        };
    }

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
            return ReadOsMspCommandHelpers.PdfDocumentNotFound(arguments[0]);
        }

        if (!ReadOsMspCommandHelpers.TryValidatePdfPage(document, arguments[1], out var page, out var pageFailure))
        {
            return pageFailure;
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

    private MspCommandPreview PreviewAdd(IReadOnlyList<string> arguments)
    {
        if (arguments.Count < 3)
        {
            return MspCommandPreview.Create("Add a PDF outline entry.");
        }

        var titleParts = new List<string>();
        var level = "1";
        for (var index = 2; index < arguments.Count; index++)
        {
            if (string.Equals(arguments[index], "--level", StringComparison.OrdinalIgnoreCase) && index + 1 < arguments.Count)
            {
                level = arguments[index + 1];
                index++;
                continue;
            }

            titleParts.Add(arguments[index]);
        }

        return MspCommandPreview.Create(
            "Add a PDF outline entry.",
            new[] { ReadOsMspCommandHelpers.DescribePdfTarget(workspaceProvider(), arguments[0], selectedDocumentProvider) },
            new[]
            {
                $"page: {arguments[1]}",
                $"level: {level}",
                $"title: {string.Join(' ', titleParts).Trim()}"
            });
    }

    private MspCommandPreview PreviewDelete(IReadOnlyList<string> arguments)
    {
        if (arguments.Count < 2)
        {
            return MspCommandPreview.Create("Delete a PDF outline entry.");
        }

        return MspCommandPreview.Create(
            "Delete a PDF outline entry.",
            new[] { ReadOsMspCommandHelpers.DescribePdfTarget(workspaceProvider(), arguments[0], selectedDocumentProvider) },
            new[] { $"selector: {string.Join(' ', arguments.Skip(1)).Trim()}" });
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
            return ReadOsMspCommandHelpers.PdfDocumentNotFound(arguments[0]);
        }

        var selector = string.Join(' ', arguments.Skip(1)).Trim();
        var item = document.Outline.FirstOrDefault(item =>
            string.Equals(item.Id, selector, StringComparison.OrdinalIgnoreCase) ||
            string.Equals(item.Title, selector, StringComparison.OrdinalIgnoreCase));
        if (item is null)
        {
            return ReadOsMspCommandHelpers.OutlineItemNotFound(document, selector);
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

    public MspCommandPreview GetPreview(IReadOnlyList<string> arguments)
    {
        if (arguments.Count == 0)
        {
            return MspCommandPreview.Create("Queue evidence for the current chat.");
        }

        return arguments[0].ToLowerInvariant() switch
        {
            "page" when arguments.Count >= 3 => MspCommandPreview.Create(
                "Queue a PDF page as chat evidence.",
                new[] { ReadOsMspCommandHelpers.DescribePdfTarget(workspaceProvider(), arguments[1], selectedDocumentProvider) },
                new[] { $"page: {arguments[2]}" }),
            "range" when arguments.Count >= 4 => MspCommandPreview.Create(
                "Queue a PDF page range as chat evidence.",
                new[] { ReadOsMspCommandHelpers.DescribePdfTarget(workspaceProvider(), arguments[1], selectedDocumentProvider) },
                new[] { $"pages: {arguments[2]}-{arguments[3]}" }),
            _ => MspCommandPreview.Create("Queue evidence for the current chat.")
        };
    }

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
            return ReadOsMspCommandHelpers.PdfDocumentNotFound(arguments[0]);
        }

        if (!ReadOsMspCommandHelpers.TryValidatePdfPage(resolved, arguments[1], out var page, out var pageFailure))
        {
            return pageFailure;
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
            return ReadOsMspCommandHelpers.PdfDocumentNotFound(arguments[0]);
        }

        if (!ReadOsMspCommandHelpers.TryValidatePdfPage(resolved, arguments[1], out var startPage, out var startPageFailure))
        {
            return startPageFailure;
        }

        if (!ReadOsMspCommandHelpers.TryValidatePdfPage(resolved, arguments[2], out var endPage, out var endPageFailure))
        {
            return endPageFailure;
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

    private static string Usage()
    {
        return "Usage: attach page [current|documentId] <page> | attach range [current|documentId] <startPage> <endPage>";
    }
}

internal sealed class ReadOsChatCommand : IMspCommand
{
    private readonly IWorkspaceStore workspaceStore;
    private readonly IAiChatService aiChatService;
    private readonly Func<WorkspaceState?> workspaceProvider;
    private readonly Func<WorkspaceSettings> settingsProvider;
    private readonly Func<LibraryItem?> selectedDocumentProvider;
    private readonly Func<IReadOnlyList<ChatAttachment>> pendingAttachmentsProvider;
    private readonly Func<ChatAttachment, Task<string>> attachmentTextProvider;
    private readonly Action clearAttachments;
    private readonly Action<LibraryItem, ChatConversation> chatResultSink;

    public ReadOsChatCommand(
        IWorkspaceStore workspaceStore,
        IAiChatService aiChatService,
        Func<WorkspaceState?> workspaceProvider,
        Func<WorkspaceSettings> settingsProvider,
        Func<LibraryItem?> selectedDocumentProvider,
        Func<IReadOnlyList<ChatAttachment>> pendingAttachmentsProvider,
        Func<ChatAttachment, Task<string>> attachmentTextProvider,
        Action clearAttachments,
        Action<LibraryItem, ChatConversation> chatResultSink)
    {
        this.workspaceStore = workspaceStore;
        this.aiChatService = aiChatService;
        this.workspaceProvider = workspaceProvider;
        this.settingsProvider = settingsProvider;
        this.selectedDocumentProvider = selectedDocumentProvider;
        this.pendingAttachmentsProvider = pendingAttachmentsProvider;
        this.attachmentTextProvider = attachmentTextProvider;
        this.clearAttachments = clearAttachments;
        this.chatResultSink = chatResultSink;
    }

    public string Name => "chat";

    public string Summary => "Ask the ReadOS chat model using current evidence attachments.";

    public MspCommandMetadata Metadata => MspCommandMetadata.Create(
        Name,
        Summary,
        "chat ask [current|documentId] <prompt...>",
        MspCommandEffects.ReadWorkspace | MspCommandEffects.WriteWorkspace | MspCommandEffects.ExternalModel,
        new[] { "reados.chat.ask" });

    public MspCommandMetadata GetMetadata(IReadOnlyList<string> arguments)
    {
        if (arguments.Count > 0 &&
            string.Equals(arguments[0], "ask", StringComparison.OrdinalIgnoreCase) &&
            HasArtifactOutput(arguments.Skip(1)))
        {
            return MspCommandMetadata.Create(
                Name,
                "Ask the ReadOS chat model and write the answer into a durable artifact.",
                "chat ask [current|documentId] <prompt...> --artifact <path>",
                MspCommandEffects.ReadWorkspace |
                MspCommandEffects.WriteWorkspace |
                MspCommandEffects.CreateArtifact |
                MspCommandEffects.ExternalModel,
                new[] { "reados.chat.ask", "msp.artifact.write" });
        }

        return Metadata;
    }

    public MspCommandPreview GetPreview(IReadOnlyList<string> arguments)
    {
        if (arguments.Count < 3 || !string.Equals(arguments[0], "ask", StringComparison.OrdinalIgnoreCase))
        {
            return MspCommandPreview.Create("Ask the configured chat model.");
        }

        if (!TryParseChatAskArguments(arguments.Skip(1).ToArray(), out var parsed, out _))
        {
            return MspCommandPreview.Create("Ask the configured chat model.");
        }

        var attachments = pendingAttachmentsProvider();
        var targets = new List<string>
        {
            ReadOsMspCommandHelpers.DescribePdfTarget(workspaceProvider(), parsed.DocumentSelector, selectedDocumentProvider)
        };
        if (!string.IsNullOrWhiteSpace(parsed.ArtifactPath))
        {
            targets.Add(NormalizeArtifactPath(parsed.ArtifactPath));
        }

        return MspCommandPreview.Create(
            string.IsNullOrWhiteSpace(parsed.ArtifactPath)
                ? "Ask the configured chat model and write the exchange to the document conversation."
                : "Ask the configured chat model, write the exchange, and save the answer as an artifact.",
            targets,
            new[]
            {
                $"prompt: {parsed.Prompt}",
                $"queuedAttachments: {attachments.Count}",
                $"provider: {settingsProvider().ProviderName}",
                $"model: {settingsProvider().ModelName}"
            });
    }

    public async ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default)
    {
        if (arguments.Count < 3 || !string.Equals(arguments[0], "ask", StringComparison.OrdinalIgnoreCase))
        {
            return MspCommandResult.Failure("Usage: chat ask [current|documentId] <prompt...> [--artifact <path>]", exitCode: 2);
        }

        var workspace = workspaceProvider();
        if (workspace is null)
        {
            return MspCommandResult.Failure("ReadOS workspace is not loaded.");
        }

        if (!TryParseChatAskArguments(arguments.Skip(1).ToArray(), out var parsed, out var parseError))
        {
            return MspCommandResult.Failure(parseError, exitCode: 2);
        }

        var document = ReadOsMspCommandHelpers.ResolvePdfDocument(workspace, parsed.DocumentSelector, selectedDocumentProvider);
        if (document is null)
        {
            return ReadOsMspCommandHelpers.PdfDocumentNotFound(parsed.DocumentSelector);
        }

        string? artifactPath = null;
        if (!string.IsNullOrWhiteSpace(parsed.ArtifactPath))
        {
            artifactPath = NormalizeArtifactPath(parsed.ArtifactPath);
            if (!IsArtifactFilePath(artifactPath))
            {
                return MspCommandResult.Failure("chat ask --artifact must target a file under /artifacts.", exitCode: 2);
            }
        }

        var existingConversation = FindConversation(document);
        var history = existingConversation?.Messages.ToArray() ?? Array.Empty<ChatMessage>();
        var attachments = pendingAttachmentsProvider()
            .Select(CloneAttachment)
            .ToArray();
        var settings = settingsProvider();
        await context.ReportProgressAsync(
            $"Preparing chat request with {attachments.Length} attachments.",
            15,
            cancellationToken);
        await context.ReportProgressAsync(
            $"Calling chat model {settings.ModelName}.",
            45,
            cancellationToken);
        string answer;
        try
        {
            answer = await aiChatService.SendAsync(
                settings,
                document,
                history,
                parsed.Prompt,
                attachments,
                attachmentTextProvider,
                allowMspCommandRequests: false,
                cancellationToken: cancellationToken);
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            return ReadOsMspCommandHelpers.ChatModelProviderFailed(settings, ex);
        }

        await context.ReportProgressAsync(
            "Writing chat answer to the document conversation.",
            80,
            cancellationToken);
        var conversation = existingConversation ?? CreateConversation(document);
        var userMessage = new ChatMessage
        {
            Role = ChatRole.User,
            Author = "MSP",
            Content = parsed.Prompt,
            CreatedAt = DateTimeOffset.Now
        };
        foreach (var attachment in attachments)
        {
            userMessage.Attachments.Add(attachment);
        }

        var assistantMessage = new ChatMessage
        {
            Role = ChatRole.Assistant,
            Author = "ReadOS",
            Content = answer,
            CreatedAt = DateTimeOffset.Now
        };

        conversation.Messages.Add(userMessage);
        conversation.Messages.Add(assistantMessage);
        conversation.UpdatedAt = DateTimeOffset.Now;
        document.UpdatedAt = DateTimeOffset.Now;
        clearAttachments();
        await workspaceStore.SaveAsync(workspace, cancellationToken);
        chatResultSink(document, conversation);
        await context.ReportProgressAsync(
            $"Chat answer saved to conversation {conversation.Id}.",
            95,
            cancellationToken);

        MspArtifact[] artifacts = Array.Empty<MspArtifact>();
        if (!string.IsNullOrWhiteSpace(artifactPath))
        {
            var artifactContent = BuildChatArtifactContent(document, conversation, userMessage, assistantMessage, attachments);
            var sourceDocuments = GetAttachmentSourceDocuments(attachments, document.Id).ToArray();
            var sourcePages = GetAttachmentSourcePages(attachments).ToArray();
            var sourcePaths = GetAttachmentSourcePaths(attachments).ToArray();
            var artifact = new MspArtifact
            {
                Path = artifactPath,
                MediaType = "text/markdown",
                SizeBytes = artifactContent.Length,
                Description = $"Chat answer generated for {document.Name}.",
                SourceCommand = context.Invocation.CommandText,
                Actor = context.Invocation.Actor,
                SessionId = context.Invocation.SessionId,
                SourceDocuments = sourceDocuments,
                SourcePages = sourcePages,
                SourcePaths = sourcePaths,
                Preview = $"document: {document.Name}; conversation: {conversation.Id}; attachments: {attachments.Length}; contentLength: {artifactContent.Length}"
            };
            await context.Workspace.WriteTextAsync(artifactPath, artifactContent, artifact, cancellationToken);
            artifacts = new[] { artifact };
        }

        var builder = new StringBuilder();
        builder.Append("chat-answer\t");
        builder.Append(document.Id);
        builder.Append('\t');
        builder.Append(conversation.Id);
        builder.Append('\t');
        builder.AppendLine(assistantMessage.Id);
        builder.AppendLine(answer);
        if (artifacts.Length > 0)
        {
            builder.Append("artifact\t");
            builder.AppendLine(artifacts[0].Path);
        }

        return MspCommandResult.Success(builder.ToString(), artifacts);
    }

    private static bool HasArtifactOutput(IEnumerable<string> arguments)
    {
        return arguments.Any(argument =>
            string.Equals(argument, "--artifact", StringComparison.OrdinalIgnoreCase) ||
            argument.StartsWith("--artifact=", StringComparison.OrdinalIgnoreCase));
    }

    private static bool TryParseChatAskArguments(
        IReadOnlyList<string> arguments,
        out ChatAskArguments parsed,
        out string error)
    {
        parsed = new ChatAskArguments(string.Empty, string.Empty, null);
        error = string.Empty;
        if (arguments.Count < 2)
        {
            error = "Usage: chat ask [current|documentId] <prompt...> [--artifact <path>]";
            return false;
        }

        var promptParts = new List<string>();
        string? artifactPath = null;
        var index = 1;
        while (index < arguments.Count)
        {
            var argument = arguments[index];
            if (string.Equals(argument, "--artifact", StringComparison.OrdinalIgnoreCase))
            {
                if (index + 1 >= arguments.Count || string.IsNullOrWhiteSpace(arguments[index + 1]))
                {
                    error = "--artifact requires a path.";
                    return false;
                }

                artifactPath = arguments[index + 1];
                index += 2;
                continue;
            }

            if (argument.StartsWith("--artifact=", StringComparison.OrdinalIgnoreCase))
            {
                artifactPath = argument["--artifact=".Length..];
                if (string.IsNullOrWhiteSpace(artifactPath))
                {
                    error = "--artifact requires a path.";
                    return false;
                }

                index++;
                continue;
            }

            promptParts.Add(argument);
            index++;
        }

        var prompt = string.Join(' ', promptParts).Trim();
        if (string.IsNullOrWhiteSpace(prompt))
        {
            error = "prompt must not be empty.";
            return false;
        }

        parsed = new ChatAskArguments(arguments[0], prompt, artifactPath);
        return true;
    }

    private static string NormalizeArtifactPath(string path)
    {
        if (path.StartsWith("/artifacts", StringComparison.Ordinal))
        {
            return path;
        }

        return "/artifacts/" + path.TrimStart('/');
    }

    private static bool IsArtifactFilePath(string path)
    {
        return path.StartsWith("/artifacts/", StringComparison.Ordinal);
    }

    private static string BuildChatArtifactContent(
        LibraryItem document,
        ChatConversation conversation,
        ChatMessage userMessage,
        ChatMessage assistantMessage,
        IReadOnlyList<ChatAttachment> attachments)
    {
        var builder = new StringBuilder();
        builder.AppendLine("# MSP Chat Answer");
        builder.AppendLine();
        builder.AppendLine($"Document: {document.Name} ({document.Id})");
        builder.AppendLine($"Conversation: {conversation.Id}");
        builder.AppendLine($"Question: {userMessage.Content}");
        if (attachments.Count > 0)
        {
            builder.AppendLine();
            builder.AppendLine("Evidence:");
            foreach (var attachment in attachments)
            {
                builder.Append("- ");
                builder.AppendLine(attachment.Detail);
            }
        }

        builder.AppendLine();
        builder.AppendLine("Answer:");
        builder.AppendLine(assistantMessage.Content);
        return builder.ToString();
    }

    private static IEnumerable<string> GetAttachmentSourceDocuments(
        IEnumerable<ChatAttachment> attachments,
        string fallbackDocumentId)
    {
        var documentIds = attachments
            .Select(attachment => attachment.DocumentId)
            .Where(id => !string.IsNullOrWhiteSpace(id))
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .ToArray();
        return documentIds.Length == 0 ? new[] { fallbackDocumentId } : documentIds;
    }

    private static IEnumerable<string> GetAttachmentSourcePages(IEnumerable<ChatAttachment> attachments)
    {
        return attachments
            .Where(attachment => !string.IsNullOrWhiteSpace(attachment.DocumentId) && attachment.StartPage > 0)
            .Select(attachment =>
            {
                var endPage = attachment.EndPage <= 0 ? attachment.StartPage : attachment.EndPage;
                return attachment.StartPage == endPage
                    ? $"{attachment.DocumentId}:{attachment.StartPage}"
                    : $"{attachment.DocumentId}:{Math.Min(attachment.StartPage, endPage)}-{Math.Max(attachment.StartPage, endPage)}";
            })
            .Distinct(StringComparer.OrdinalIgnoreCase);
    }

    private static IEnumerable<string> GetAttachmentSourcePaths(IEnumerable<ChatAttachment> attachments)
    {
        foreach (var attachment in attachments.Where(attachment => !string.IsNullOrWhiteSpace(attachment.DocumentId) && attachment.StartPage > 0))
        {
            var endPage = attachment.EndPage <= 0 ? attachment.StartPage : attachment.EndPage;
            var start = Math.Min(attachment.StartPage, endPage);
            var end = Math.Max(attachment.StartPage, endPage);
            for (var page = start; page <= end; page++)
            {
                yield return $"/documents/{attachment.DocumentId}/pages/{page}.txt";
            }
        }
    }

    private static ChatAttachment CloneAttachment(ChatAttachment source)
    {
        return new ChatAttachment
        {
            Kind = source.Kind,
            DocumentId = source.DocumentId,
            Title = source.Title,
            StartPage = source.StartPage,
            EndPage = source.EndPage,
            FilePath = source.FilePath,
            RegionX = source.RegionX,
            RegionY = source.RegionY,
            RegionWidth = source.RegionWidth,
            RegionHeight = source.RegionHeight
        };
    }

    private static ChatConversation? FindConversation(LibraryItem document)
    {
        return document.Conversations.OrderByDescending(item => item.UpdatedAt).FirstOrDefault();
    }

    private static ChatConversation CreateConversation(LibraryItem document)
    {
        var conversation = new ChatConversation
        {
            DocumentId = document.Id,
            Title = "MSP 问答",
            UpdatedAt = DateTimeOffset.Now
        };
        document.Conversations.Insert(0, conversation);
        return conversation;
    }

    private sealed record ChatAskArguments(
        string DocumentSelector,
        string Prompt,
        string? ArtifactPath);
}

internal static class ReadOsMspCommandHelpers
{
    public static MspCommandResult PdfDocumentNotFound(string selector)
    {
        var target = string.IsNullOrWhiteSpace(selector) ? "current" : selector;
        return MspCommandResult.Failure(
            $"PDF document not found: {target}",
            code: "reados.pdf.document_not_found",
            target: target,
            recoveryHint: "Run library list to find a PDF document id, import or select a PDF, or use current only when a PDF is selected.");
    }

    public static MspCommandResult ChatModelProviderFailed(WorkspaceSettings settings, Exception exception)
    {
        var provider = string.IsNullOrWhiteSpace(settings.ProviderName) ? "unknown-provider" : settings.ProviderName;
        var model = string.IsNullOrWhiteSpace(settings.ModelName) ? "unknown-model" : settings.ModelName;
        var target = $"{provider}/{model}";
        return MspCommandResult.Failure(
            $"Chat model provider failed: {exception.Message}",
            code: "reados.chat.model_provider_failed",
            target: target,
            recoveryHint: "Check the chat provider base URL, API key, model name, and network access, or enable offline responses before retrying chat ask.");
    }

    public static bool TryValidatePdfPage(
        LibraryItem document,
        string value,
        out int page,
        out MspCommandResult failure)
    {
        if (!int.TryParse(value, out page))
        {
            failure = MspCommandResult.Failure(
                $"PDF page must be a number: {value}",
                exitCode: 2,
                code: "reados.pdf.invalid_page",
                target: $"{document.Id}:{value}",
                recoveryHint: "Use a numeric 1-based PDF page index. Run pdf inspect current to check the document page count.");
            return false;
        }

        if (page < 1 || page > document.PageCount)
        {
            failure = MspCommandResult.Failure(
                $"PDF page is outside document range: {page} (1-{document.PageCount})",
                exitCode: 2,
                code: "reados.pdf.invalid_page",
                target: $"{document.Id}:{page}",
                recoveryHint: "Run pdf inspect current to check PageCount, then retry with a page inside the document range.");
            return false;
        }

        failure = MspCommandResult.Success();
        return true;
    }

    public static MspCommandResult OutlineItemNotFound(LibraryItem document, string selector)
    {
        return MspCommandResult.Failure(
            $"Outline item not found: {selector}",
            code: "reados.pdf.outline_item_not_found",
            target: $"{document.Id}:{selector}",
            recoveryHint: "Run pdf inspect current to list outline ids and titles, then retry with an existing outline selector.");
    }

    public static string DescribePdfTarget(
        WorkspaceState? workspace,
        string value,
        Func<LibraryItem?> selectedDocumentProvider)
    {
        var document = workspace is null
            ? null
            : ResolvePdfDocument(workspace, value, selectedDocumentProvider);
        return document is null
            ? value
            : $"{document.Name} ({document.Id})";
    }

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
