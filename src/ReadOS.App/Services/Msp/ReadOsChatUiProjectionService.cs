using System.Collections.Immutable;
using ReadOS.App.Models;
using ReadOS.App.Models.ChatUi;

namespace ReadOS.App.Services.Msp;

// Projects existing ReadOS workbench records into the canonical MSP Chat UI
// timeline model (msp.chat-ui.timeline.v1).
//
// This is the ReadOS-side adapter over the upstream-neutral Projection layer.
// It owns the product-specific mapping:
//
//   ChatMessage          -> ChatUiMessage (user/assistant markdown block,
//                           reasoning/thinking block, image block,
//                           text-selection block for region attachments)
//   MspTranscriptEntry   -> ChatUiMessage (tool role) with toolCall/toolGroup,
//                           progress, processing, search results/progress,
//                           video progress, and notice blocks
//   WorkspaceArtifact     -> ChatUiMessage (tool role) with attachment/image/
//                           proposed-plan + markdown + sources + footer blocks
//   ChatAttachment        -> attachment / image / text-selection block entries
//
// The projection is read-only and deterministic: the same input records produce
// the same timeline, so the render planner can diff successive projections.
// Presentation defaults follow the Default renderer manifest (light theme).
//
// This service does not render anything. It only builds the canonical model that
// the XAML ChatTimelineView consumes through render operations.

public sealed class ReadOsChatUiProjectionService
{
    public ChatUiTimeline BuildTimeline(
        IEnumerable<ChatMessage> messages,
        IEnumerable<MspTranscriptEntry> mspEntries,
        IEnumerable<WorkspaceArtifact> artifacts)
    {
        var projected = new List<TimelineEntry>();

        foreach (var message in messages)
        {
            projected.Add(new TimelineEntry(message.CreatedAt, ProjectMessage(message)));
        }

        // Group MSP transcript entries by session so a batch of related tool
        // calls collapses into a single tool-group message. Lone entries stay
        // standalone to preserve the 1:1 transcript→tool-call mapping.
        foreach (var group in mspEntries.GroupBy(e => e.SessionId))
        {
            var entries = group.ToArray();
            if (entries.Length == 1)
            {
                projected.Add(new TimelineEntry(entries[0].StartedAt, ProjectMspEntry(entries[0])));
            }
            else
            {
                projected.Add(new TimelineEntry(entries[0].StartedAt, ProjectMspGroup(group.Key, entries)));
            }
        }

        foreach (var artifact in artifacts)
        {
            projected.Add(new TimelineEntry(artifact.UpdatedAt, ProjectArtifact(artifact)));
        }

        projected.Sort((a, b) => a.At.CompareTo(b.At));

        return new ChatUiTimeline
        {
            SchemaVersion = ChatUiTimeline.Schema,
            Id = "reados",
            Title = "ReadOS conversation",
            Revision = 1,
            Presentation = DefaultPresentation(),
            Messages = projected.Select(p => p.Message).ToImmutableArray()
        };
    }

    private static ChatUiPresentation DefaultPresentation() => new()
    {
        Theme = "dark",
        MarkdownProfile = "markstream-readex-fade",
        CodeTheme = "vitesse",
        MessageActions = new ChatUiMessageActions
        {
            Enabled = true,
            AssistantPlacement = "footer",
            Assistant = new[] { "copy", "regenerate", "inspectRenderPatch" },
            User = new[] { "copy", "edit", "branch" }
        }
    };

    private static ChatUiMessage ProjectMessage(ChatMessage message)
    {
        var isAssistant = message.Role == ChatRole.Assistant;
        var role = message.Role switch
        {
            ChatRole.User => ChatUiRole.User,
            ChatRole.Assistant => ChatUiRole.Assistant,
            _ => ChatUiRole.System
        };

        var blocks = new List<ChatUiBlock>();

        // Assistant messages may embed a <thinking>…</thinking> section in their
        // content (a realistic LLM transcript shape). Lift it into a dedicated
        // reasoning block and let the remaining text become the markdown block.
        var content = message.Content ?? string.Empty;
        var markdownText = content;
        if (isAssistant && TryExtractReasoning(content, out var reasoning, out var rest))
        {
            blocks.Add(new ChatUiReasoningBlock
            {
                Id = $"{message.Id}:reason",
                Text = reasoning
            });
            markdownText = rest;
        }

        // Assistant messages with empty content still get a streaming markdown
        // block so the Default renderer shows a shimmer/caret slot. Non-empty
        // content gets a normal markdown block. User/system messages with empty
        // content simply produce no markdown block.
        var isAssistantStreaming = isAssistant && string.IsNullOrEmpty(markdownText?.Trim());
        if (!string.IsNullOrWhiteSpace(markdownText) || isAssistantStreaming)
        {
            blocks.Add(new ChatUiMarkdownBlock
            {
                Id = $"{message.Id}:text",
                Text = markdownText ?? string.Empty,
                Streaming = isAssistantStreaming ? true : null
            });
        }

        foreach (var attachment in message.Attachments)
        {
            if (IsImageAttachment(attachment))
            {
                blocks.Add(new ChatUiImageBlock
                {
                    Id = $"{message.Id}:img:{attachment.Id}",
                    Images = new object[] { new { id = attachment.Id, filePath = attachment.FilePath, title = attachment.Title } }
                });
            }
            else
            {
                blocks.Add(new ChatUiAttachmentBlock
                {
                    Id = $"{message.Id}:att:{attachment.Id}",
                    Attachments = new object[] { new { id = attachment.Id, title = attachment.Title, detail = attachment.Detail } }
                });
            }

            // A region attachment is exactly a text selection on a document page.
            if (attachment.Kind == AttachmentKind.Region)
            {
                blocks.Add(new ChatUiTextSelectionBlock
                {
                    Id = $"{message.Id}:sel:{attachment.Id}",
                    TextSelection = new Dictionary<string, object?>
                    {
                        ["documentId"] = attachment.DocumentId,
                        ["page"] = attachment.StartPage,
                        ["regionX"] = attachment.RegionX,
                        ["regionY"] = attachment.RegionY,
                        ["regionWidth"] = attachment.RegionWidth,
                        ["regionHeight"] = attachment.RegionHeight
                    }
                });
            }
        }

        return new ChatUiMessage
        {
            Id = message.Id,
            Role = role,
            Status = ChatUiStatus.Success,
            ModelName = isAssistant ? message.Author : null,
            CreatedAt = message.CreatedAt.ToLocalTime().ToString("HH:mm"),
            TimeText = message.CreatedAt.ToLocalTime().ToString("HH:mm"),
            Blocks = blocks
        };
    }

    private static ChatUiMessage ProjectMspEntry(MspTranscriptEntry entry)
    {
        var blocks = new List<ChatUiBlock> { BuildToolCallBlock(entry) };
        blocks.AddRange(BuildMspExtraBlocks(entry));

        return new ChatUiMessage
        {
            Id = entry.Id,
            Role = ChatUiRole.Tool,
            Status = ProjectStatus(entry),
            CreatedAt = entry.StartedAt.ToLocalTime().ToString("HH:mm"),
            TimeText = entry.StartedAt.ToLocalTime().ToString("HH:mm"),
            Blocks = blocks
        };
    }

    private static ChatUiMessage ProjectMspGroup(string sessionId, IReadOnlyList<MspTranscriptEntry> entries)
    {
        var blocks = new List<ChatUiBlock>();

        blocks.Add(new ChatUiToolGroupBlock
        {
            Id = $"{sessionId}:group",
            Title = $"{entries.Count} tool calls",
            ToolCalls = entries.Select(BuildToolCallBlock).ToArray()
        });

        foreach (var entry in entries)
        {
            blocks.AddRange(BuildMspExtraBlocks(entry));
        }

        var status = ChatUiStatus.Success;
        if (entries.Any(e => e.IsRunning))
        {
            status = ChatUiStatus.Running;
        }
        else if (entries.Any(e => e.ExitCode != 0 && !e.WasCanceled))
        {
            status = ChatUiStatus.Failed;
        }

        return new ChatUiMessage
        {
            Id = sessionId,
            Role = ChatUiRole.Tool,
            Status = status,
            CreatedAt = entries[0].StartedAt.ToLocalTime().ToString("HH:mm"),
            TimeText = entries[0].StartedAt.ToLocalTime().ToString("HH:mm"),
            Blocks = blocks
        };
    }

    // Builds the tool-call block for a single transcript entry.
    private static ChatUiToolCallBlock BuildToolCallBlock(MspTranscriptEntry entry)
    {
        return new ChatUiToolCallBlock
        {
            Id = $"{entry.Id}:call",
            ToolName = ExtractToolName(entry.CommandText),
            Title = entry.IsApprovalRequired ? "MSP approval required" : entry.CommandText,
            DetailText = Trim(entry.Actor, 96),
            OutputText = string.IsNullOrWhiteSpace(entry.OutputPreview) ? null : entry.OutputPreview,
            ErrorText = entry.ExitCode != 0 && !entry.WasCanceled && !string.IsNullOrWhiteSpace(entry.DiagnosticsSummary)
                ? entry.DiagnosticsSummary : null,
            DurationMs = entry.CompletedAt != default ? (entry.CompletedAt - entry.StartedAt).TotalMilliseconds : null,
            Status = ProjectStatus(entry)
        };
    }

    // Builds the auxiliary blocks that hang off a transcript entry: progress,
    // processing activity, search results/progress, video progress, and notices.
    private static List<ChatUiBlock> BuildMspExtraBlocks(MspTranscriptEntry entry)
    {
        var blocks = new List<ChatUiBlock>();
        var isSearch = IsSearchCommand(entry.CommandText);
        var isVideo = IsVideoCommand(entry.CommandText);

        if (entry.IsRunning)
        {
            blocks.Add(new ChatUiProgressBlock
            {
                Id = $"{entry.Id}:prog",
                Title = entry.ProgressMessage,
                DetailText = entry.ProgressPercent.HasValue ? $"{entry.ProgressPercent}%" : null,
                Progress = entry.ProgressPercent,
                Status = ChatUiStatus.Running
            });

            blocks.Add(new ChatUiProcessingBlock
            {
                Id = $"{entry.Id}:proc",
                Title = "Processing",
                Active = true,
                Status = ChatUiStatus.Running,
                Items = new[]
                {
                    new ChatUiActivityItem
                    {
                        Id = entry.Id,
                        Type = ChatUiActivityItemType.Tool,
                        ToolName = ExtractToolName(entry.CommandText),
                        Status = ChatUiStatus.Running,
                        Text = entry.ProgressMessage
                    }
                }
            });

            if (isSearch)
            {
                blocks.Add(new ChatUiSearchProgressBlock
                {
                    Id = $"{entry.Id}:searchprog",
                    Title = "Searching",
                    SearchQueries = ParseSearchQueries(entry.CommandText)
                });
            }
        }

        if (isVideo)
        {
            blocks.Add(new ChatUiVideoProgressBlock
            {
                Id = $"{entry.Id}:video",
                Title = entry.ProgressMessage ?? "Video processing",
                DetailText = entry.ProgressPercent.HasValue ? $"{entry.ProgressPercent}%" : null,
                Progress = entry.ProgressPercent
            });
        }

        if (isSearch && entry.ExitCode == 0 && !entry.IsRunning)
        {
            blocks.Add(new ChatUiSearchResultsBlock
            {
                Id = $"{entry.Id}:search",
                SearchQueries = ParseSearchQueries(entry.CommandText)
            });
        }

        // Notice block for policy previews / approvals.
        if (entry.IsApprovalRequired && !string.IsNullOrWhiteSpace(entry.PolicyPreview))
        {
            blocks.Add(new ChatUiNoticeBlock
            {
                Id = $"{entry.Id}:notice",
                Text = entry.PolicyPreview,
                Status = ChatUiStatus.Pending
            });
        }

        // Recovery hint for failures.
        if (entry.ExitCode != 0 && !entry.WasCanceled && !string.IsNullOrWhiteSpace(entry.RecoveryHint))
        {
            blocks.Add(new ChatUiNoticeBlock
            {
                Id = $"{entry.Id}:recovery",
                Text = "Recovery: " + entry.RecoveryHint,
                Status = ChatUiStatus.Failed
            });
        }

        return blocks;
    }

    private static ChatUiMessage ProjectArtifact(WorkspaceArtifact artifact)
    {
        var blocks = new List<ChatUiBlock>();

        if (IsImageMediaType(artifact.MediaType))
        {
            blocks.Add(new ChatUiImageBlock
            {
                Id = $"{artifact.Id}:img",
                Images = new object[] { new { id = artifact.Id, path = artifact.Path, mediaType = artifact.MediaType } }
            });
        }
        else if (IsProposedPlan(artifact))
        {
            blocks.Add(new ChatUiProposedPlanBlock
            {
                Id = $"{artifact.Id}:plan",
                Text = string.IsNullOrWhiteSpace(artifact.Preview) ? artifact.Content : artifact.Preview,
                PhaseTitle = artifact.Description
            });
        }
        else
        {
            blocks.Add(new ChatUiAttachmentBlock
            {
                Id = $"{artifact.Id}:att",
                Attachments = new object[]
                {
                    new { id = artifact.Id, path = artifact.Path, mediaType = artifact.MediaType, actor = artifact.Actor }
                }
            });
        }

        if (!string.IsNullOrWhiteSpace(artifact.Preview) || !string.IsNullOrWhiteSpace(artifact.Content))
        {
            blocks.Add(new ChatUiMarkdownBlock
            {
                Id = $"{artifact.Id}:preview",
                Text = string.IsNullOrWhiteSpace(artifact.Preview) ? artifact.Content : artifact.Preview
            });
        }

        if (artifact.SourcePaths.Count > 0)
        {
            blocks.Add(new ChatUiSourcesBlock
            {
                Id = $"{artifact.Id}:sources",
                Sources = artifact.SourcePaths.Select(p => (object)new { path = p }).ToImmutableArray()
            });
        }

        blocks.Add(new ChatUiFooterBlock
        {
            Id = $"{artifact.Id}:footer",
            Text = $"{artifact.MediaType} · {artifact.Actor}"
        });

        return new ChatUiMessage
        {
            Id = artifact.Id,
            Role = ChatUiRole.Tool,
            Status = ChatUiStatus.Success,
            CreatedAt = artifact.UpdatedAt.ToLocalTime().ToString("HH:mm"),
            TimeText = artifact.UpdatedAt.ToLocalTime().ToString("HH:mm"),
            Blocks = blocks
        };
    }

    private static ChatUiStatus ProjectStatus(MspTranscriptEntry entry)
    {
        if (entry.IsRunning) return ChatUiStatus.Running;
        if (entry.IsApprovalRequired) return ChatUiStatus.Pending;
        if (entry.WasCanceled) return ChatUiStatus.Cancelled;
        return entry.ExitCode == 0 ? ChatUiStatus.Success : ChatUiStatus.Failed;
    }

    // Extracts a <thinking>…</thinking> section from assistant content.
    private static bool TryExtractReasoning(string content, out string reasoning, out string rest)
    {
        reasoning = string.Empty;
        rest = content;
        const string open = "<thinking>";
        const string close = "</thinking>";
        var start = content.IndexOf(open, StringComparison.OrdinalIgnoreCase);
        if (start < 0)
        {
            return false;
        }

        var end = content.IndexOf(close, start + open.Length, StringComparison.OrdinalIgnoreCase);
        if (end < 0)
        {
            return false;
        }

        reasoning = content.Substring(start + open.Length, end - (start + open.Length)).Trim();
        rest = (content[..start] + content[(end + close.Length)..]).Trim();
        return true;
    }

    private static bool IsImageAttachment(ChatAttachment attachment)
    {
        var path = attachment.FilePath;
        if (string.IsNullOrWhiteSpace(path))
        {
            return false;
        }

        var lower = path.ToLowerInvariant();
        return lower.EndsWith(".png") || lower.EndsWith(".jpg") || lower.EndsWith(".jpeg")
            || lower.EndsWith(".gif") || lower.EndsWith(".webp") || lower.EndsWith(".bmp");
    }

    private static bool IsImageMediaType(string mediaType) =>
        !string.IsNullOrWhiteSpace(mediaType) && mediaType.StartsWith("image/", StringComparison.OrdinalIgnoreCase);

    private static bool IsProposedPlan(WorkspaceArtifact artifact)
    {
        if (string.Equals(artifact.MediaType, "application/x-msp-plan", StringComparison.OrdinalIgnoreCase))
        {
            return true;
        }

        if (artifact.Description.Contains("plan", StringComparison.OrdinalIgnoreCase))
        {
            return true;
        }

        var text = string.IsNullOrWhiteSpace(artifact.Preview) ? artifact.Content : artifact.Preview;
        if (string.IsNullOrWhiteSpace(text))
        {
            return false;
        }

        var trimmed = text.TrimStart();
        return trimmed.StartsWith("# Plan", StringComparison.OrdinalIgnoreCase)
            || trimmed.StartsWith("## Plan", StringComparison.OrdinalIgnoreCase)
            || trimmed.StartsWith("# 计划", StringComparison.OrdinalIgnoreCase)
            || trimmed.StartsWith("## 计划", StringComparison.OrdinalIgnoreCase)
            || trimmed.StartsWith("Proposed plan", StringComparison.OrdinalIgnoreCase);
    }

    private static bool IsSearchCommand(string command) =>
        !string.IsNullOrWhiteSpace(command) && command.Contains("search", StringComparison.OrdinalIgnoreCase);

    private static bool IsVideoCommand(string command) =>
        !string.IsNullOrWhiteSpace(command) && command.Contains("video", StringComparison.OrdinalIgnoreCase);

    private static IReadOnlyList<string> ParseSearchQueries(string command)
    {
        if (string.IsNullOrWhiteSpace(command))
        {
            return Array.Empty<string>();
        }

        var idx = command.IndexOf("search", StringComparison.OrdinalIgnoreCase);
        var after = idx < 0 ? command : command[(idx + "search".Length)..];
        after = after.Trim();

        // Drop a leading qualifier token such as "current" or "web".
        if (after.StartsWith("current", StringComparison.OrdinalIgnoreCase))
        {
            after = after["current".Length..].Trim();
        }

        return string.IsNullOrWhiteSpace(after) ? new[] { command.Trim() } : new[] { after };
    }

    private static string ExtractToolName(string commandText)
    {
        if (string.IsNullOrWhiteSpace(commandText)) return "msp";
        var trimmed = commandText.Trim();
        var space = trimmed.IndexOf(' ');
        return space < 0 ? trimmed : trimmed[..space];
    }

    private static string Trim(string value, int max)
    {
        if (string.IsNullOrEmpty(value)) return string.Empty;
        return value.Length <= max ? value : value[..max] + "...";
    }

    private sealed record TimelineEntry(DateTimeOffset At, ChatUiMessage Message);
}
