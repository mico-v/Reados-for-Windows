namespace ReadOS.Msp.Parsing;

public sealed class MspParseException : Exception
{
    public MspParseException(string message)
        : base(message)
    {
    }
}
