using System.Text;
using ReadOS.Msp.Models;
using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Commands;

public sealed class LsCommand : IMspCommand
{
    public string Name => "ls";

    public string Summary => "List virtual workspace entries.";

    public async ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default)
    {
        var path = arguments.Count == 0 ? context.WorkingDirectory : arguments[0];
        var normalized = context.Workspace.NormalizePath(path, context.WorkingDirectory);
        var entries = await context.Workspace.ListAsync(normalized, cancellationToken);
        if (entries.Count == 0 && !await context.Workspace.ExistsAsync(normalized, cancellationToken))
        {
            return MspCommandResult.Failure($"No such file or directory: {normalized}", exitCode: 2);
        }

        var builder = new StringBuilder();
        foreach (var entry in entries.OrderBy(item => item.Name, StringComparer.OrdinalIgnoreCase))
        {
            builder.Append(entry.IsDirectory ? "d " : "- ");
            builder.Append(entry.Name);
            if (entry.SizeBytes is not null)
            {
                builder.Append('\t');
                builder.Append(entry.SizeBytes.Value);
            }

            builder.AppendLine();
        }

        return MspCommandResult.Success(builder.ToString());
    }
}
