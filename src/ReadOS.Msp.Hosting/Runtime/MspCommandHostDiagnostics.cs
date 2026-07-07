namespace ReadOS.Msp.Hosting.Runtime;

public sealed record MspCommandHostDiagnostics
{
    public string DefaultSessionId { get; init; } = string.Empty;

    public string DefaultActor { get; init; } = string.Empty;

    public string DefaultWorkingDirectory { get; init; } = "/";

    public string HostCommandPackName { get; init; } = string.Empty;

    public int CoreCommandCount { get; init; }

    public int HostCommandCount { get; init; }

    public int CommandCount { get; init; }

    public IReadOnlyList<string> HostCommandNames { get; init; } = Array.Empty<string>();

    public IReadOnlyList<string> OverriddenCoreCommandNames { get; init; } = Array.Empty<string>();

    public bool HasCoreOverrides => OverriddenCoreCommandNames.Count > 0;
}
