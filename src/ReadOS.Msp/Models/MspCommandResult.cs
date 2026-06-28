namespace ReadOS.Msp.Models;

public sealed record MspCommandResult
{
    public int ExitCode { get; init; }

    public string Stdout { get; init; } = string.Empty;

    public string Stderr { get; init; } = string.Empty;

    public IReadOnlyList<MspArtifact> Artifacts { get; init; } = Array.Empty<MspArtifact>();

    public IReadOnlyList<MspAuditRecord> AuditRecords { get; init; } = Array.Empty<MspAuditRecord>();

    public bool Succeeded => ExitCode == 0;

    public static MspCommandResult Success(string stdout = "", IReadOnlyList<MspArtifact>? artifacts = null)
    {
        return new MspCommandResult
        {
            ExitCode = 0,
            Stdout = stdout,
            Artifacts = artifacts ?? Array.Empty<MspArtifact>()
        };
    }

    public static MspCommandResult Failure(string stderr, int exitCode = 1)
    {
        return new MspCommandResult
        {
            ExitCode = exitCode == 0 ? 1 : exitCode,
            Stderr = stderr
        };
    }
}
