using System.IO.Compression;
using System.Security.Cryptography;
using System.Text.Json;
using System.Text.Json.Serialization;
using ReadOS.App.Models;

namespace ReadOS.App.Services;

public sealed class WorkspaceStore : IWorkspaceStore
{
    private const string StateFileName = "workspace.json";
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PreferredObjectCreationHandling = JsonObjectCreationHandling.Populate,
        WriteIndented = true
    };

    private readonly IPdfDocumentService pdfService;
    private readonly IProviderCredentialStore providerCredentialStore;
    private readonly SemaphoreSlim credentialGate = new(1, 1);
    private readonly string statePath;
    private string persistedProviderScope = string.Empty;
    private string persistedProviderApiKey = string.Empty;
    private bool hasPersistedCredentialSnapshot;

    public WorkspaceStore(
        IPdfDocumentService pdfService,
        IProviderCredentialStore providerCredentialStore)
        : this(
            pdfService,
            providerCredentialStore,
            Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "ReadOS"))
    {
    }

    internal WorkspaceStore(
        IPdfDocumentService pdfService,
        IProviderCredentialStore providerCredentialStore,
        string workspaceRoot)
    {
        this.pdfService = pdfService ?? throw new ArgumentNullException(nameof(pdfService));
        this.providerCredentialStore = providerCredentialStore ?? throw new ArgumentNullException(nameof(providerCredentialStore));
        ArgumentException.ThrowIfNullOrWhiteSpace(workspaceRoot);
        WorkspaceRoot = Path.GetFullPath(workspaceRoot);
        LibraryRoot = Path.Combine(WorkspaceRoot, "Library");
        statePath = Path.Combine(WorkspaceRoot, StateFileName);
    }

    public string WorkspaceRoot { get; }

    public string LibraryRoot { get; }

    public async Task<WorkspaceState> LoadAsync(CancellationToken cancellationToken = default)
    {
        Directory.CreateDirectory(WorkspaceRoot);
        Directory.CreateDirectory(LibraryRoot);

        if (!File.Exists(statePath))
        {
            var initial = CreateInitialState();
            await HydrateProviderCredentialAsync(initial.Settings, default, cancellationToken);
            await WriteStateAsync(initial, cancellationToken);
            return initial;
        }

        var serializedState = await File.ReadAllBytesAsync(statePath, cancellationToken);
        WorkspaceState? state;
        LegacyProviderApiKey legacyProviderApiKey;
        try
        {
            legacyProviderApiKey = ReadLegacyProviderApiKey(serializedState);
            state = JsonSerializer.Deserialize<WorkspaceState>(serializedState, JsonOptions);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(serializedState);
        }

        state ??= CreateInitialState();
        EnsureStateShape(state);
        await HydrateProviderCredentialAsync(state.Settings, legacyProviderApiKey, cancellationToken);
        if (legacyProviderApiKey.WasPresent)
        {
            await WriteStateAsync(state, cancellationToken);
        }

        return state;
    }

    public async Task SaveAsync(WorkspaceState state, CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(state);
        await PersistProviderCredentialAsync(state.Settings, cancellationToken);
        await WriteStateAsync(state, cancellationToken);
    }

    private async Task WriteStateAsync(WorkspaceState state, CancellationToken cancellationToken)
    {
        Directory.CreateDirectory(WorkspaceRoot);
        Directory.CreateDirectory(LibraryRoot);
        var tempPath = statePath + ".tmp";
        try
        {
            await using (var stream = File.Create(tempPath))
            {
                await JsonSerializer.SerializeAsync(stream, state, JsonOptions, cancellationToken);
            }

            File.Move(tempPath, statePath, overwrite: true);
        }
        finally
        {
            if (File.Exists(tempPath))
            {
                File.Delete(tempPath);
            }
        }
    }

    public async Task<ProjectItem> CreateProjectAsync(WorkspaceState state, string name, CancellationToken cancellationToken = default)
    {
        var project = new ProjectItem
        {
            Name = string.IsNullOrWhiteSpace(name) ? $"阅读项目 {state.Projects.Count + 1}" : name.Trim(),
            Description = "本地 PDF 阅读项目",
            UpdatedAt = DateTimeOffset.Now
        };

        project.StandaloneConversations.Add(new ChatConversation
        {
            Title = "项目问答",
            UpdatedAt = DateTimeOffset.Now
        });

        state.Projects.Insert(0, project);
        Directory.CreateDirectory(GetProjectDirectory(project));
        await SaveAsync(state, cancellationToken);
        return project;
    }

    public async Task<LibraryItem> ImportDocumentAsync(WorkspaceState state, ProjectItem project, string sourcePath, CancellationToken cancellationToken = default)
    {
        if (!File.Exists(sourcePath))
        {
            throw new FileNotFoundException("导入文件不存在。", sourcePath);
        }

        var extension = Path.GetExtension(sourcePath).ToLowerInvariant();
        var kind = extension switch
        {
            ".pdf" => LibraryItemKind.Pdf,
            ".md" or ".markdown" => LibraryItemKind.Markdown,
            ".txt" => LibraryItemKind.Note,
            _ => throw new InvalidOperationException("当前版本支持导入 PDF、Markdown 和文本文件。")
        };

        var projectDirectory = GetProjectDirectory(project);
        Directory.CreateDirectory(projectDirectory);

        var fileName = GetUniqueFileName(projectDirectory, Path.GetFileName(sourcePath));
        var destination = Path.Combine(projectDirectory, fileName);
        File.Copy(sourcePath, destination, overwrite: false);

        var item = new LibraryItem
        {
            ProjectId = project.Id,
            Kind = kind,
            Name = Path.GetFileName(destination),
            RelativePath = Path.GetRelativePath(LibraryRoot, destination),
            SizeBytes = new FileInfo(destination).Length,
            Order = project.LibraryItems.Count,
            ImportedAt = DateTimeOffset.Now,
            UpdatedAt = DateTimeOffset.Now
        };

        if (kind == LibraryItemKind.Pdf)
        {
            var info = await pdfService.InspectAsync(destination, cancellationToken);
            item.PageCount = info.PageCount;
            item.CurrentPage = info.PageCount > 0 ? 1 : 0;
            foreach (var label in info.Labels)
            {
                item.PageLabels.Add(label);
            }

            foreach (var outlineItem in info.Outline)
            {
                item.Outline.Add(outlineItem);
            }
        }

        item.Conversations.Add(new ChatConversation
        {
            DocumentId = item.Id,
            Title = "阅读问答",
            UpdatedAt = DateTimeOffset.Now
        });

        project.LibraryItems.Insert(0, item);
        Touch(project);
        await SaveAsync(state, cancellationToken);
        return item;
    }

    public async Task RenameDocumentAsync(WorkspaceState state, LibraryItem document, string newName, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(newName))
        {
            return;
        }

        var currentPath = GetAbsolutePath(document);
        var extension = Path.GetExtension(currentPath);
        var sanitized = SanitizeFileName(newName.Trim());
        if (string.IsNullOrWhiteSpace(Path.GetExtension(sanitized)))
        {
            sanitized += extension;
        }

        var directory = Path.GetDirectoryName(currentPath) ?? LibraryRoot;
        var targetPath = Path.Combine(directory, GetUniqueFileName(directory, sanitized, currentPath));
        if (!string.Equals(currentPath, targetPath, StringComparison.OrdinalIgnoreCase))
        {
            File.Move(currentPath, targetPath);
            document.RelativePath = Path.GetRelativePath(LibraryRoot, targetPath);
        }

        document.Name = Path.GetFileName(targetPath);
        document.UpdatedAt = DateTimeOffset.Now;
        await SaveAsync(state, cancellationToken);
    }

    public async Task DeleteDocumentAsync(WorkspaceState state, ProjectItem project, LibraryItem document, CancellationToken cancellationToken = default)
    {
        project.LibraryItems.Remove(document);
        var path = GetAbsolutePath(document);
        if (File.Exists(path))
        {
            var trashDirectory = Path.Combine(WorkspaceRoot, "Trash", DateTimeOffset.Now.ToString("yyyyMMdd-HHmmss"));
            Directory.CreateDirectory(trashDirectory);
            File.Move(path, Path.Combine(trashDirectory, Path.GetFileName(path)), overwrite: true);
        }

        Touch(project);
        await SaveAsync(state, cancellationToken);
    }

    public async Task<string> ExportWorkspaceAsync(WorkspaceState state, string destinationPath, CancellationToken cancellationToken = default)
    {
        await SaveAsync(state, cancellationToken);
        if (File.Exists(destinationPath))
        {
            File.Delete(destinationPath);
        }

        ZipFile.CreateFromDirectory(WorkspaceRoot, destinationPath, CompressionLevel.Optimal, includeBaseDirectory: false);
        return destinationPath;
    }

    public async Task<WorkspaceState> ImportWorkspaceAsync(string sourcePath, CancellationToken cancellationToken = default)
    {
        if (!File.Exists(sourcePath))
        {
            throw new FileNotFoundException("工作区备份不存在。", sourcePath);
        }

        var backupDirectory = Path.Combine(WorkspaceRoot, "Backup-" + DateTimeOffset.Now.ToString("yyyyMMdd-HHmmss"));
        if (Directory.Exists(WorkspaceRoot))
        {
            Directory.CreateDirectory(backupDirectory);
            foreach (var path in Directory.EnumerateFileSystemEntries(WorkspaceRoot).Where(path => !path.Equals(backupDirectory, StringComparison.OrdinalIgnoreCase)))
            {
                var target = Path.Combine(backupDirectory, Path.GetFileName(path));
                if (Directory.Exists(path))
                {
                    Directory.Move(path, target);
                }
                else
                {
                    File.Move(path, target);
                }
            }
        }

        Directory.CreateDirectory(WorkspaceRoot);
        ZipFile.ExtractToDirectory(sourcePath, WorkspaceRoot, overwriteFiles: true);
        var state = await LoadAsync(cancellationToken);
        await SaveAsync(state, cancellationToken);
        return state;
    }

    public string GetAbsolutePath(LibraryItem item)
    {
        if (!TryResolveConfinedPath(item.RelativePath, out var fullPath))
        {
            throw new InvalidOperationException($"workspace.path.not_confined: {item.RelativePath}");
        }

        return fullPath;
    }

    private bool TryResolveConfinedPath(string relativePath, out string fullPath)
    {
        fullPath = string.Empty;
        if (string.IsNullOrWhiteSpace(relativePath) || relativePath.IndexOf('\0') >= 0)
        {
            return false;
        }

        if (Path.IsPathFullyQualified(relativePath) ||
            Path.IsPathRooted(relativePath) ||
            (relativePath.Length >= 2 &&
                char.IsLetter(relativePath[0]) &&
                relativePath[1] == ':'))
        {
            return false;
        }

        fullPath = Path.GetFullPath(Path.Combine(LibraryRoot, relativePath));
        return fullPath.StartsWith(LibraryRoot + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase) ||
            fullPath.Equals(LibraryRoot, StringComparison.OrdinalIgnoreCase);
    }

    private static WorkspaceState CreateInitialState()
    {
        var state = new WorkspaceState();
        var project = new ProjectItem
        {
            Name = "默认阅读项目",
            Description = "导入 PDF 后开始阅读和问答",
            UpdatedAt = DateTimeOffset.Now
        };
        project.StandaloneConversations.Add(new ChatConversation
        {
            Title = "项目问答",
            UpdatedAt = DateTimeOffset.Now
        });
        state.Projects.Add(project);
        return state;
    }

    private void EnsureStateShape(WorkspaceState state)
    {
        if (state.Projects.Count == 0)
        {
            state.Projects.Add(CreateInitialState().Projects[0]);
        }

        foreach (var project in state.Projects)
        {
            foreach (var item in project.LibraryItems
                .Where(item => !TryResolveConfinedPath(item.RelativePath, out _))
                .ToArray())
            {
                project.LibraryItems.Remove(item);
            }

            if (project.StandaloneConversations.Count == 0)
            {
                project.StandaloneConversations.Add(new ChatConversation
                {
                    Title = "项目问答",
                    UpdatedAt = DateTimeOffset.Now
                });
            }

            foreach (var item in project.LibraryItems.Where(item => item.Conversations.Count == 0))
            {
                item.Conversations.Add(new ChatConversation
                {
                    DocumentId = item.Id,
                    Title = "阅读问答",
                    UpdatedAt = DateTimeOffset.Now
                });
            }
        }
    }

    private async Task HydrateProviderCredentialAsync(
        WorkspaceSettings settings,
        LegacyProviderApiKey legacyProviderApiKey,
        CancellationToken cancellationToken)
    {
        var providerScope = settings.ProviderBaseUrl?.Trim() ?? string.Empty;
        string? migratedApiKey = null;
        if (legacyProviderApiKey.WasPresent && !string.IsNullOrWhiteSpace(legacyProviderApiKey.Value))
        {
            migratedApiKey = legacyProviderApiKey.Value.Trim();
            await providerCredentialStore.SetApiKeyAsync(providerScope, migratedApiKey, cancellationToken);
        }

        var apiKey = await providerCredentialStore.GetApiKeyAsync(providerScope, cancellationToken) ?? migratedApiKey ?? string.Empty;
        settings.ProviderApiKey = apiKey;
        persistedProviderScope = providerScope;
        persistedProviderApiKey = apiKey;
        hasPersistedCredentialSnapshot = true;
    }

    private async Task PersistProviderCredentialAsync(
        WorkspaceSettings settings,
        CancellationToken cancellationToken)
    {
        var providerScope = settings.ProviderBaseUrl?.Trim() ?? string.Empty;
        var apiKey = settings.ProviderApiKey?.Trim() ?? string.Empty;

        await credentialGate.WaitAsync(cancellationToken);
        try
        {
            if (hasPersistedCredentialSnapshot &&
                string.Equals(persistedProviderScope, providerScope, StringComparison.Ordinal) &&
                string.Equals(persistedProviderApiKey, apiKey, StringComparison.Ordinal))
            {
                return;
            }

            await providerCredentialStore.SetApiKeyAsync(providerScope, apiKey, cancellationToken);
            settings.ProviderApiKey = apiKey;
            persistedProviderScope = providerScope;
            persistedProviderApiKey = apiKey;
            hasPersistedCredentialSnapshot = true;
        }
        finally
        {
            credentialGate.Release();
        }
    }

    private static LegacyProviderApiKey ReadLegacyProviderApiKey(ReadOnlyMemory<byte> serializedState)
    {
        using var document = JsonDocument.Parse(serializedState);
        if (!TryGetProperty(document.RootElement, "settings", out var settings) ||
            settings.ValueKind != JsonValueKind.Object ||
            !TryGetProperty(settings, "providerApiKey", out var apiKey))
        {
            return default;
        }

        return new LegacyProviderApiKey(
            WasPresent: true,
            Value: apiKey.ValueKind == JsonValueKind.String ? apiKey.GetString() : null);
    }

    private static bool TryGetProperty(JsonElement element, string propertyName, out JsonElement value)
    {
        foreach (var property in element.EnumerateObject())
        {
            if (string.Equals(property.Name, propertyName, StringComparison.OrdinalIgnoreCase))
            {
                value = property.Value;
                return true;
            }
        }

        value = default;
        return false;
    }

    private string GetProjectDirectory(ProjectItem project)
    {
        return Path.Combine(LibraryRoot, SanitizeFileName(project.Id));
    }

    private static string GetUniqueFileName(string directory, string fileName, string? allowedExistingPath = null)
    {
        var baseName = Path.GetFileNameWithoutExtension(fileName);
        var extension = Path.GetExtension(fileName);
        var candidate = SanitizeFileName(fileName);
        var index = 2;
        while (true)
        {
            var path = Path.Combine(directory, candidate);
            if (!File.Exists(path) || string.Equals(path, allowedExistingPath, StringComparison.OrdinalIgnoreCase))
            {
                return candidate;
            }

            candidate = $"{SanitizeFileName(baseName)} ({index}){extension}";
            index++;
        }
    }

    private static string SanitizeFileName(string value)
    {
        foreach (var invalid in Path.GetInvalidFileNameChars())
        {
            value = value.Replace(invalid, '-');
        }

        return string.IsNullOrWhiteSpace(value) ? "untitled" : value;
    }

    private static void Touch(ProjectItem project)
    {
        project.UpdatedAt = DateTimeOffset.Now;
    }

    private readonly record struct LegacyProviderApiKey(bool WasPresent, string? Value);
}
