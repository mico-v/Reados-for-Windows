namespace ReadOS.Msp.Workspace;

public sealed class InMemoryMspWorkspace : IMspWorkspace
{
    private readonly SortedDictionary<string, string> files = new(StringComparer.Ordinal);
    private readonly SortedSet<string> directories = new(StringComparer.Ordinal) { "/" };

    public string NormalizePath(string path, string workingDirectory = "/")
    {
        return MspPathUtility.Normalize(path, workingDirectory);
    }

    public ValueTask<bool> ExistsAsync(string path, CancellationToken cancellationToken = default)
    {
        var normalized = NormalizePath(path);
        return ValueTask.FromResult(files.ContainsKey(normalized) || directories.Contains(normalized));
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
            AddEntry(directory, isDirectory: true, content: null);
        }

        foreach (var file in files)
        {
            AddEntry(file.Key, isDirectory: false, content: file.Value);
        }

        return ValueTask.FromResult<IReadOnlyList<MspWorkspaceEntry>>(entries.Values.ToArray());

        void AddEntry(string candidate, bool isDirectory, string? content)
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
                MediaType = slashIndex >= 0 || isDirectory ? null : "text/plain"
            });
        }
    }

    public ValueTask<string?> TryReadTextAsync(string path, CancellationToken cancellationToken = default)
    {
        files.TryGetValue(NormalizePath(path), out var content);
        return ValueTask.FromResult(content);
    }

    public ValueTask WriteTextAsync(string path, string content, CancellationToken cancellationToken = default)
    {
        var normalized = NormalizePath(path);
        files[normalized] = content;
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
}
