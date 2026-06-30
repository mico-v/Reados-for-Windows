namespace ReadOS.Msp.Models;

public sealed record MspCommandResult
{
    public int ExitCode { get; init; }

    public string Stdout { get; init; } = string.Empty;

    public string Stderr { get; init; } = string.Empty;

    public IReadOnlyList<MspArtifact> Artifacts { get; init; } = Array.Empty<MspArtifact>();

    public IReadOnlyList<MspAuditRecord> AuditRecords { get; init; } = Array.Empty<MspAuditRecord>();

    public IReadOnlyList<MspCommandDiagnostic> Diagnostics { get; init; } = Array.Empty<MspCommandDiagnostic>();

    public bool Succeeded => ExitCode == 0;

    public static MspCommandResult Success(
        string stdout = "",
        IReadOnlyList<MspArtifact>? artifacts = null,
        IReadOnlyList<MspCommandDiagnostic>? diagnostics = null)
    {
        return new MspCommandResult
        {
            ExitCode = 0,
            Stdout = stdout,
            Artifacts = artifacts ?? Array.Empty<MspArtifact>(),
            Diagnostics = diagnostics ?? Array.Empty<MspCommandDiagnostic>()
        };
    }

    public static MspCommandResult Failure(
        string stderr,
        int exitCode = 1,
        string? code = null,
        string? target = null,
        string? recoveryHint = null,
        IReadOnlyList<MspCommandDiagnostic>? diagnostics = null)
    {
        var resolvedExitCode = exitCode == 0 ? 1 : exitCode;
        return new MspCommandResult
        {
            ExitCode = resolvedExitCode,
            Stderr = stderr,
            Diagnostics = diagnostics ?? new[]
            {
                MspCommandDiagnostic.Error(
                    code ?? GetDefaultDiagnosticCode(resolvedExitCode),
                    stderr,
                    target,
                    recoveryHint)
            }
        };
    }

    private static string GetDefaultDiagnosticCode(int exitCode)
    {
        return exitCode switch
        {
            2 => "msp.usage",
            126 => "msp.policy",
            127 => "msp.command_not_found",
            130 => "msp.canceled",
            _ => "msp.command_failed"
        };
    }
}
