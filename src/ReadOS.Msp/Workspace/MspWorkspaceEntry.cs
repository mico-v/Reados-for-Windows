namespace ReadOS.Msp.Workspace;

public sealed record MspWorkspaceEntry
{
    public required string Path { get; init; }

    public required string Name { get; init; }

    public bool IsDirectory { get; init; }

    public long? SizeBytes { get; init; }

    public string? MediaType { get; init; }
}
