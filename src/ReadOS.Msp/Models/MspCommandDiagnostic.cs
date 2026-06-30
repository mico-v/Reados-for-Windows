namespace ReadOS.Msp.Models;

public enum MspDiagnosticSeverity
{
    Info,
    Warning,
    Error
}

public sealed record MspCommandDiagnostic
{
    public MspDiagnosticSeverity Severity { get; init; } = MspDiagnosticSeverity.Error;

    public string Code { get; init; } = "msp.command_failed";

    public string Message { get; init; } = string.Empty;

    public string Target { get; init; } = string.Empty;

    public string RecoveryHint { get; init; } = string.Empty;

    public static MspCommandDiagnostic Error(
        string code,
        string message,
        string? target = null,
        string? recoveryHint = null)
    {
        return new MspCommandDiagnostic
        {
            Severity = MspDiagnosticSeverity.Error,
            Code = string.IsNullOrWhiteSpace(code) ? "msp.command_failed" : code,
            Message = message,
            Target = target ?? string.Empty,
            RecoveryHint = recoveryHint ?? string.Empty
        };
    }

    public string ToDisplayText()
    {
        var prefix = $"{Severity.ToString().ToLowerInvariant()} {Code}";
        var target = string.IsNullOrWhiteSpace(Target) ? string.Empty : $" [{Target}]";
        var message = string.IsNullOrWhiteSpace(Message) ? string.Empty : $": {Message}";
        var text = prefix + target + message;
        return string.IsNullOrWhiteSpace(RecoveryHint)
            ? text
            : text + Environment.NewLine + "recovery: " + RecoveryHint;
    }
}
