namespace ReadOS.Msp.Hosting.Native;

public enum MspNativeCommandRouteDecisionKind
{
    ExecuteNative,
    ParserDisagreement,
    UnsupportedShellForm,
    CommandMismatch
}

public sealed record MspNativeCommandRouteDecision
{
    public required MspNativeCommandRouteDecisionKind Kind { get; init; }

    public required string DiagnosticCode { get; init; }

    public required string Message { get; init; }

    public required string RecoveryHint { get; init; }

    public bool ShouldExecuteNative => Kind == MspNativeCommandRouteDecisionKind.ExecuteNative;
}

public sealed class MspNativeCommandRouteClassifier
{
    public MspNativeCommandRouteDecision Classify(
        string canonicalCommandName,
        string rawCommandText,
        IReadOnlyList<string> managedArguments,
        MspNativeShellParseResult parseResult)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(canonicalCommandName);
        ArgumentNullException.ThrowIfNull(rawCommandText);
        ArgumentNullException.ThrowIfNull(managedArguments);
        ArgumentNullException.ThrowIfNull(parseResult);

        if (!parseResult.Succeeded || parseResult.Script is null ||
            !string.Equals(
                parseResult.Script.RawInput,
                rawCommandText,
                StringComparison.Ordinal))
        {
            return Reject(
                MspNativeCommandRouteDecisionKind.ParserDisagreement,
                "msp.native.route.parser_disagreement",
                "The managed and native shell parsers did not agree on this command.",
                "Use a canonical simple command without ambiguous quoting or shell syntax.");
        }

        if (parseResult.Script.Pipelines.Count != 1)
        {
            return UnsupportedForm();
        }

        var pipeline = parseResult.Script.Pipelines[0];
        if (pipeline.LeadingOperator is not null ||
            pipeline.IsNegated ||
            pipeline.Commands.Count != 1 ||
            pipeline.PipeOperators.Count != 0)
        {
            return UnsupportedForm();
        }

        var command = pipeline.Commands[0];
        if (rawCommandText.IndexOfAny(['\r', '\n']) >= 0 ||
            ContainsUnquotedAmpersand(command) ||
            command.IsAssignmentOnly ||
            command.Assignments.Count != 0 ||
            command.Redirections.Count != 0)
        {
            return UnsupportedForm();
        }

        if (!string.Equals(
                command.CommandName,
                canonicalCommandName,
                StringComparison.Ordinal))
        {
            return Reject(
                MspNativeCommandRouteDecisionKind.CommandMismatch,
                "msp.native.route.command_mismatch",
                "The native shell command did not match the selected runtime command.",
                "Use the exact canonical lowercase command name.");
        }

        if (!command.Arguments.SequenceEqual(managedArguments, StringComparer.Ordinal))
        {
            return Reject(
                MspNativeCommandRouteDecisionKind.ParserDisagreement,
                "msp.native.route.parser_disagreement",
                "The managed and native shell parsers produced different command arguments.",
                "Use quoting and whitespace that both runtime parsers interpret identically.");
        }

        return new MspNativeCommandRouteDecision
        {
            Kind = MspNativeCommandRouteDecisionKind.ExecuteNative,
            DiagnosticCode = string.Empty,
            Message = string.Empty,
            RecoveryHint = string.Empty
        };
    }

    private static bool ContainsUnquotedAmpersand(
        MspNativeParsedCommandLine command)
    {
        IEnumerable<MspNativeParsedWord> words = command.ArgumentWords;
        if (command.CommandNameWord is { } commandNameWord)
        {
            words = words.Prepend(commandNameWord);
        }

        return words.Any(word => word.Parts.Any(part =>
            !part.IsQuoted &&
            part.IsExpandable &&
            part.Text.Contains('&', StringComparison.Ordinal)));
    }

    private static MspNativeCommandRouteDecision UnsupportedForm()
    {
        return Reject(
            MspNativeCommandRouteDecisionKind.UnsupportedShellForm,
            "msp.native.route.unsupported_shell_form",
            "This native command route accepts one simple command without pipelines, lists, assignments, negation, or redirection.",
            "Run a single canonical pwd or echo command; keep compound workflows in the managed command surface.");
    }

    private static MspNativeCommandRouteDecision Reject(
        MspNativeCommandRouteDecisionKind kind,
        string diagnosticCode,
        string message,
        string recoveryHint)
    {
        return new MspNativeCommandRouteDecision
        {
            Kind = kind,
            DiagnosticCode = diagnosticCode,
            Message = message,
            RecoveryHint = recoveryHint
        };
    }
}
