namespace ReadOS.App.Services;

public sealed class AiChatServiceException : Exception
{
    public AiChatServiceException(string message)
        : base(message)
    {
    }

    public AiChatServiceException(string message, Exception innerException)
        : base(message, innerException)
    {
    }
}
