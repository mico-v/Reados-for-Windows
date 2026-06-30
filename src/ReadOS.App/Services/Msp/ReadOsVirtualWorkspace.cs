using System.Text.Json;
using ReadOS.App.Models;
using ReadOS.Msp.Workspace;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsVirtualWorkspace : IMspWorkspace
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        WriteIndented = true
    };

    private readonly IWorkspaceStore workspaceStore;
    private readonly IPdfDocumentService pdfService;
    private readonly Func<WorkspaceState?> workspaceProvider;

    public ReadOsVirtualWorkspace(
        IWorkspaceStore workspaceStore,
        IPdfDocumentService pdfService,
        Func<WorkspaceState?> workspaceProvider)
    {
        this.workspaceStore = workspaceStore;
        this.pdfService = pdfService;
        this.workspaceProvider = workspaceProvider;
    }

    public string NormalizePath(string path, string workingDirectory = "/")
    {
        return MspPathUtility.Normalize(path, workingDirectory);
    }

    public async ValueTask<bool> ExistsAsync(string path, CancellationToken cancellationToken = default)
    {
        var normalized = NormalizePath(path);
        if (normalized is "/" or "/projects" or "/library" or "/documents" or "/artifacts" or "/settings.json")
        {
            return workspaceProvider() is not null;
        }

        if ((await ListAsync(normalized, cancellationToken)).Count > 0)
        {
            return true;
        }

        return await TryReadTextAsync(normalized, cancellationToken) is not null;
    }

    public ValueTask<IReadOnlyList<MspWorkspaceEntry>> ListAsync(string path, CancellationToken cancellationToken = default)
    {
        var workspace = workspaceProvider();
        if (workspace is null)
        {
            return ValueTask.FromResult<IReadOnlyList<MspWorkspaceEntry>>(Array.Empty<MspWorkspaceEntry>());
        }

        var normalized = NormalizePath(path);
        IReadOnlyList<MspWorkspaceEntry> entries = normalized switch
        {
            "/" => RootEntries(),
            "/projects" => workspace.Projects
                .Select(project => Directory($"/projects/{project.Id}", project.Id))
                .ToArray(),
            "/library" => Documents(workspace)
                .Select(document => File($"/library/{document.Id}.json", $"{document.Id}.json", EstimateDocumentInfoSize(document), "application/json"))
                .ToArray(),
            "/documents" => Documents(workspace)
                .Select(document => Directory($"/documents/{document.Id}", document.Id))
                .ToArray(),
            "/artifacts" => ListArtifactEntries(normalized, workspace),
            _ => ListNested(normalized, workspace)
        };

        return ValueTask.FromResult(entries);
    }

    public async ValueTask<string?> TryReadTextAsync(string path, CancellationToken cancellationToken = default)
    {
        var workspace = workspaceProvider();
        if (workspace is null)
        {
            return null;
        }

        var normalized = NormalizePath(path);
        if (normalized == "/settings.json")
        {
            return JsonSerializer.Serialize(ProjectSettings(workspace.Settings), JsonOptions);
        }

        if (normalized.StartsWith("/artifacts/", StringComparison.Ordinal))
        {
            var artifact = workspace.Artifacts.FirstOrDefault(item =>
                string.Equals(item.Path, normalized, StringComparison.OrdinalIgnoreCase));
            return artifact?.Content;
        }

        if (normalized.StartsWith("/library/", StringComparison.Ordinal) && normalized.EndsWith(".json", StringComparison.Ordinal))
        {
            var documentId = Path.GetFileNameWithoutExtension(normalized);
            var document = FindDocument(workspace, documentId);
            return document is null ? null : JsonSerializer.Serialize(ProjectDocument(document), JsonOptions);
        }

        var parts = normalized.Split('/', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length < 2)
        {
            return null;
        }

        if (parts[0] == "projects")
        {
            var project = workspace.Projects.FirstOrDefault(item => item.Id == parts[1]);
            if (project is null)
            {
                return null;
            }

            if (parts.Length == 3 && parts[2] == "info.json")
            {
                return JsonSerializer.Serialize(ProjectInfo(project), JsonOptions);
            }
        }

        if (parts[0] != "documents")
        {
            return null;
        }

        var doc = FindDocument(workspace, parts[1]);
        if (doc is null)
        {
            return null;
        }

        if (parts.Length == 3 && parts[2] == "info.json")
        {
            return JsonSerializer.Serialize(ProjectDocument(doc), JsonOptions);
        }

        if (parts.Length == 3 && parts[2] == "outline.json")
        {
            return JsonSerializer.Serialize(doc.Outline.Select(ProjectOutline), JsonOptions);
        }

        if (parts.Length == 4 && parts[2] == "pages" && parts[3].EndsWith(".txt", StringComparison.Ordinal))
        {
            if (doc.Kind != LibraryItemKind.Pdf)
            {
                return null;
            }

            var pageText = Path.GetFileNameWithoutExtension(parts[3]);
            if (!int.TryParse(pageText, out var pageNumber) || pageNumber < 1 || pageNumber > doc.PageCount)
            {
                return null;
            }

            return await pdfService.ExtractPageTextAsync(
                workspaceStore.GetAbsolutePath(doc),
                pageNumber,
                pageNumber,
                cancellationToken);
        }

        if (parts.Length == 4 && parts[2] == "conversations" && parts[3].EndsWith(".json", StringComparison.Ordinal))
        {
            var conversationId = Path.GetFileNameWithoutExtension(parts[3]);
            var conversation = doc.Conversations.FirstOrDefault(item => item.Id == conversationId);
            return conversation is null ? null : JsonSerializer.Serialize(ProjectConversation(conversation), JsonOptions);
        }

        return null;
    }

    public async ValueTask WriteTextAsync(string path, string content, CancellationToken cancellationToken = default)
    {
        var workspace = workspaceProvider();
        if (workspace is null)
        {
            throw new InvalidOperationException("ReadOS workspace is not loaded.");
        }

        var normalized = NormalizePath(path);
        if (!normalized.StartsWith("/artifacts/", StringComparison.Ordinal) ||
            normalized.EndsWith("/", StringComparison.Ordinal))
        {
            throw new NotSupportedException("ReadOS virtual workspace only supports writing under /artifacts in this MSP phase.");
        }

        var now = DateTimeOffset.Now;
        var artifact = workspace.Artifacts.FirstOrDefault(item =>
            string.Equals(item.Path, normalized, StringComparison.OrdinalIgnoreCase));
        if (artifact is null)
        {
            artifact = new WorkspaceArtifact
            {
                Path = normalized,
                CreatedAt = now
            };
            workspace.Artifacts.Add(artifact);
        }

        artifact.Content = content;
        artifact.MediaType = GuessMediaType(normalized);
        artifact.UpdatedAt = now;
        await workspaceStore.SaveAsync(workspace, cancellationToken);
    }

    private static IReadOnlyList<MspWorkspaceEntry> RootEntries()
    {
        return new[]
        {
            Directory("/projects", "projects"),
            Directory("/library", "library"),
            Directory("/documents", "documents"),
            Directory("/artifacts", "artifacts"),
            File("/settings.json", "settings.json", null, "application/json")
        };
    }

    private static IReadOnlyList<MspWorkspaceEntry> ListNested(string normalized, WorkspaceState workspace)
    {
        var parts = normalized.Split('/', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length == 2 && parts[0] == "projects")
        {
            var project = workspace.Projects.FirstOrDefault(item => item.Id == parts[1]);
            return project is null
                ? Array.Empty<MspWorkspaceEntry>()
                : new[] { File($"{normalized}/info.json", "info.json", null, "application/json") };
        }

        if (parts.Length == 2 && parts[0] == "documents")
        {
            var document = FindDocument(workspace, parts[1]);
            return document is null
                ? Array.Empty<MspWorkspaceEntry>()
                : new[]
                {
                    File($"{normalized}/info.json", "info.json", EstimateDocumentInfoSize(document), "application/json"),
                    File($"{normalized}/outline.json", "outline.json", null, "application/json"),
                    Directory($"{normalized}/pages", "pages"),
                    Directory($"{normalized}/conversations", "conversations")
                };
        }

        if (parts.Length == 3 && parts[0] == "documents" && parts[2] == "pages")
        {
            var document = FindDocument(workspace, parts[1]);
            if (document is null || document.Kind != LibraryItemKind.Pdf)
            {
                return Array.Empty<MspWorkspaceEntry>();
            }

            return Enumerable.Range(1, document.PageCount)
                .Select(page => File($"{normalized}/{page}.txt", $"{page}.txt", null, "text/plain"))
                .ToArray();
        }

        if (parts.Length == 3 && parts[0] == "documents" && parts[2] == "conversations")
        {
            var document = FindDocument(workspace, parts[1]);
            if (document is null)
            {
                return Array.Empty<MspWorkspaceEntry>();
            }

            return document.Conversations
                .Select(conversation => File($"{normalized}/{conversation.Id}.json", $"{conversation.Id}.json", null, "application/json"))
                .ToArray();
        }

        if (parts.Length >= 1 && parts[0] == "artifacts")
        {
            return ListArtifactEntries(normalized, workspace);
        }

        return Array.Empty<MspWorkspaceEntry>();
    }

    private static IReadOnlyList<MspWorkspaceEntry> ListArtifactEntries(string normalized, WorkspaceState workspace)
    {
        var entries = new Dictionary<string, MspWorkspaceEntry>(StringComparer.OrdinalIgnoreCase);
        var prefix = normalized == "/artifacts" ? "/artifacts/" : normalized.TrimEnd('/') + "/";
        foreach (var artifact in workspace.Artifacts)
        {
            if (!artifact.Path.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
            {
                continue;
            }

            var remainder = artifact.Path[prefix.Length..];
            if (string.IsNullOrWhiteSpace(remainder))
            {
                continue;
            }

            var slashIndex = remainder.IndexOf('/');
            if (slashIndex >= 0)
            {
                var directoryName = remainder[..slashIndex];
                var directoryPath = prefix.TrimEnd('/') + "/" + directoryName;
                entries.TryAdd(directoryPath, Directory(directoryPath, directoryName));
            }
            else
            {
                entries.TryAdd(artifact.Path, File(
                    artifact.Path,
                    Path.GetFileName(artifact.Path),
                    artifact.SizeBytes,
                    artifact.MediaType));
            }
        }

        return entries.Values
            .OrderBy(item => item.IsDirectory ? 0 : 1)
            .ThenBy(item => item.Name, StringComparer.OrdinalIgnoreCase)
            .ToArray();
    }

    private static IEnumerable<LibraryItem> Documents(WorkspaceState workspace)
    {
        return workspace.Projects.SelectMany(project => project.LibraryItems).Where(item => item.Kind != LibraryItemKind.Folder);
    }

    private static LibraryItem? FindDocument(WorkspaceState workspace, string documentId)
    {
        return Documents(workspace).FirstOrDefault(item => string.Equals(item.Id, documentId, StringComparison.OrdinalIgnoreCase));
    }

    private static MspWorkspaceEntry Directory(string path, string name)
    {
        return new MspWorkspaceEntry
        {
            Path = path,
            Name = name,
            IsDirectory = true
        };
    }

    private static MspWorkspaceEntry File(string path, string name, long? sizeBytes, string mediaType)
    {
        return new MspWorkspaceEntry
        {
            Path = path,
            Name = name,
            IsDirectory = false,
            SizeBytes = sizeBytes,
            MediaType = mediaType
        };
    }

    private static long EstimateDocumentInfoSize(LibraryItem document)
    {
        return document.Name.Length + 128;
    }

    private static string GuessMediaType(string path)
    {
        return Path.GetExtension(path).ToLowerInvariant() switch
        {
            ".json" => "application/json",
            ".md" or ".markdown" => "text/markdown",
            ".txt" => "text/plain",
            ".csv" => "text/csv",
            _ => "text/plain"
        };
    }

    private static object ProjectSettings(WorkspaceSettings settings)
    {
        return new
        {
            settings.LanguageCode,
            settings.ProviderName,
            settings.ProviderBaseUrl,
            settings.ModelName,
            settings.UseOfflineResponses,
            settings.AttachmentDefaultPrompt,
            settings.RegionExplainPrompt,
            settings.ChapterExplainPrompt,
            settings.MinorUEndpoint
        };
    }

    private static object ProjectInfo(ProjectItem project)
    {
        return new
        {
            project.Id,
            project.Name,
            project.Description,
            project.UpdatedAt,
            DocumentCount = project.LibraryItems.Count(item => item.Kind != LibraryItemKind.Folder)
        };
    }

    private static object ProjectDocument(LibraryItem document)
    {
        return new
        {
            document.Id,
            document.ProjectId,
            document.Kind,
            document.Name,
            document.PageCount,
            document.CurrentPage,
            document.CurrentPageLabel,
            document.ImportedAt,
            document.UpdatedAt,
            document.SizeBytes,
            PageLabels = document.PageLabels.Select(label => new { label.PdfPage, label.Label })
        };
    }

    private static object ProjectOutline(OutlineItem item)
    {
        return new
        {
            item.Id,
            item.Title,
            item.Page,
            item.Level
        };
    }

    private static object ProjectConversation(ChatConversation conversation)
    {
        return new
        {
            conversation.Id,
            conversation.DocumentId,
            conversation.Title,
            conversation.UpdatedAt,
            Messages = conversation.Messages.Select(message => new
            {
                message.Id,
                message.Role,
                message.Author,
                message.Content,
                message.CreatedAt,
                Attachments = message.Attachments.Select(attachment => new
                {
                    attachment.Id,
                    attachment.Kind,
                    attachment.DocumentId,
                    attachment.Title,
                    attachment.StartPage,
                    attachment.EndPage,
                    attachment.RegionX,
                    attachment.RegionY,
                    attachment.RegionWidth,
                    attachment.RegionHeight
                })
            })
        };
    }
}
