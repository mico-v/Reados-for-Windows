using System.Text;
using System.Text.Json;
using ReadOS.Msp.Models;
using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Commands;

public sealed class ArtifactCommand : IMspCommand
{
    private const string ManifestSuffix = ".manifest.json";

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    public string Name => "artifact";

    public string Summary => "List, show, write, rename, or delete MSP artifacts in the virtual workspace.";

    public MspCommandMetadata Metadata => MspCommandMetadata.Create(
        Name,
        Summary,
        Usage(),
        MspCommandEffects.ReadWorkspace |
            MspCommandEffects.WriteWorkspace |
            MspCommandEffects.DeleteWorkspace |
            MspCommandEffects.CreateArtifact,
        new[] { "msp.artifact.read", "msp.artifact.write", "msp.artifact.delete" });

    public MspCommandMetadata GetMetadata(IReadOnlyList<string> arguments)
    {
        if (arguments.Count == 0)
        {
            return Metadata;
        }

        if (string.Equals(arguments[0], "write", StringComparison.OrdinalIgnoreCase))
        {
            return MspCommandMetadata.Create(
                Name,
                Summary,
                "artifact write <path> <content...>",
                MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact,
                new[] { "msp.artifact.write" });
        }

        if (string.Equals(arguments[0], "rename", StringComparison.OrdinalIgnoreCase))
        {
            return MspCommandMetadata.Create(
                Name,
                Summary,
                "artifact rename <source> <target>",
                MspCommandEffects.ReadWorkspace | MspCommandEffects.WriteWorkspace | MspCommandEffects.DeleteWorkspace,
                new[] { "msp.artifact.write", "msp.artifact.delete" });
        }

        if (string.Equals(arguments[0], "delete", StringComparison.OrdinalIgnoreCase))
        {
            return MspCommandMetadata.Create(
                Name,
                Summary,
                "artifact delete <path>",
                MspCommandEffects.DeleteWorkspace,
                new[] { "msp.artifact.delete" });
        }

        return MspCommandMetadata.Create(
            Name,
            Summary,
            "artifact list [path] | artifact show <path>",
            MspCommandEffects.ReadWorkspace,
            new[] { "msp.artifact.read" });
    }

    public MspCommandPreview GetPreview(IReadOnlyList<string> arguments)
    {
        if (arguments.Count == 0)
        {
            return MspCommandPreview.Create("Inspect or modify MSP artifacts.");
        }

        return arguments[0].ToLowerInvariant() switch
        {
            "write" => MspCommandPreview.Create(
                "Write a text artifact into the virtual workspace.",
                arguments.Count > 1 ? new[] { arguments[1] } : Array.Empty<string>(),
                arguments.Count > 2 ? new[] { $"contentLength: {string.Join(' ', arguments.Skip(2)).Length}" } : Array.Empty<string>()),
            "rename" => MspCommandPreview.Create(
                "Rename an artifact and its projected manifest.",
                arguments.Count > 2 ? new[] { arguments[1], arguments[2] } : arguments.Skip(1).Take(2).ToArray(),
                arguments.Count > 2 ? new[] { $"from: {arguments[1]}", $"to: {arguments[2]}" } : Array.Empty<string>()),
            "delete" => MspCommandPreview.Create(
                "Delete an artifact and its projected manifest.",
                arguments.Count > 1 ? new[] { arguments[1] } : Array.Empty<string>()),
            "show" => MspCommandPreview.Create(
                "Read a text artifact.",
                arguments.Count > 1 ? new[] { arguments[1] } : Array.Empty<string>()),
            "list" => MspCommandPreview.Create(
                "List artifact entries.",
                arguments.Count > 1 ? new[] { arguments[1] } : new[] { "/artifacts" }),
            _ => MspCommandPreview.Create("Inspect or modify MSP artifacts.")
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
            "list" => await ListAsync(context, arguments.Skip(1).ToArray(), cancellationToken),
            "show" => await ShowAsync(context, arguments.Skip(1).ToArray(), cancellationToken),
            "write" => await WriteAsync(context, arguments.Skip(1).ToArray(), cancellationToken),
            "rename" => await RenameAsync(context, arguments.Skip(1).ToArray(), cancellationToken),
            "delete" => await DeleteAsync(context, arguments.Skip(1).ToArray(), cancellationToken),
            _ => MspCommandResult.Failure(Usage(), exitCode: 2)
        };
    }

    private static async ValueTask<MspCommandResult> ListAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken)
    {
        var path = arguments.Count == 0 ? "/artifacts" : arguments[0];
        var normalized = context.Workspace.NormalizePath(path, context.WorkingDirectory);
        if (!normalized.StartsWith("/artifacts", StringComparison.Ordinal))
        {
            return MspCommandResult.Failure("artifact list only supports /artifacts paths.", exitCode: 2);
        }

        var entries = await context.Workspace.ListAsync(normalized, cancellationToken);
        if (entries.Count == 0 && !await context.Workspace.ExistsAsync(normalized, cancellationToken))
        {
            return MspCommandResult.Failure($"No such artifact path: {normalized}", exitCode: 2);
        }

        var builder = new StringBuilder();
        foreach (var entry in entries.OrderBy(item => item.Name, StringComparer.OrdinalIgnoreCase))
        {
            builder.Append(entry.IsDirectory ? "d " : "- ");
            builder.Append(entry.Path);
            if (entry.SizeBytes is not null)
            {
                builder.Append('\t');
                builder.Append(entry.SizeBytes.Value);
            }

            builder.AppendLine();
        }

        return MspCommandResult.Success(builder.ToString());
    }

    private static async ValueTask<MspCommandResult> ShowAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken)
    {
        if (arguments.Count != 1)
        {
            return MspCommandResult.Failure("Usage: artifact show <path>", exitCode: 2);
        }

        var path = NormalizeArtifactPath(context, arguments[0]);
        var content = await context.Workspace.TryReadTextAsync(path, cancellationToken);
        return content is null
            ? MspCommandResult.Failure($"Artifact not found: {path}")
            : MspCommandResult.Success(content.EndsWith(Environment.NewLine, StringComparison.Ordinal)
                ? content
                : content + Environment.NewLine);
    }

    private static async ValueTask<MspCommandResult> WriteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken)
    {
        if (arguments.Count < 2)
        {
            return MspCommandResult.Failure("Usage: artifact write <path> <content...>", exitCode: 2);
        }

        var path = NormalizeArtifactPath(context, arguments[0]);
        var content = string.Join(' ', arguments.Skip(1));
        var now = DateTimeOffset.UtcNow;
        var artifact = new MspArtifact
        {
            Path = path,
            MediaType = GuessMediaType(path),
            SizeBytes = content.Length,
            Description = "Text artifact created by MSP.",
            SourceCommand = context.Invocation.CommandText,
            Actor = context.Invocation.Actor,
            SessionId = context.Invocation.SessionId,
            CreatedAt = now,
            UpdatedAt = now,
            Preview = $"contentLength: {content.Length}"
        };

        await context.Workspace.WriteTextAsync(path, content, artifact, cancellationToken);
        return MspCommandResult.Success(
            $"wrote\t{path}{Environment.NewLine}",
            new[] { artifact });
    }

    private static async ValueTask<MspCommandResult> RenameAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken)
    {
        if (arguments.Count != 2)
        {
            return MspCommandResult.Failure("Usage: artifact rename <source> <target>", exitCode: 2);
        }

        var sourcePath = NormalizeArtifactPath(context, arguments[0]);
        var targetPath = NormalizeArtifactPath(context, arguments[1]);
        if (string.Equals(sourcePath, targetPath, StringComparison.OrdinalIgnoreCase))
        {
            return MspCommandResult.Failure("Artifact source and target paths must differ.", exitCode: 2);
        }

        var content = await context.Workspace.TryReadTextAsync(sourcePath, cancellationToken);
        if (content is null)
        {
            return MspCommandResult.Failure($"Artifact not found: {sourcePath}", target: sourcePath);
        }

        if (await context.Workspace.ExistsAsync(targetPath, cancellationToken))
        {
            return MspCommandResult.Failure($"Artifact already exists: {targetPath}", exitCode: 2, target: targetPath);
        }

        var now = DateTimeOffset.UtcNow;
        var existing = await ReadArtifactManifestAsync(context, sourcePath, cancellationToken);
        var artifact = (existing ?? new MspArtifact
        {
            Path = sourcePath,
            MediaType = GuessMediaType(sourcePath),
            CreatedAt = now
        }) with
        {
            Path = targetPath,
            MediaType = string.IsNullOrWhiteSpace(existing?.MediaType) ? GuessMediaType(targetPath) : existing.MediaType,
            SizeBytes = content.Length,
            SourceCommand = context.Invocation.CommandText,
            Actor = context.Invocation.Actor,
            SessionId = context.Invocation.SessionId,
            UpdatedAt = now,
            Preview = $"renamedFrom: {sourcePath}"
        };

        await context.Workspace.WriteTextAsync(targetPath, content, artifact, cancellationToken);
        var removed = await context.Workspace.TryDeleteAsync(sourcePath, cancellationToken);
        if (!removed)
        {
            return MspCommandResult.Failure($"Artifact rename could not remove source: {sourcePath}", target: sourcePath);
        }

        return MspCommandResult.Success(
            $"renamed\t{sourcePath}\t{targetPath}{Environment.NewLine}",
            new[] { artifact });
    }

    private static async ValueTask<MspCommandResult> DeleteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken)
    {
        if (arguments.Count != 1)
        {
            return MspCommandResult.Failure("Usage: artifact delete <path>", exitCode: 2);
        }

        var path = NormalizeArtifactPath(context, arguments[0]);
        var removed = await context.Workspace.TryDeleteAsync(path, cancellationToken);
        return removed
            ? MspCommandResult.Success($"deleted\t{path}{Environment.NewLine}")
            : MspCommandResult.Failure($"Artifact not found: {path}", target: path);
    }

    private static string NormalizeArtifactPath(MspCommandContext context, string path)
    {
        var normalized = context.Workspace.NormalizePath(path, context.WorkingDirectory);
        if (normalized == "/artifacts")
        {
            throw new InvalidOperationException("Artifact path must include a file name.");
        }

        if (!normalized.StartsWith("/artifacts/", StringComparison.Ordinal))
        {
            normalized = context.Workspace.NormalizePath("/artifacts/" + path.TrimStart('/'));
        }

        if (TryResolveManifestPath(normalized, out var artifactPath))
        {
            normalized = artifactPath;
        }

        return normalized;
    }

    private static async ValueTask<MspArtifact?> ReadArtifactManifestAsync(
        MspCommandContext context,
        string artifactPath,
        CancellationToken cancellationToken)
    {
        var manifestContent = await context.Workspace.TryReadTextAsync(GetManifestPath(artifactPath), cancellationToken);
        if (string.IsNullOrWhiteSpace(manifestContent))
        {
            return null;
        }

        try
        {
            return JsonSerializer.Deserialize<MspArtifact>(manifestContent, JsonOptions);
        }
        catch (JsonException)
        {
            return null;
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

    private static string Usage()
    {
        return "Usage: artifact list [path] | artifact show <path> | artifact write <path> <content...> | artifact rename <source> <target> | artifact delete <path>";
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
}
