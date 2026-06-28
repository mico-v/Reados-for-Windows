using System.Text;
using ReadOS.Msp.Models;
using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Commands;

public sealed class HelpCommand : IMspCommand
{
    private readonly MspCommandRegistry registry;

    public HelpCommand(MspCommandRegistry registry)
    {
        this.registry = registry;
    }

    public string Name => "help";

    public string Summary => "List registered MSP commands.";

    public ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default)
    {
        var builder = new StringBuilder();
        foreach (var command in registry.Commands.OrderBy(item => item.Name, StringComparer.OrdinalIgnoreCase))
        {
            builder.Append(command.Name);
            builder.Append('\t');
            builder.AppendLine(command.Summary);
        }

        return ValueTask.FromResult(MspCommandResult.Success(builder.ToString()));
    }
}
