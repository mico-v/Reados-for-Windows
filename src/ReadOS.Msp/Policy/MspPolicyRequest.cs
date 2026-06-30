using ReadOS.Msp.Models;

namespace ReadOS.Msp.Policy;

public sealed record MspPolicyRequest
{
    public required string CommandName { get; init; }

    public required string CommandText { get; init; }

    public string Actor { get; init; } = "agent";

    public string WorkingDirectory { get; init; } = "/";

    public bool DryRun { get; init; }

    public IReadOnlyList<string> Arguments { get; init; } = Array.Empty<string>();

    public MspCommandMetadata CommandMetadata { get; init; } = MspCommandMetadata.Create(
        "unknown",
        "No command metadata was supplied.");

    public MspCommandEffects Effects => CommandMetadata.Effects;

    public IReadOnlyList<string> Capabilities => CommandMetadata.Capabilities;

    public bool RequiresConfirmation => CommandMetadata.RequiresConfirmation;
}
