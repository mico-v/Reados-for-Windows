using System.Text.Json;
using ReadOS.App.Models;
using ReadOS.Msp.Hosting.Sessions;
using ReadOS.Msp.Models;
using ReadOS.Msp.Workspace;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsVirtualWorkspace : IMspWorkspace
{
    private const string ArtifactManifestSuffix = ".manifest.json";

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        WriteIndented = true
    };

    private readonly IWorkspaceStore workspaceStore;
    private readonly IPdfDocumentService pdfService;
    private readonly Func<WorkspaceState?> workspaceProvider;
    private readonly MspSessionWorkspaceProjectionService sessionWorkspaceProjectionService = new();

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
        if (normalized is "/" or "/projects" or "/library" or "/documents" or "/artifacts" or "/sessions" or "/transcripts" or "/settings.json")
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
            "/sessions" => sessionWorkspaceProjectionService.ListSessionEntries(
                workspace.MspSessions.Select(session => session.ToRecord())),
            "/transcripts" => sessionWorkspaceProjectionService.ListTranscriptEntries(
                workspace.MspTranscript.Select(entry => entry.ToRecord())),
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
            if (TryResolveArtifactManifestPath(normalized, out var artifactPath))
            {
                var manifestArtifact = workspace.Artifacts.FirstOrDefault(item =>
                    string.Equals(item.Path, artifactPath, StringComparison.OrdinalIgnoreCase));
                return manifestArtifact is null ? null : JsonSerializer.Serialize(ProjectArtifact(manifestArtifact), JsonOptions);
            }

            var artifact = workspace.Artifacts.FirstOrDefault(item =>
                string.Equals(item.Path, normalized, StringComparison.OrdinalIgnoreCase));
            return artifact?.Content;
        }

        if (normalized.StartsWith("/sessions/", StringComparison.Ordinal) && normalized.EndsWith(".json", StringComparison.Ordinal))
        {
            var session = sessionWorkspaceProjectionService.FindSession(
                workspace.MspSessions.Select(item => item.ToRecord()),
                normalized);
            return session is null ? null : JsonSerializer.Serialize(ProjectSession(session), JsonOptions);
        }

        if (normalized.StartsWith("/transcripts/", StringComparison.Ordinal) && normalized.EndsWith(".json", StringComparison.Ordinal))
        {
            var transcript = sessionWorkspaceProjectionService.FindTranscript(
                workspace.MspTranscript.Select(item => item.ToRecord()),
                normalized);
            return transcript is null ? null : JsonSerializer.Serialize(ProjectTranscript(transcript), JsonOptions);
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

    public ValueTask WriteTextAsync(string path, string content, CancellationToken cancellationToken = default)
    {
        return WriteTextAsync(path, content, artifact: null, cancellationToken);
    }

    public async ValueTask WriteTextAsync(
        string path,
        string content,
        MspArtifact? artifact,
        CancellationToken cancellationToken = default)
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
        var workspaceArtifact = workspace.Artifacts.FirstOrDefault(item =>
            string.Equals(item.Path, normalized, StringComparison.OrdinalIgnoreCase));
        if (workspaceArtifact is null)
        {
            workspaceArtifact = new WorkspaceArtifact
            {
                Path = normalized,
                CreatedAt = artifact is null || artifact.CreatedAt == default ? now : artifact.CreatedAt
            };
            workspace.Artifacts.Add(workspaceArtifact);
        }

        workspaceArtifact.Content = content;
        workspaceArtifact.MediaType = string.IsNullOrWhiteSpace(artifact?.MediaType)
            ? GuessMediaType(normalized)
            : artifact.MediaType;
        workspaceArtifact.Description = string.IsNullOrWhiteSpace(artifact?.Description)
            ? workspaceArtifact.Description
            : artifact.Description;
        workspaceArtifact.UpdatedAt = artifact is null || artifact.UpdatedAt == default ? now : artifact.UpdatedAt;

        if (artifact is not null)
        {
            workspaceArtifact.SourceCommand = artifact.SourceCommand ?? string.Empty;
            workspaceArtifact.Actor = string.IsNullOrWhiteSpace(artifact.Actor) ? "agent" : artifact.Actor;
            workspaceArtifact.SessionId = artifact.SessionId ?? string.Empty;
            workspaceArtifact.Preview = artifact.Preview ?? string.Empty;
            ReplaceValues(workspaceArtifact.SourcePaths, artifact.SourcePaths);
            ReplaceValues(workspaceArtifact.SourceDocuments, artifact.SourceDocuments);
            ReplaceValues(workspaceArtifact.SourcePages, artifact.SourcePages);
        }

        await workspaceStore.SaveAsync(workspace, cancellationToken);
    }

    public async ValueTask<bool> TryDeleteAsync(string path, CancellationToken cancellationToken = default)
    {
        var workspace = workspaceProvider();
        if (workspace is null)
        {
            return false;
        }

        var normalized = NormalizePath(path);
        if (TryResolveArtifactManifestPath(normalized, out var artifactPath))
        {
            normalized = artifactPath;
        }

        if (!normalized.StartsWith("/artifacts/", StringComparison.Ordinal) ||
            normalized.EndsWith("/", StringComparison.Ordinal))
        {
            return false;
        }

        var artifact = workspace.Artifacts.FirstOrDefault(item =>
            string.Equals(item.Path, normalized, StringComparison.OrdinalIgnoreCase));
        if (artifact is null)
        {
            return false;
        }

        workspace.Artifacts.Remove(artifact);
        await workspaceStore.SaveAsync(workspace, cancellationToken);
        return true;
    }

    private static IReadOnlyList<MspWorkspaceEntry> RootEntries()
    {
        return new[]
        {
            Directory("/projects", "projects"),
            Directory("/library", "library"),
            Directory("/documents", "documents"),
            Directory("/artifacts", "artifacts"),
            Directory("/sessions", "sessions"),
            Directory("/transcripts", "transcripts"),
            File("/settings.json", "settings.json", null, "application/json")
        };
    }

    private IReadOnlyList<MspWorkspaceEntry> ListNested(string normalized, WorkspaceState workspace)
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

        if (parts.Length >= 1 && parts[0] == "sessions")
        {
            return parts.Length == 1
                ? sessionWorkspaceProjectionService.ListSessionEntries(
                    workspace.MspSessions.Select(session => session.ToRecord()))
                : Array.Empty<MspWorkspaceEntry>();
        }

        if (parts.Length >= 1 && parts[0] == "transcripts")
        {
            return parts.Length == 1
                ? sessionWorkspaceProjectionService.ListTranscriptEntries(
                    workspace.MspTranscript.Select(entry => entry.ToRecord()))
                : Array.Empty<MspWorkspaceEntry>();
        }

        return Array.Empty<MspWorkspaceEntry>();
    }

    private static IReadOnlyList<MspWorkspaceEntry> ListArtifactEntries(string normalized, WorkspaceState workspace)
    {
        var entries = new Dictionary<string, MspWorkspaceEntry>(StringComparer.OrdinalIgnoreCase);
        var prefix = normalized == "/artifacts" ? "/artifacts/" : normalized.TrimEnd('/') + "/";
        foreach (var artifact in workspace.Artifacts)
        {
            AddArtifactEntry(artifact.Path, artifact.SizeBytes, artifact.MediaType);
            AddArtifactEntry(GetArtifactManifestPath(artifact.Path), EstimateArtifactManifestSize(artifact), "application/json");
        }

        return entries.Values
            .OrderBy(item => item.IsDirectory ? 0 : 1)
            .ThenBy(item => item.Name, StringComparer.OrdinalIgnoreCase)
            .ToArray();

        void AddArtifactEntry(string path, long? sizeBytes, string mediaType)
        {
            if (!path.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
            {
                return;
            }

            var remainder = path[prefix.Length..];
            if (string.IsNullOrWhiteSpace(remainder))
            {
                return;
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
                entries.TryAdd(path, File(
                    path,
                    Path.GetFileName(path),
                    sizeBytes,
                    mediaType));
            }
        }
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

    private static long EstimateArtifactManifestSize(WorkspaceArtifact artifact)
    {
        return artifact.Path.Length +
            artifact.Description.Length +
            artifact.SourceCommand.Length +
            artifact.Actor.Length +
            artifact.SessionId.Length +
            artifact.Preview.Length +
            512;
    }

    private static string GetArtifactManifestPath(string artifactPath)
    {
        return artifactPath + ArtifactManifestSuffix;
    }

    private static bool TryResolveArtifactManifestPath(string path, out string artifactPath)
    {
        if (path.StartsWith("/artifacts/", StringComparison.Ordinal) &&
            path.EndsWith(ArtifactManifestSuffix, StringComparison.OrdinalIgnoreCase))
        {
            artifactPath = path[..^ArtifactManifestSuffix.Length];
            return artifactPath.Length > "/artifacts/".Length;
        }

        artifactPath = string.Empty;
        return false;
    }

    private static void ReplaceValues(ICollection<string> target, IEnumerable<string> values)
    {
        target.Clear();
        foreach (var value in values.Where(value => !string.IsNullOrWhiteSpace(value)))
        {
            target.Add(value);
        }
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

    private static object ProjectArtifact(WorkspaceArtifact artifact)
    {
        return new
        {
            artifact.Id,
            artifact.Path,
            artifact.MediaType,
            artifact.Description,
            artifact.SizeBytes,
            artifact.SourceCommand,
            artifact.Actor,
            artifact.SessionId,
            SourcePaths = artifact.SourcePaths.ToArray(),
            SourceDocuments = artifact.SourceDocuments.ToArray(),
            SourcePages = artifact.SourcePages.ToArray(),
            artifact.CreatedAt,
            artifact.UpdatedAt,
            artifact.Preview
        };
    }

    private static object ProjectSession(MspSessionRecord record)
    {
        return new
        {
            record.Id,
            record.Title,
            record.Actor,
            record.StartedAt,
            record.UpdatedAt,
            record.LastCommandText,
            record.LastDecision,
            record.LastExitCode,
            record.LastProgressMessage,
            record.LastDiagnosticsSummary,
            record.LastRecoveryHint,
            record.CommandCount,
            record.RunningCount,
            record.PendingApprovalCount,
            record.ApprovalCount,
            record.FailureCount,
            record.TranscriptIds,
            record.ArtifactPaths
        };
    }

    private static object ProjectTranscript(MspCommandTranscriptRecord record)
    {
        return new
        {
            record.Id,
            record.Actor,
            record.SessionId,
            record.CommandText,
            record.StartedAt,
            record.CompletedAt,
            record.ExitCode,
            record.Stdout,
            record.Stderr,
            record.Decision,
            record.Effects,
            record.ArtifactsSummary,
            record.PolicyPreview,
            record.DiagnosticsSummary,
            record.RecoveryHint,
            record.ProgressMessage,
            record.ProgressPercent,
            record.IsRunning,
            record.WasCanceled
        };
    }
}
