using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsChatTurnServiceTests
{
    [Fact]
    public void ResolvePrompt_trims_composer_draft_when_present()
    {
        var service = new ReadOsChatTurnService();

        var prompt = service.ResolvePrompt(
            "  explain this section  ",
            Array.Empty<ChatAttachment>(),
            "attachment prompt");

        Assert.Equal("explain this section", prompt);
    }

    [Fact]
    public void ResolvePrompt_uses_attachment_default_prompt_for_attachment_only_turn()
    {
        var service = new ReadOsChatTurnService();
        var attachments = new[]
        {
            new ChatAttachment
            {
                Kind = AttachmentKind.Page,
                StartPage = 2,
                EndPage = 2
            }
        };

        var prompt = service.ResolvePrompt("  ", attachments, "explain attachments");

        Assert.Equal("explain attachments", prompt);
    }

    [Fact]
    public void ResolvePrompt_uses_general_default_prompt_without_draft_or_attachments()
    {
        var service = new ReadOsChatTurnService();

        var prompt = service.ResolvePrompt("  ", Array.Empty<ChatAttachment>(), "attachment prompt");

        Assert.Equal(ReadOsChatTurnService.GeneralDefaultPrompt, prompt);
    }

    [Fact]
    public void CreateUserMessage_preserves_prompt_attachments_and_timestamp()
    {
        var service = new ReadOsChatTurnService();
        var createdAt = new DateTimeOffset(2026, 7, 7, 12, 0, 0, TimeSpan.Zero);
        var attachment = new ChatAttachment
        {
            Id = "attachment",
            Kind = AttachmentKind.File,
            Title = "Artifact",
            FilePath = "/artifacts/report.md"
        };

        var message = service.CreateUserMessage("summarize", new[] { attachment }, createdAt);

        Assert.Equal(ChatRole.User, message.Role);
        Assert.Equal("你", message.Author);
        Assert.Equal("summarize", message.Content);
        Assert.Equal(createdAt, message.CreatedAt);
        Assert.Single(message.Attachments);
        Assert.Same(attachment, message.Attachments[0]);
    }

    [Fact]
    public void CreateAssistantMessages_assign_expected_authors()
    {
        var service = new ReadOsChatTurnService();
        var createdAt = new DateTimeOffset(2026, 7, 7, 12, 5, 0, TimeSpan.Zero);

        var readOs = service.CreateReadOsMessage("answer", createdAt);
        var msp = service.CreateMspMessage("report", createdAt.AddMinutes(1));

        Assert.Equal(ChatRole.Assistant, readOs.Role);
        Assert.Equal("ReadOS", readOs.Author);
        Assert.Equal("answer", readOs.Content);
        Assert.Equal(createdAt, readOs.CreatedAt);
        Assert.Equal(ChatRole.Assistant, msp.Role);
        Assert.Equal("MSP", msp.Author);
        Assert.Equal("report", msp.Content);
        Assert.Equal(createdAt.AddMinutes(1), msp.CreatedAt);
    }

    [Fact]
    public void BuildMspFinalAnswerPrompt_reuses_original_prompt()
    {
        var service = new ReadOsChatTurnService();

        var prompt = service.BuildMspFinalAnswerPrompt("what changed?");

        Assert.Equal("请基于 MSP 执行结果回答用户原始问题：what changed?", prompt);
    }

    [Fact]
    public void CreateFailureMessage_formats_model_error()
    {
        var service = new ReadOsChatTurnService();
        var createdAt = new DateTimeOffset(2026, 7, 7, 12, 10, 0, TimeSpan.Zero);

        var message = service.CreateFailureMessage(new InvalidOperationException("network down"), createdAt);

        Assert.Equal(ChatRole.Assistant, message.Role);
        Assert.Equal("ReadOS", message.Author);
        Assert.Equal("请求失败：network down", message.Content);
        Assert.Equal(createdAt, message.CreatedAt);
    }
}
