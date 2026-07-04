using System;
using System.Collections.ObjectModel;
using System.Text.Json.Serialization;

namespace ReadOS.App.Models;

public enum TimelineItemKind
{
    Message,
    MspCommand,
    Approval,
    Artifact,
    Evidence,
    Error
}

public sealed class ThreadTimelineItem
{
    public TimelineItemKind Kind { get; init; }

    public ChatMessage? Message { get; init; }

    public MspTranscriptEntry? MspEntry { get; init; }

    public WorkspaceArtifact? Artifact { get; init; }

    public DateTimeOffset CreatedAt { get; init; }

    public string Title { get; init; } = string.Empty;

    public string Subtitle { get; init; } = string.Empty;

    public string Body { get; init; } = string.Empty;

    public string StatusLabel { get; init; } = string.Empty;

    public string Glyph { get; init; } = "\uE8BD";

    public ObservableCollection<ChatAttachment> Attachments { get; } = new();

    [JsonIgnore]
    public string Timestamp => CreatedAt.ToLocalTime().ToString("HH:mm:ss");

    [JsonIgnore]
    public bool HasBody => !string.IsNullOrWhiteSpace(Body);

    [JsonIgnore]
    public bool HasAttachments => Attachments.Count > 0;

    [JsonIgnore]
    public bool HasMspEntry => MspEntry is not null;

    [JsonIgnore]
    public bool HasArtifact => Artifact is not null;

    [JsonIgnore]
    public bool IsApprovalRequired => MspEntry?.IsApprovalRequired == true;

    [JsonIgnore]
    public bool IsRunning => MspEntry?.IsRunning == true;

    [JsonIgnore]
    public bool HasPolicyPreview => !string.IsNullOrWhiteSpace(MspEntry?.PolicyPreview);

    [JsonIgnore]
    public bool HasArtifactsSummary => !string.IsNullOrWhiteSpace(MspEntry?.ArtifactsSummary);

    [JsonIgnore]
    public bool HasDiagnostics => !string.IsNullOrWhiteSpace(MspEntry?.DiagnosticsSummary);

    [JsonIgnore]
    public bool HasRecoveryHint => !string.IsNullOrWhiteSpace(MspEntry?.RecoveryHint);

    public static ThreadTimelineItem FromMessage(ChatMessage message)
    {
        var item = new ThreadTimelineItem
        {
            Kind = message.Role == ChatRole.System ? TimelineItemKind.Evidence : TimelineItemKind.Message,
            Message = message,
            CreatedAt = message.CreatedAt,
            Title = message.Author,
            Subtitle = message.IsAssistant ? "assistant response" : "user prompt",
            Body = message.Content,
            StatusLabel = message.Role.ToString(),
            Glyph = message.IsAssistant ? "\uE8BD" : "\uE77B"
        };

        foreach (var attachment in message.Attachments)
        {
            item.Attachments.Add(attachment);
        }

        return item;
    }

    public static ThreadTimelineItem FromMsp(MspTranscriptEntry entry)
    {
        var kind = entry.IsApprovalRequired
            ? TimelineItemKind.Approval
            : entry.Succeeded || entry.IsRunning || entry.WasCanceled
                ? TimelineItemKind.MspCommand
                : TimelineItemKind.Error;

        return new ThreadTimelineItem
        {
            Kind = kind,
            MspEntry = entry,
            CreatedAt = entry.StartedAt,
            Title = entry.IsApprovalRequired ? "MSP approval required" : "MSP command",
            Subtitle = entry.Actor,
            Body = entry.OutputPreview,
            StatusLabel = entry.StatusLabel,
            Glyph = entry.IsApprovalRequired ? "\uE7BA" : entry.Succeeded ? "\uE756" : "\uEA39"
        };
    }

    public static ThreadTimelineItem FromArtifact(WorkspaceArtifact artifact)
    {
        return new ThreadTimelineItem
        {
            Kind = TimelineItemKind.Artifact,
            Artifact = artifact,
            CreatedAt = artifact.UpdatedAt,
            Title = "Artifact",
            Subtitle = artifact.Path,
            Body = TrimPreview(string.IsNullOrWhiteSpace(artifact.Preview) ? artifact.Content : artifact.Preview, 1200),
            StatusLabel = artifact.MediaType,
            Glyph = "\uE8A5"
        };
    }

    private static string TrimPreview(string value, int maxLength)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            return string.Empty;
        }

        value = value.Trim();
        return value.Length <= maxLength ? value : value[..maxLength] + "...";
    }
}
