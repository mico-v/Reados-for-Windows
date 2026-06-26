using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;
using ReadOS.App.Models;

namespace ReadOS.App.Services;

public sealed class AiChatService : IAiChatService
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);
    private readonly HttpClient httpClient = new();

    public async Task<string> SendAsync(
        WorkspaceSettings settings,
        LibraryItem? document,
        IEnumerable<ChatMessage> history,
        string userPrompt,
        IEnumerable<ChatAttachment> attachments,
        Func<ChatAttachment, Task<string>> attachmentTextProvider,
        CancellationToken cancellationToken = default)
    {
        var attachmentContext = await BuildAttachmentContextAsync(attachments, attachmentTextProvider);
        if (settings.UseOfflineResponses || string.IsNullOrWhiteSpace(settings.ProviderApiKey))
        {
            return BuildOfflineResponse(document, userPrompt, attachmentContext);
        }

        var messages = new List<object>
        {
            new
            {
                role = "system",
                content = "你是 ReadOS 的阅读助手。回答应基于用户问题、当前 PDF 上下文和附件页文本；不确定时直接说明。"
            }
        };

        foreach (var message in history.TakeLast(12))
        {
            if (string.IsNullOrWhiteSpace(message.Content))
            {
                continue;
            }

            messages.Add(new
            {
                role = message.Role == ChatRole.Assistant ? "assistant" : "user",
                content = message.Content
            });
        }

        var prompt = string.IsNullOrWhiteSpace(attachmentContext)
            ? userPrompt
            : $"{userPrompt}\n\n以下是 ReadOS 附件上下文：\n{attachmentContext}";
        messages.Add(new { role = "user", content = prompt });

        var baseUrl = settings.ProviderBaseUrl.Trim().TrimEnd('/');
        var endpoint = baseUrl.EndsWith("/chat/completions", StringComparison.OrdinalIgnoreCase)
            ? baseUrl
            : baseUrl + "/chat/completions";

        using var request = new HttpRequestMessage(HttpMethod.Post, endpoint);
        request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", settings.ProviderApiKey.Trim());
        request.Content = new StringContent(JsonSerializer.Serialize(new
        {
            model = settings.ModelName,
            messages,
            temperature = 0.2
        }, JsonOptions), Encoding.UTF8, "application/json");

        using var response = await httpClient.SendAsync(request, cancellationToken);
        var responseText = await response.Content.ReadAsStringAsync(cancellationToken);
        if (!response.IsSuccessStatusCode)
        {
            return $"模型请求失败：{(int)response.StatusCode} {response.ReasonPhrase}\n{Trim(responseText, 1200)}";
        }

        using var documentJson = JsonDocument.Parse(responseText);
        var content = documentJson.RootElement
            .GetProperty("choices")[0]
            .GetProperty("message")
            .GetProperty("content")
            .GetString();

        return string.IsNullOrWhiteSpace(content) ? "模型没有返回文本内容。" : content.Trim();
    }

    private static async Task<string> BuildAttachmentContextAsync(
        IEnumerable<ChatAttachment> attachments,
        Func<ChatAttachment, Task<string>> attachmentTextProvider)
    {
        var builder = new StringBuilder();
        foreach (var attachment in attachments)
        {
            var text = await attachmentTextProvider(attachment);
            if (string.IsNullOrWhiteSpace(text))
            {
                continue;
            }

            builder.AppendLine($"## {attachment.Title}");
            builder.AppendLine(Trim(text, 7000));
            builder.AppendLine();
        }

        return builder.ToString().Trim();
    }

    private static string BuildOfflineResponse(LibraryItem? document, string userPrompt, string attachmentContext)
    {
        var builder = new StringBuilder();
        builder.AppendLine("当前使用离线阅读模式。ReadOS 已保留你的问题和附件，并基于本地可读取文本生成以下整理：");
        builder.AppendLine();
        builder.AppendLine($"问题：{userPrompt}");

        if (document is not null)
        {
            builder.AppendLine($"文档：{document.Name}，共 {document.PageCount} 页，当前位置第 {document.CurrentPage} 页。");
        }

        if (!string.IsNullOrWhiteSpace(attachmentContext))
        {
            builder.AppendLine();
            builder.AppendLine("附件文本摘录：");
            builder.AppendLine(Trim(attachmentContext, 1800));
            builder.AppendLine();
            builder.AppendLine("要得到完整模型解释，请在设置中关闭离线回答并填写 OpenAI 兼容 API Key。当前附件、页码和对话记录会直接用于真实请求。");
        }
        else
        {
            builder.AppendLine();
            builder.AppendLine("这条消息没有附件上下文。你可以先使用“附加本页”或“附加范围”，再发送问题。若已配置模型，也可以关闭离线回答直接请求模型。");
        }

        return builder.ToString().Trim();
    }

    private static string Trim(string value, int maxLength)
    {
        if (value.Length <= maxLength)
        {
            return value;
        }

        return value[..maxLength] + "...";
    }
}
