namespace ReadOS.Msp.Workspace;

public static class MspNamespacePathUtility
{
    public static bool TryResolveDescendant(
        IMspWorkspace workspace,
        string namespacePath,
        string candidatePath,
        string workingDirectory,
        out string normalizedPath)
    {
        return TryResolve(
            workspace,
            namespacePath,
            candidatePath,
            workingDirectory,
            allowNamespaceRoot: false,
            out normalizedPath);
    }

    public static bool TryResolveRootOrDescendant(
        IMspWorkspace workspace,
        string namespacePath,
        string candidatePath,
        string workingDirectory,
        out string normalizedPath)
    {
        return TryResolve(
            workspace,
            namespacePath,
            candidatePath,
            workingDirectory,
            allowNamespaceRoot: true,
            out normalizedPath);
    }

    public static bool TryResolveRecordPath(
        IMspWorkspace workspace,
        string namespacePath,
        string identifier,
        string suffix,
        out string normalizedPath)
    {
        ArgumentNullException.ThrowIfNull(workspace);

        normalizedPath = string.Empty;
        if (!IsValidIdentifier(identifier) ||
            string.IsNullOrEmpty(suffix) ||
            suffix.Contains('/') ||
            suffix.Contains('\\'))
        {
            return false;
        }

        var normalizedNamespace = workspace.NormalizePath(namespacePath);
        var candidate = normalizedNamespace.TrimEnd('/') + "/" + identifier + suffix;
        var normalizedCandidate = workspace.NormalizePath(candidate);
        if (!IsStrictDescendant(normalizedNamespace, normalizedCandidate))
        {
            return false;
        }

        normalizedPath = normalizedCandidate;
        return true;
    }

    public static bool IsValidIdentifier(string identifier)
    {
        return !string.IsNullOrWhiteSpace(identifier) &&
            identifier is not "." and not ".." &&
            identifier.All(character =>
                char.IsLetterOrDigit(character) ||
                character is '.' or '-' or '_');
    }

    private static bool TryResolve(
        IMspWorkspace workspace,
        string namespacePath,
        string candidatePath,
        string workingDirectory,
        bool allowNamespaceRoot,
        out string normalizedPath)
    {
        ArgumentNullException.ThrowIfNull(workspace);

        normalizedPath = string.Empty;
        if (string.IsNullOrWhiteSpace(namespacePath) ||
            string.IsNullOrWhiteSpace(candidatePath) ||
            HasWindowsDrivePrefix(candidatePath) ||
            candidatePath.Contains('\0'))
        {
            return false;
        }

        var normalizedNamespace = workspace.NormalizePath(namespacePath);
        var resolvedFromWorkingDirectory = workspace.NormalizePath(candidatePath, workingDirectory);
        if (IsRootOrDescendant(normalizedNamespace, resolvedFromWorkingDirectory))
        {
            if (!allowNamespaceRoot &&
                string.Equals(normalizedNamespace, resolvedFromWorkingDirectory, StringComparison.Ordinal))
            {
                return false;
            }

            normalizedPath = resolvedFromWorkingDirectory;
            return true;
        }

        if (IsVirtualRooted(candidatePath) || ContainsParentSegment(candidatePath))
        {
            return false;
        }

        var resolvedFromNamespace = workspace.NormalizePath(candidatePath, normalizedNamespace);
        if (!IsRootOrDescendant(normalizedNamespace, resolvedFromNamespace) ||
            !allowNamespaceRoot &&
            string.Equals(normalizedNamespace, resolvedFromNamespace, StringComparison.Ordinal))
        {
            return false;
        }

        normalizedPath = resolvedFromNamespace;
        return true;
    }

    private static bool IsRootOrDescendant(string namespacePath, string candidatePath)
    {
        return string.Equals(namespacePath, candidatePath, StringComparison.Ordinal) ||
            IsStrictDescendant(namespacePath, candidatePath);
    }

    private static bool IsStrictDescendant(string namespacePath, string candidatePath)
    {
        var prefix = namespacePath == "/" ? "/" : namespacePath + "/";
        return candidatePath.StartsWith(prefix, StringComparison.Ordinal) &&
            candidatePath.Length > prefix.Length;
    }

    private static bool ContainsParentSegment(string path)
    {
        return path
            .Replace('\\', '/')
            .Split('/', StringSplitOptions.RemoveEmptyEntries)
            .Any(part => string.Equals(part, "..", StringComparison.Ordinal));
    }

    private static bool IsVirtualRooted(string path)
    {
        return path.Replace('\\', '/').StartsWith("/", StringComparison.Ordinal);
    }

    private static bool HasWindowsDrivePrefix(string path)
    {
        return path.Length >= 2 &&
            char.IsAsciiLetter(path[0]) &&
            path[1] == ':';
    }
}
