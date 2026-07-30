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
        var tokenStarted = false;

        foreach (var character in commandText)
        {
            if (escaping)
            {
                current.Append(character);
                escaping = false;
                tokenStarted = true;
                continue;
            }

            if (character == '\\' && !inSingleQuote)
            {
                escaping = true;
                tokenStarted = true;
                continue;
            }

            if (character == '\'' && !inDoubleQuote)
            {
                inSingleQuote = !inSingleQuote;
                tokenStarted = true;
                continue;
            }

            if (character == '"' && !inSingleQuote)
            {
                inDoubleQuote = !inDoubleQuote;
                tokenStarted = true;
                continue;
            }

            if (!inSingleQuote && !inDoubleQuote)
            {
                if (char.IsWhiteSpace(character))
                {
                    FlushToken(tokens, current, ref tokenStarted);
                    continue;
                }

                if (character is '|' or ';' or '<' or '>')
                {
                    throw new MspParseException(
                        "Pipes, redirection, and compound shell forms are reserved for the next MSP runtime phase.");
                }
            }

            current.Append(character);
            tokenStarted = true;
        }

        if (escaping)
        {
            current.Append('\\');
            tokenStarted = true;
        }

        if (inSingleQuote || inDoubleQuote)
        {
            throw new MspParseException("Command text contains an unterminated quote.");
        }

        FlushToken(tokens, current, ref tokenStarted);
        if (tokens.Count == 0)
        {
            throw new MspParseException("Command text is empty.");
        }

        if (tokens[0].Length == 0)
        {
            throw new MspParseException("Command name is empty.");
        }

        return new MspParsedCommand(tokens[0], tokens.Skip(1).ToArray());
    }

    private static void FlushToken(
        ICollection<string> tokens,
        StringBuilder current,
        ref bool tokenStarted)
    {
        if (!tokenStarted)
        {
            return;
        }

        tokens.Add(current.ToString());
        current.Clear();
        tokenStarted = false;
    }
}
