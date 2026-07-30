using System.Collections.ObjectModel;
using System.Collections.Immutable;
using System.Text;
using ReadOS.Msp.Models;

namespace ReadOS.Msp.Hosting.Native;

public sealed record MspNativeCommandRequest
{
    public required string CommandText { get; init; }

    public string WorkingDirectory { get; init; } = "/";

    public string Actor { get; init; } = "agent";

    public string SessionId { get; init; } = "default";

    public bool DryRun { get; init; }

    public IReadOnlyDictionary<string, string> Environment { get; init; } =
        ReadOnlyDictionary<string, string>.Empty;

    public string? WorkspaceRoot { get; init; }
}

public sealed class MspNativeCommandResult
{
    private static readonly Encoding DisplayEncoding = new UTF8Encoding(
        encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: false);

    private readonly ImmutableArray<byte> stdoutBytes;
    private readonly ImmutableArray<byte> stderrBytes;

    internal MspNativeCommandResult(
        int exitCode,
        byte[] stdoutBytes,
        byte[] stderrBytes,
        MspNativeCommandRuntimeStateChange? stateChange,
        IReadOnlyList<MspNativeAuditRecord> auditRecords,
        IReadOnlyList<MspNativeDiagnostic> diagnostics)
    {
        ExitCode = exitCode;
        this.stdoutBytes = ImmutableArray.CreateRange(stdoutBytes);
        this.stderrBytes = ImmutableArray.CreateRange(stderrBytes);
        StateChange = stateChange is null ? null : stateChange with { };
        Diagnostics = Array.AsReadOnly(diagnostics.Select(FreezeDiagnostic).ToArray());
        AuditRecords = Array.AsReadOnly(auditRecords.Select(FreezeAudit).ToArray());
    }

    public string ContractVersion => MspNativeContract.Version;

    public int ExitCode { get; }

    public ImmutableArray<byte> StdoutBytes => stdoutBytes;

    public ImmutableArray<byte> StderrBytes => stderrBytes;

    public string StdoutText => DisplayEncoding.GetString(stdoutBytes.AsSpan());

    public string StderrText => DisplayEncoding.GetString(stderrBytes.AsSpan());

    public MspNativeCommandRuntimeStateChange? StateChange { get; }

    public IReadOnlyList<MspNativeAuditRecord> AuditRecords { get; }

    public IReadOnlyList<MspNativeDiagnostic> Diagnostics { get; }

    public bool Succeeded => ExitCode == 0;

    private static MspNativeDiagnostic FreezeDiagnostic(MspNativeDiagnostic diagnostic)
    {
        return diagnostic with { };
    }

    private static MspNativeAuditRecord FreezeAudit(MspNativeAuditRecord audit)
    {
        return audit with
        {
            Arguments = Array.AsReadOnly(audit.Arguments.ToArray()),
            PolicyDecision = audit.PolicyDecision with { },
            Diagnostics = Array.AsReadOnly(audit.Diagnostics.Select(FreezeDiagnostic).ToArray())
        };
    }
}

public sealed record MspNativeCommandRuntimeStateChange
{
    public string? CurrentDirectory { get; init; }
}

public enum MspNativeDiagnosticSeverity
{
    Info,
    Warning,
    Error
}

public sealed record MspNativeDiagnostic
{
    public required MspNativeDiagnosticSeverity Severity { get; init; }

    public required string Code { get; init; }

    public required string Message { get; init; }

    public string? Target { get; init; }

    public string? RecoveryHint { get; init; }

    public MspCommandDiagnostic ToManagedDiagnostic()
    {
        return new MspCommandDiagnostic
        {
            Severity = Severity switch
            {
                MspNativeDiagnosticSeverity.Info => MspDiagnosticSeverity.Info,
                MspNativeDiagnosticSeverity.Warning => MspDiagnosticSeverity.Warning,
                _ => MspDiagnosticSeverity.Error
            },
            Code = Code,
            Message = Message,
            Target = Target ?? string.Empty,
            RecoveryHint = RecoveryHint ?? string.Empty
        };
    }
}

public enum MspNativePolicyDecisionKind
{
    Allow,
    Deny,
    RequiresConfirmation,
    NotEvaluated
}

public sealed record MspNativePolicyDecision
{
    public required MspNativePolicyDecisionKind Kind { get; init; }

    public string? Reason { get; init; }

    public string? Prompt { get; init; }
}

public sealed record MspNativeAuditRecord
{
    public required string RunId { get; init; }

    public required string CommandLine { get; init; }

    public string? CommandName { get; init; }

    public required IReadOnlyList<string> Arguments { get; init; }

    public required int ExitCode { get; init; }

    public required ulong StartedAtUnixMs { get; init; }

    public required ulong EndedAtUnixMs { get; init; }

    public required string Actor { get; init; }

    public required string SessionId { get; init; }

    public required string WorkingDirectory { get; init; }

    public required MspNativePolicyDecision PolicyDecision { get; init; }

    public required IReadOnlyList<MspNativeDiagnostic> Diagnostics { get; init; }
}

internal sealed record MspNativeCommandRequestWire
{
    public string ContractVersion { get; init; } = MspNativeContract.Version;

    public required string CommandText { get; init; }

    public required string WorkingDirectory { get; init; }

    public required string Actor { get; init; }

    public required string SessionId { get; init; }

    public bool DryRun { get; init; }

    public required IReadOnlyDictionary<string, string> Environment { get; init; }

    public string? WorkspaceRoot { get; init; }
}

internal sealed record MspNativeCommandResultWire
{
    public string? ContractVersion { get; init; }

    public string? Stdout { get; init; }

    public string? Stderr { get; init; }

    public string? StdoutBytesBase64 { get; init; }

    public string? StderrBytesBase64 { get; init; }

    public int? ExitCode { get; init; }

    public MspNativeCommandRuntimeStateChange? StateChange { get; init; }

    public IReadOnlyList<MspNativeAuditRecord>? AuditRecords { get; init; }

    public IReadOnlyList<MspNativeDiagnostic>? Diagnostics { get; init; }
}
