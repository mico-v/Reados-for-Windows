using ReadOS.Msp.Models;
using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Commands;

public sealed class PwdCommand : IMspCommand
{
    public string Name => "pwd";

    public string Summary => "Print the current virtual workspace directory.";

    public ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default)
    {
        return ValueTask.FromResult(MspCommandResult.Success(context.WorkingDirectory + Environment.NewLine));
    }
}
