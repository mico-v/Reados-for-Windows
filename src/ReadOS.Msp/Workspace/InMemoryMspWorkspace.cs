using System.Text.Json;
using ReadOS.Msp.Models;

namespace ReadOS.Msp.Workspace;

public sealed class InMemoryMspWorkspace : IMspWorkspace
{
    private const string ManifestSuffix = ".manifest.json";

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        WriteIndented = true
    };

    private readonly SortedDictionary<string, string> files = new(StringComparer.Ordinal);
    private readonly SortedDictionary<string, MspArtifact> artifacts = new(StringComparer.Ordinal);
    private readonly SortedSet<string> directories = new(StringComparer.Ordinal) { "/" };

    public string NormalizePath(string path, string workingDirectory = "/")
    {
        return MspPathUtility.Normalize(path, workingDirectory);
    }

    public ValueTask<bool> ExistsAsync(string path, CancellationToken cancellationToken = default)
    {
        var normalized = NormalizePath(path);
        var exists = files.ContainsKey(normalized) ||
            directories.Contains(normalized) ||
            TryResolveManifestPath(normalized, out var artifactPath) && artifacts.ContainsKey(artifactPath);
        return ValueTask.FromResult(exists);
    }

    public ValueTask<IReadOnlyList<MspWorkspaceEntry>> ListAsync(string path, CancellationToken cancellationToken = default)
    {
        var normalized = NormalizePath(path);
        if (!directories.Contains(normalized))
        {
            return ValueTask.FromResult<IReadOnlyList<MspWorkspaceEntry>>(Array.Empty<MspWorkspaceEntry>());
        }

        var prefix = normalized == "/" ? "/" : normalized + "/";
        var entries = new SortedDictionary<string, MspWorkspaceEntry>(StringComparer.Ordinal);
        foreach (var directory in directories)
        {
            AddEntry(directory, isDirectory: true, content: null, mediaType: null);
        }

        foreach (var file in files)
        {
            var mediaType = artifacts.TryGetValue(file.Key, out var artifact)
                ? artifact.MediaType
                : "text/plain";
            AddEntry(file.Key, isDirectory: false, content: file.Value, mediaType);
        }

        foreach (var artifact in artifacts.Values)
        {
            AddEntry(GetManifestPath(artifact.Path), isDirectory: false, content: SerializeArtifact(artifact), mediaType: "application/json");
        }

        return ValueTask.FromResult<IReadOnlyList<MspWorkspaceEntry>>(entries.Values.ToArray());

        void AddEntry(string candidate, bool isDirectory, string? content, string? mediaType)
        {
            if (candidate == normalized || !candidate.StartsWith(prefix, StringComparison.Ordinal))
            {
                return;
            }

            var remainder = candidate[prefix.Length..];
            var slashIndex = remainder.IndexOf('/');
            var childName = slashIndex < 0 ? remainder : remainder[..slashIndex];
            if (string.IsNullOrWhiteSpace(childName))
            {
                return;
            }

            var childPath = prefix.TrimEnd('/') + "/" + childName;
            if (normalized == "/")
            {
                childPath = "/" + childName;
            }

            entries.TryAdd(childPath, new MspWorkspaceEntry
            {
                Path = childPath,
                Name = childName,
                IsDirectory = slashIndex >= 0 || isDirectory,
                SizeBytes = slashIndex >= 0 || isDirectory ? null : content?.Length,
                MediaType = slashIndex >= 0 || isDirectory ? null : mediaType
            });
        }
    }

    public ValueTask<string?> TryReadTextAsync(string path, CancellationToken cancellationToken = default)
    {
        var normalized = NormalizePath(path);
        if (TryResolveManifestPath(normalized, out var artifactPath) &&
            artifacts.TryGetValue(artifactPath, out var artifact))
        {
            return ValueTask.FromResult<string?>(SerializeArtifact(artifact));
        }

        files.TryGetValue(normalized, out var content);
        return ValueTask.FromResult(content);
    }

    public ValueTask WriteTextAsync(string path, string content, CancellationToken cancellationToken = default)
    {
        var normalized = NormalizePath(path);
        files[normalized] = content;
        AddParentDirectories(normalized);
        return ValueTask.CompletedTask;
    }

    public ValueTask WriteTextAsync(
        string path,
        string content,
        MspArtifact? artifact,
        CancellationToken cancellationToken = default)
    {
        var normalized = NormalizePath(path);
        files[normalized] = content;
        if (artifact is not null)
        {
            var now = DateTimeOffset.UtcNow;
            var createdAt = artifacts.TryGetValue(normalized, out var existing)
                ? existing.CreatedAt
                : artifact.CreatedAt == default ? now : artifact.CreatedAt;
            var updatedAt = artifact.UpdatedAt == default ? now : artifact.UpdatedAt;
            artifacts[normalized] = artifact with
            {
                Path = normalized,
                SizeBytes = content.Length,
                CreatedAt = createdAt,
                UpdatedAt = updatedAt
            };
        }

        AddParentDirectories(normalized);
        return ValueTask.CompletedTask;
    }

    private void AddParentDirectories(string path)
    {
        var current = "/";
        foreach (var part in path.Split('/', StringSplitOptions.RemoveEmptyEntries).SkipLast(1))
        {
            current = current == "/" ? "/" + part : current + "/" + part;
            directories.Add(current);
        }
    }

    private static string GetManifestPath(string artifactPath)
    {
        return artifactPath + ManifestSuffix;
    }

    private static bool TryResolveManifestPath(string path, out string artifactPath)
    {
        if (path.StartsWith("/artifacts/", StringComparison.Ordinal) &&
            path.EndsWith(ManifestSuffix, StringComparison.OrdinalIgnoreCase))
        {
            artifactPath = path[..^ManifestSuffix.Length];
            return artifactPath.Length > "/artifacts/".Length;
        }

        artifactPath = string.Empty;
        return false;
    }

    private static string SerializeArtifact(MspArtifact artifact)
    {
        return JsonSerializer.Serialize(artifact, JsonOptions);
    }
}
