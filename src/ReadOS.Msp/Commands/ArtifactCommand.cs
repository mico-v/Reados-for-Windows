using System.Text;
using ReadOS.Msp.Models;
using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Commands;

public sealed class ArtifactCommand : IMspCommand
{
    public string Name => "artifact";

    public string Summary => "List, show, or write MSP artifacts in the virtual workspace.";

    public MspCommandMetadata Metadata => MspCommandMetadata.Create(
        Name,
        Summary,
        "artifact list [path] | artifact show <path> | artifact write <path> <content...>",
        MspCommandEffects.ReadWorkspace | MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact,
        new[] { "msp.artifact.read", "msp.artifact.write" });

    public MspCommandMetadata GetMetadata(IReadOnlyList<string> arguments)
    {
        if (arguments.Count > 0 && string.Equals(arguments[0], "write", StringComparison.OrdinalIgnoreCase))
        {
            return MspCommandMetadata.Create(
                Name,
                Summary,
                "artifact write <path> <content...>",
                MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact,
                new[] { "msp.artifact.write" });
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
        await context.Workspace.WriteTextAsync(path, content, cancellationToken);
        return MspCommandResult.Success(
            $"wrote\t{path}{Environment.NewLine}",
            new[] { new MspArtifact { Path = path, Description = "Text artifact created by MSP." } });
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

        return normalized;
    }

    private static string Usage()
    {
        return "Usage: artifact list [path] | artifact show <path> | artifact write <path> <content...>";
    }
}
