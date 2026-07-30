namespace ReadOS.Msp.Hosting.Native;

public sealed record MspNativeWorkspacePathRequest
{
    public required string Path { get; init; }

    public string CurrentDirectory { get; init; } = "/";
}

public sealed record MspNativeWorkspacePathResult
{
    public required string ContractVersion { get; init; }

    public required bool Succeeded { get; init; }

    public string? VirtualPath { get; init; }

    public string? Error { get; init; }
}

internal sealed record MspNativeWorkspacePathRequestWire
{
    public string ContractVersion { get; init; } = MspNativeContract.Version;

    public required string Path { get; init; }

    public required string CurrentDirectory { get; init; }
}
