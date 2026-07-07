using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Hosting.Runtime;

public sealed record MspCommandHostComposition
{
    public required MspCommandRegistry Registry { get; init; }

    public string HostCommandPackName { get; init; } = string.Empty;

    public IReadOnlyList<string> CoreCommandNames { get; init; } = Array.Empty<string>();

    public IReadOnlyList<string> HostCommandNames { get; init; } = Array.Empty<string>();

    public IReadOnlyList<string> OverriddenCoreCommandNames { get; init; } = Array.Empty<string>();

    public IReadOnlyList<string> CommandNames { get; init; } = Array.Empty<string>();
}
