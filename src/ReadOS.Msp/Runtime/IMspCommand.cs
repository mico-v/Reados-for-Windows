using ReadOS.Msp.Models;

namespace ReadOS.Msp.Runtime;

public interface IMspCommand
{
    string Name { get; }

    string Summary { get; }

    MspCommandMetadata Metadata => MspCommandMetadata.Create(Name, Summary);

    ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default);
}
