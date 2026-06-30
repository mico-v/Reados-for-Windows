using System.Collections.ObjectModel;
using ReadOS.Msp.Models;

namespace ReadOS.Msp.Policy;

public sealed record MspPolicyRequest
{
    public required string CommandName { get; init; }

    public required string CommandText { get; init; }

    public string Actor { get; init; } = "agent";

    public string SessionId { get; init; } = "default";

    public string WorkingDirectory { get; init; } = "/";

    public bool DryRun { get; init; }

    public IReadOnlyDictionary<string, string> Environment { get; init; } =
        ReadOnlyDictionary<string, string>.Empty;

    public IReadOnlyList<string> Arguments { get; init; } = Array.Empty<string>();

    public MspCommandMetadata CommandMetadata { get; init; } = MspCommandMetadata.Create(
        "unknown",
        "No command metadata was supplied.");

    public MspCommandPreview Preview { get; init; } = MspCommandPreview.Empty;

    public MspCommandEffects Effects => CommandMetadata.Effects;

    public IReadOnlyList<string> Capabilities => CommandMetadata.Capabilities;

    public bool RequiresConfirmation => CommandMetadata.RequiresConfirmation;
}
