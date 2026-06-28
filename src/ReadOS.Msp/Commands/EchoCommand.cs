using ReadOS.Msp.Models;
using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Commands;

public sealed class EchoCommand : IMspCommand
{
    public string Name => "echo";

    public string Summary => "Write arguments to stdout.";

    public ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default)
    {
        return ValueTask.FromResult(MspCommandResult.Success(string.Join(' ', arguments) + Environment.NewLine));
    }
}
