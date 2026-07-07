namespace ReadOS.Msp.Hosting.Runtime;

internal enum MspCommandNameValidationError
{
    None,
    Empty,
    LeadingOrTrailingWhitespace,
    Whitespace,
    UnsupportedCharacters
}

internal static class MspCommandNameValidation
{
    public static MspCommandNameValidationError GetError(string? commandName)
    {
        if (string.IsNullOrWhiteSpace(commandName))
        {
            return MspCommandNameValidationError.Empty;
        }

        if (!string.Equals(commandName, commandName.Trim(), StringComparison.Ordinal))
        {
            return MspCommandNameValidationError.LeadingOrTrailingWhitespace;
        }

        if (commandName.Any(char.IsWhiteSpace))
        {
            return MspCommandNameValidationError.Whitespace;
        }

        if (commandName.Any(character => !IsCommandNameCharacter(character)))
        {
            return MspCommandNameValidationError.UnsupportedCharacters;
        }

        return MspCommandNameValidationError.None;
    }

    private static bool IsCommandNameCharacter(char character)
    {
        return char.IsAsciiLetterOrDigit(character) ||
            character is '-' or '_' or '.';
    }
}
