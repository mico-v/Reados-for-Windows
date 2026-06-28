using System.Collections.ObjectModel;

namespace ReadOS.Msp.Models;

public sealed record MspCommandRequest
{
    public required string CommandText { get; init; }

    public string WorkingDirectory { get; init; } = "/";

    public string Actor { get; init; } = "agent";

    public bool DryRun { get; init; }

    public IReadOnlyDictionary<string, string> Environment { get; init; } =
        ReadOnlyDictionary<string, string>.Empty;
}
