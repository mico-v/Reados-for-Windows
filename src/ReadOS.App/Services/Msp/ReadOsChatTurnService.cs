using ReadOS.App.Models;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsChatTurnService
{
    public const string GeneralDefaultPrompt = "请总结当前文档。";

    public string ResolvePrompt(
        string composerDraft,
        IReadOnlyCollection<ChatAttachment> attachments,
        string attachmentDefaultPrompt)
    {
        if (!string.IsNullOrWhiteSpace(composerDraft))
        {
            return composerDraft.Trim();
        }

        return attachments.Count > 0 ? attachmentDefaultPrompt : GeneralDefaultPrompt;
    }

    public ChatMessage CreateUserMessage(
        string prompt,
        IEnumerable<ChatAttachment> attachments,
        DateTimeOffset createdAt)
    {
        var message = new ChatMessage
        {
            Role = ChatRole.User,
            Author = "你",
            Content = prompt,
            CreatedAt = createdAt
        };

        foreach (var attachment in attachments)
        {
            message.Attachments.Add(attachment);
        }

        return message;
    }

    public ChatMessage CreateReadOsMessage(string content, DateTimeOffset createdAt)
    {
        return CreateAssistantMessage("ReadOS", content, createdAt);
    }

    public ChatMessage CreateMspMessage(string content, DateTimeOffset createdAt)
    {
        return CreateAssistantMessage("MSP", content, createdAt);
    }

    public string BuildMspFinalAnswerPrompt(string originalPrompt)
    {
        return $"请基于 MSP 执行结果回答用户原始问题：{originalPrompt}";
    }

    public ChatMessage CreateFailureMessage(Exception exception, DateTimeOffset createdAt)
    {
        return CreateReadOsMessage($"请求失败：{exception.Message}", createdAt);
    }

    private static ChatMessage CreateAssistantMessage(string author, string content, DateTimeOffset createdAt)
    {
        return new ChatMessage
        {
            Role = ChatRole.Assistant,
            Author = author,
            Content = content,
            CreatedAt = createdAt
        };
    }
}
