using ReadOS.Msp.Models;
using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Commands;

public sealed class CatCommand : IMspCommand
{
    public string Name => "cat";

    public string Summary => "Read a text file from the virtual workspace.";

    public MspCommandMetadata Metadata => MspCommandMetadata.Create(
        Name,
        Summary,
        "cat <path> [path...]",
        MspCommandEffects.ReadWorkspace);

    public async ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default)
    {
        if (arguments.Count == 0)
        {
            return MspCommandResult.Failure("Usage: cat <path>", exitCode: 2);
        }

        var builder = new System.Text.StringBuilder();
        foreach (var argument in arguments)
        {
            var path = context.Workspace.NormalizePath(argument, context.WorkingDirectory);
            var content = await context.Workspace.TryReadTextAsync(path, cancellationToken);
            if (content is null)
            {
                return MspCommandResult.Failure($"Cannot read text file: {path}", exitCode: 1);
            }

            builder.Append(content);
            if (!content.EndsWith(Environment.NewLine, StringComparison.Ordinal))
            {
                builder.AppendLine();
            }
        }

        return MspCommandResult.Success(builder.ToString());
    }
}
