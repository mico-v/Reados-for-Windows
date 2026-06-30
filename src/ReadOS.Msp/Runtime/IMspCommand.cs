using ReadOS.Msp.Models;

namespace ReadOS.Msp.Runtime;

public interface IMspCommand
{
    string Name { get; }

    string Summary { get; }

    MspCommandMetadata Metadata => MspCommandMetadata.Create(Name, Summary);

    MspCommandMetadata GetMetadata(IReadOnlyList<string> arguments)
    {
        return Metadata;
    }

    MspCommandPreview GetPreview(IReadOnlyList<string> arguments)
    {
        return MspCommandPreview.Empty;
    }

    ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default);
}
