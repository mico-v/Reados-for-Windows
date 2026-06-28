namespace ReadOS.Msp.Parsing;

public sealed record MspParsedCommand(string Name, IReadOnlyList<string> Arguments);
