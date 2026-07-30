namespace ReadOS.Msp.Hosting.Native;

public sealed record MspNativeShellParseRequest
{
    public required string CommandText { get; init; }
}

public sealed record MspNativeShellParseResult
{
    public required string ContractVersion { get; init; }

    public required bool Succeeded { get; init; }

    public MspNativeParsedShellScript? Script { get; init; }

    public MspNativeShellParseError? Error { get; init; }
}

public sealed record MspNativeParsedShellScript
{
    public required string RawInput { get; init; }

    public required IReadOnlyList<MspNativeParsedCommandPipeline> Pipelines { get; init; }
}

public enum MspNativeParsedListOperator
{
    Semicolon,
    And,
    Or
}

public enum MspNativeParsedPipeOperator
{
    Stdout,
    StdoutAndStderr
}

public sealed record MspNativeParsedCommandPipeline
{
    public MspNativeParsedListOperator? LeadingOperator { get; init; }

    public required bool IsNegated { get; init; }

    public required IReadOnlyList<MspNativeParsedCommandLine> Commands { get; init; }

    public required IReadOnlyList<MspNativeParsedPipeOperator> PipeOperators { get; init; }
}

public sealed record MspNativeParsedCommandLine
{
    public required string CommandName { get; init; }

    public required IReadOnlyList<string> Arguments { get; init; }

    public required IReadOnlyList<MspNativeParsedAssignment> Assignments { get; init; }

    public required IReadOnlyList<MspNativeParsedRedirection> Redirections { get; init; }

    public required bool IsAssignmentOnly { get; init; }

    public required string RawInput { get; init; }

    public MspNativeParsedWord? CommandNameWord { get; init; }

    public required IReadOnlyList<MspNativeParsedWord> ArgumentWords { get; init; }
}

public sealed record MspNativeParsedAssignment
{
    public required string Name { get; init; }

    public required string Value { get; init; }
}

public sealed record MspNativeParsedWord
{
    public required IReadOnlyList<MspNativeParsedWordPart> Parts { get; init; }

    public required bool HasExplicitEmptyQuotedFragment { get; init; }
}

public sealed record MspNativeParsedWordPart
{
    public required string Text { get; init; }

    public required bool IsExpandable { get; init; }

    public required bool IsQuoted { get; init; }
}

public enum MspNativeParsedRedirectionOperator
{
    Input,
    Output,
    AppendOutput,
    OutputBoth,
    AppendOutputBoth,
    DuplicateOutput,
    DuplicateInput,
    ReadWrite,
    ClobberOutput,
    HereDocument,
    HereDocumentStripTabs,
    HereString
}

public sealed record MspNativeParsedRedirection
{
    public uint? Fd { get; init; }

    public required MspNativeParsedRedirectionOperator Operation { get; init; }

    public required string Target { get; init; }

    public required MspNativeParsedWord TargetWord { get; init; }
}

public enum MspNativeShellParseErrorKind
{
    EmptyInput,
    Syntax
}

public sealed record MspNativeShellParseError
{
    public required MspNativeShellParseErrorKind Kind { get; init; }

    public required int ExitCode { get; init; }

    public required string Message { get; init; }
}

internal sealed record MspNativeShellParseRequestWire
{
    public string ContractVersion { get; init; } = MspNativeContract.Version;

    public required string CommandText { get; init; }
}
