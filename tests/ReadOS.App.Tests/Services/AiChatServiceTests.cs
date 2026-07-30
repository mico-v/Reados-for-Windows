using ReadOS.App.Models;
using ReadOS.App.Services;

namespace ReadOS.App.Tests.Services;

public sealed class AiChatServiceTests
{
    [Fact]
    public async Task Missing_credential_falls_back_to_offline_response_without_provider_request()
    {
        var service = new AiChatService();
        var settings = new WorkspaceSettings
        {
            UseOfflineResponses = false,
            ProviderApiKey = string.Empty
        };

        var response = await service.SendAsync(
            settings,
            document: null,
            history: [],
            userPrompt: "Summarize the current context.",
            attachments: [],
            attachmentTextProvider: _ => Task.FromResult(string.Empty));

        Assert.Contains("离线阅读模式", response, StringComparison.Ordinal);
    }
}
