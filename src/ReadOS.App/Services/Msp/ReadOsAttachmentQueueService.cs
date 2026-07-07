using System.Collections.ObjectModel;
using ReadOS.App.Models;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsAttachmentQueueService
{
    public IReadOnlyList<ChatAttachment> Snapshot(IEnumerable<ChatAttachment> attachments)
    {
        return attachments.Select(Clone).ToArray();
    }

    public ChatAttachment Clone(ChatAttachment source)
    {
        return new ChatAttachment
        {
            Kind = source.Kind,
            DocumentId = source.DocumentId,
            Title = source.Title,
            StartPage = source.StartPage,
            EndPage = source.EndPage,
            FilePath = source.FilePath,
            RegionX = source.RegionX,
            RegionY = source.RegionY,
            RegionWidth = source.RegionWidth,
            RegionHeight = source.RegionHeight
        };
    }

    public ChatAttachment? RemoveById(
        ObservableCollection<ChatAttachment> attachments,
        ChatAttachment? attachment)
    {
        if (attachment is null)
        {
            return null;
        }

        var existing = attachments.FirstOrDefault(item =>
            string.Equals(item.Id, attachment.Id, StringComparison.OrdinalIgnoreCase));
        if (existing is null)
        {
            return null;
        }

        attachments.Remove(existing);
        return existing;
    }

    public QueuedComposerPrompt? QueueDraft(
        ObservableCollection<QueuedComposerPrompt> queuedPrompts,
        ObservableCollection<ChatAttachment> pendingAttachments,
        string composerDraft,
        DateTimeOffset createdAt)
    {
        if (string.IsNullOrWhiteSpace(composerDraft) && pendingAttachments.Count == 0)
        {
            return null;
        }

        var queued = new QueuedComposerPrompt
        {
            Prompt = composerDraft.Trim(),
            CreatedAt = createdAt
        };
        foreach (var attachment in pendingAttachments)
        {
            queued.Attachments.Add(Clone(attachment));
        }

        queuedPrompts.Add(queued);
        pendingAttachments.Clear();
        return queued;
    }

    public bool CanRestore(string composerDraft, ObservableCollection<ChatAttachment> pendingAttachments)
    {
        return string.IsNullOrWhiteSpace(composerDraft) && pendingAttachments.Count == 0;
    }

    public string? RestoreQueuedPrompt(
        ObservableCollection<QueuedComposerPrompt> queuedPrompts,
        ObservableCollection<ChatAttachment> pendingAttachments,
        QueuedComposerPrompt? queued)
    {
        if (queued is null)
        {
            return null;
        }

        foreach (var attachment in queued.Attachments)
        {
            pendingAttachments.Add(Clone(attachment));
        }

        queuedPrompts.Remove(queued);
        return queued.Prompt;
    }

    public void Clear(ObservableCollection<ChatAttachment> attachments)
    {
        attachments.Clear();
    }

    public ChatAttachment CreateArtifactAttachment(WorkspaceArtifact artifact, string? documentId)
    {
        return new ChatAttachment
        {
            Kind = AttachmentKind.File,
            DocumentId = documentId ?? string.Empty,
            Title = $"产物 · {artifact.Path}",
            FilePath = artifact.Path
        };
    }
}
