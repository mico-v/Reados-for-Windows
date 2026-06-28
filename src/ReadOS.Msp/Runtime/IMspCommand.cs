using ReadOS.Msp.Models;

namespace ReadOS.Msp.Runtime;

public interface IMspCommand
{
    string Name { get; }

    string Summary { get; }

    ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default);
}
