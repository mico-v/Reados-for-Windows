using System.Collections.ObjectModel;
using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsAttachmentQueueServiceTests
{
    [Fact]
    public void Snapshot_clones_attachments_without_reusing_instances_or_ids()
    {
        var source = CreateAttachment("source-id");
        var service = new ReadOsAttachmentQueueService();

        var snapshot = service.Snapshot(new[] { source });

        var clone = Assert.Single(snapshot);
        Assert.NotSame(source, clone);
        Assert.NotEqual(source.Id, clone.Id);
        Assert.Equal(source.Kind, clone.Kind);
        Assert.Equal(source.DocumentId, clone.DocumentId);
        Assert.Equal(source.Title, clone.Title);
        Assert.Equal(source.StartPage, clone.StartPage);
        Assert.Equal(source.EndPage, clone.EndPage);
        Assert.Equal(source.FilePath, clone.FilePath);
        Assert.Equal(source.RegionX, clone.RegionX);
        Assert.Equal(source.RegionY, clone.RegionY);
        Assert.Equal(source.RegionWidth, clone.RegionWidth);
        Assert.Equal(source.RegionHeight, clone.RegionHeight);
    }

    [Fact]
    public void RemoveById_removes_matching_attachment_case_insensitively()
    {
        var first = CreateAttachment("attachment-1");
        var second = CreateAttachment("attachment-2");
        var attachments = new ObservableCollection<ChatAttachment> { first, second };
        var service = new ReadOsAttachmentQueueService();

        var removed = service.RemoveById(attachments, new ChatAttachment { Id = "ATTACHMENT-1" });
        var missing = service.RemoveById(attachments, new ChatAttachment { Id = "missing" });
        var nullRemoved = service.RemoveById(attachments, null);

        Assert.Same(first, removed);
        Assert.Single(attachments);
        Assert.Same(second, attachments[0]);
        Assert.Null(missing);
        Assert.Null(nullRemoved);
    }

    [Fact]
    public void QueueDraft_creates_queued_prompt_with_cloned_attachments_and_clears_pending()
    {
        var pending = new ObservableCollection<ChatAttachment>
        {
            CreateAttachment("attachment-1")
        };
        var queuedPrompts = new ObservableCollection<QueuedComposerPrompt>();
        var createdAt = new DateTimeOffset(2026, 7, 7, 10, 0, 0, TimeSpan.Zero);
        var service = new ReadOsAttachmentQueueService();

        var queued = service.QueueDraft(queuedPrompts, pending, "  Review this later.  ", createdAt);

        Assert.NotNull(queued);
        Assert.Same(queued, Assert.Single(queuedPrompts));
        Assert.Equal("Review this later.", queued.Prompt);
        Assert.Equal(createdAt, queued.CreatedAt);
        Assert.Empty(pending);
        var queuedAttachment = Assert.Single(queued.Attachments);
        Assert.Equal("Artifact", queuedAttachment.Title);
        Assert.NotEqual("attachment-1", queuedAttachment.Id);
    }

    [Fact]
    public void QueueDraft_ignores_empty_prompt_without_attachments()
    {
        var service = new ReadOsAttachmentQueueService();
        var queuedPrompts = new ObservableCollection<QueuedComposerPrompt>();
        var pending = new ObservableCollection<ChatAttachment>();

        var queued = service.QueueDraft(queuedPrompts, pending, "   ", DateTimeOffset.UtcNow);

        Assert.Null(queued);
        Assert.Empty(queuedPrompts);
        Assert.Empty(pending);
    }

    [Fact]
    public void RestoreQueuedPrompt_restores_only_when_caller_allows_empty_composer_state()
    {
        var service = new ReadOsAttachmentQueueService();
        var pending = new ObservableCollection<ChatAttachment>();
        var queuedPrompts = new ObservableCollection<QueuedComposerPrompt>();
        var queued = new QueuedComposerPrompt
        {
            Prompt = "Continue review"
        };
        queued.Attachments.Add(CreateAttachment("queued-attachment"));
        queuedPrompts.Add(queued);

        Assert.True(service.CanRestore(string.Empty, pending));
        Assert.False(service.CanRestore("draft", pending));

        pending.Add(CreateAttachment("pending-attachment"));
        Assert.False(service.CanRestore(string.Empty, pending));
        pending.Clear();

        var prompt = service.RestoreQueuedPrompt(queuedPrompts, pending, queued);

        Assert.Equal("Continue review", prompt);
        Assert.Empty(queuedPrompts);
        var restored = Assert.Single(pending);
        Assert.Equal("Artifact", restored.Title);
        Assert.NotEqual("queued-attachment", restored.Id);
        Assert.Null(service.RestoreQueuedPrompt(queuedPrompts, pending, null));
    }

    [Fact]
    public void Clear_removes_all_attachments()
    {
        var attachments = new ObservableCollection<ChatAttachment>
        {
            CreateAttachment("attachment-1"),
            CreateAttachment("attachment-2")
        };
        var service = new ReadOsAttachmentQueueService();

        service.Clear(attachments);

        Assert.Empty(attachments);
    }

    [Fact]
    public void CreateArtifactAttachment_builds_virtual_file_attachment()
    {
        var service = new ReadOsAttachmentQueueService();
        var artifact = new WorkspaceArtifact
        {
            Path = "/artifacts/brief.md",
            Content = "brief"
        };

        var attachment = service.CreateArtifactAttachment(artifact, "doc-1");

        Assert.Equal(AttachmentKind.File, attachment.Kind);
        Assert.Equal("doc-1", attachment.DocumentId);
        Assert.Equal("产物 · /artifacts/brief.md", attachment.Title);
        Assert.Equal("/artifacts/brief.md", attachment.FilePath);
    }

    private static ChatAttachment CreateAttachment(string id)
    {
        return new ChatAttachment
        {
            Id = id,
            Kind = AttachmentKind.File,
            DocumentId = "doc-1",
            Title = "Artifact",
            StartPage = 2,
            EndPage = 4,
            FilePath = "/artifacts/brief.md",
            RegionX = 0.1,
            RegionY = 0.2,
            RegionWidth = 0.3,
            RegionHeight = 0.4
        };
    }
}
