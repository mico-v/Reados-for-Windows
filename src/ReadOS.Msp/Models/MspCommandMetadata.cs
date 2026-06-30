namespace ReadOS.Msp.Models;

public sealed record MspCommandMetadata
{
    public required string Name { get; init; }

    public required string Summary { get; init; }

    public string Usage { get; init; } = string.Empty;

    public MspCommandEffects Effects { get; init; } = MspCommandEffects.None;

    public IReadOnlyList<string> Capabilities { get; init; } = Array.Empty<string>();

    public bool RequiresConfirmation =>
        (Effects & (
            MspCommandEffects.WriteWorkspace |
            MspCommandEffects.DeleteWorkspace |
            MspCommandEffects.CreateArtifact |
            MspCommandEffects.ExternalNetwork |
            MspCommandEffects.ExternalModel)) != 0;

    public static MspCommandMetadata Create(
        string name,
        string summary,
        string usage = "",
        MspCommandEffects effects = MspCommandEffects.None,
        IReadOnlyList<string>? capabilities = null)
    {
        return new MspCommandMetadata
        {
            Name = name,
            Summary = summary,
            Usage = usage,
            Effects = effects,
            Capabilities = capabilities ?? Array.Empty<string>()
        };
    }
}
