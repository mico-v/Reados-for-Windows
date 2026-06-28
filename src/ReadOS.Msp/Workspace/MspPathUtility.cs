namespace ReadOS.Msp.Workspace;

public static class MspPathUtility
{
    public static string Normalize(string path, string workingDirectory = "/")
    {
        if (string.IsNullOrWhiteSpace(path))
        {
            path = ".";
        }

        path = path.Replace('\\', '/');
        var combined = path.StartsWith("/", StringComparison.Ordinal)
            ? path
            : $"{NormalizeDirectory(workingDirectory)}/{path}";

        var parts = new List<string>();
        foreach (var part in combined.Split('/', StringSplitOptions.RemoveEmptyEntries))
        {
            if (part == ".")
            {
                continue;
            }

            if (part == "..")
            {
                if (parts.Count > 0)
                {
                    parts.RemoveAt(parts.Count - 1);
                }

                continue;
            }

            parts.Add(part);
        }

        return "/" + string.Join('/', parts);
    }

    private static string NormalizeDirectory(string path)
    {
        var normalized = Normalize(path, "/");
        return normalized == "/" ? string.Empty : normalized.TrimEnd('/');
    }
}
