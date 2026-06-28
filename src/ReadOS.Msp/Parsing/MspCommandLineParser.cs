using System.Text;

namespace ReadOS.Msp.Parsing;

public static class MspCommandLineParser
{
    public static MspParsedCommand Parse(string commandText)
    {
        if (string.IsNullOrWhiteSpace(commandText))
        {
            throw new MspParseException("Command text is empty.");
        }

        var tokens = new List<string>();
        var current = new StringBuilder();
        var inSingleQuote = false;
        var inDoubleQuote = false;
        var escaping = false;

        foreach (var character in commandText)
        {
            if (escaping)
            {
                current.Append(character);
                escaping = false;
                continue;
            }

            if (character == '\\' && !inSingleQuote)
            {
                escaping = true;
                continue;
            }

            if (character == '\'' && !inDoubleQuote)
            {
                inSingleQuote = !inSingleQuote;
                continue;
            }

            if (character == '"' && !inSingleQuote)
            {
                inDoubleQuote = !inDoubleQuote;
                continue;
            }

            if (!inSingleQuote && !inDoubleQuote)
            {
                if (char.IsWhiteSpace(character))
                {
                    FlushToken(tokens, current);
                    continue;
                }

                if (character is '|' or ';' or '<' or '>')
                {
                    throw new MspParseException(
                        "Pipes, redirection, and compound shell forms are reserved for the next MSP runtime phase.");
                }
            }

            current.Append(character);
        }

        if (escaping)
        {
            current.Append('\\');
        }

        if (inSingleQuote || inDoubleQuote)
        {
            throw new MspParseException("Command text contains an unterminated quote.");
        }

        FlushToken(tokens, current);
        if (tokens.Count == 0)
        {
            throw new MspParseException("Command text is empty.");
        }

        return new MspParsedCommand(tokens[0], tokens.Skip(1).ToArray());
    }

    private static void FlushToken(ICollection<string> tokens, StringBuilder current)
    {
        if (current.Length == 0)
        {
            return;
        }

        tokens.Add(current.ToString());
        current.Clear();
    }
}
