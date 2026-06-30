using System;
using System.Collections.ObjectModel;
using System.Linq;
using System.Text.Json.Serialization;
using CommunityToolkit.Mvvm.ComponentModel;
using Microsoft.UI.Xaml.Media.Imaging;
using ReadOS.Msp.Models;

namespace ReadOS.App.Models;

public enum LibraryItemKind
{
    Folder,
    Pdf,
    Markdown,
    Note
}

public enum ChatRole
{
    User,
    Assistant,
    System
}

public enum AttachmentKind
{
    Page,
    PageRange,
    Region,
    File
}

public sealed partial class WorkspaceState : ObservableObject
{
    [ObservableProperty]
    public partial WorkspaceSettings Settings { get; set; } = new();

    public ObservableCollection<ProjectItem> Projects { get; } = new();

    public ObservableCollection<WorkspaceArtifact> Artifacts { get; } = new();

    public ObservableCollection<MspTranscriptEntry> MspTranscript { get; } = new();

    public ObservableCollection<MspSessionEntry> MspSessions { get; } = new();
}

public sealed partial class WorkspaceSettings : ObservableObject
{
    [ObservableProperty]
    public partial string LanguageCode { get; set; } = "zh-CN";

    [ObservableProperty]
    public partial string ProviderName { get; set; } = "OpenAI Compatible";

    [ObservableProperty]
    public partial string ProviderBaseUrl { get; set; } = "https://api.openai.com/v1";

    [ObservableProperty]
    public partial string ProviderApiKey { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string ModelName { get; set; } = "gpt-4.1-mini";

    [ObservableProperty]
    public partial bool UseOfflineResponses { get; set; } = true;

    [ObservableProperty]
    public partial bool UseDarkTheme { get; set; }

    [ObservableProperty]
    public partial string AttachmentDefaultPrompt { get; set; } = "请按书本顺序解释我附加的页面。";

    [ObservableProperty]
    public partial string RegionExplainPrompt { get; set; } = "请结合上下文，重点讲解红框内容。";

    [ObservableProperty]
    public partial string ChapterExplainPrompt { get; set; } = "请围绕我附加的这一整节内容进行系统讲解。";

    [ObservableProperty]
    public partial string MinorUEndpoint { get; set; } = "https://mineru.net/api";
}

public sealed partial class ProjectItem : ObservableObject
{
    [ObservableProperty]
    public partial string Id { get; set; } = Guid.NewGuid().ToString("N");

    [ObservableProperty]
    public partial string Name { get; set; } = "未命名项目";

    [ObservableProperty]
    public partial string Description { get; set; } = "本地阅读项目";

    [ObservableProperty]
    public partial DateTimeOffset UpdatedAt { get; set; } = DateTimeOffset.Now;

    public ObservableCollection<LibraryItem> LibraryItems { get; } = new();

    public ObservableCollection<ChatConversation> StandaloneConversations { get; } = new();

    [JsonIgnore]
    public string UpdatedLabel => UpdatedAt.ToLocalTime().ToString("MM-dd HH:mm");
}

public sealed partial class LibraryItem : ObservableObject
{
    [ObservableProperty]
    public partial string Id { get; set; } = Guid.NewGuid().ToString("N");

    [ObservableProperty]
    public partial string ProjectId { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string? ParentId { get; set; }

    [ObservableProperty]
    public partial LibraryItemKind Kind { get; set; }

    [ObservableProperty]
    public partial string Name { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string RelativePath { get; set; } = string.Empty;

    [ObservableProperty]
    public partial long SizeBytes { get; set; }

    [ObservableProperty]
    public partial int Order { get; set; }

    [ObservableProperty]
    public partial int PageCount { get; set; }

    [ObservableProperty]
    public partial int CurrentPage { get; set; } = 1;

    [ObservableProperty]
    public partial DateTimeOffset ImportedAt { get; set; } = DateTimeOffset.Now;

    [ObservableProperty]
    public partial DateTimeOffset UpdatedAt { get; set; } = DateTimeOffset.Now;

    public ObservableCollection<PageLabelRule> PageLabels { get; } = new();

    public ObservableCollection<OutlineItem> Outline { get; } = new();

    public ObservableCollection<ChatConversation> Conversations { get; } = new();

    [JsonIgnore]
    public string KindLabel => Kind switch
    {
        LibraryItemKind.Pdf => "PDF",
        LibraryItemKind.Markdown => "MD",
        LibraryItemKind.Note => "NOTE",
        _ => "DIR"
    };

    [JsonIgnore]
    public string Detail => Kind == LibraryItemKind.Pdf
        ? $"{PageCount} 页 · {FormatSize(SizeBytes)}"
        : Kind == LibraryItemKind.Folder
            ? "文件夹"
            : FormatSize(SizeBytes);

    [JsonIgnore]
    public string SearchText => $"{Name} {KindLabel}";

    [JsonIgnore]
    public string CurrentPageLabel => PageLabels.Count == 0
        ? CurrentPage.ToString()
        : PageLabels.FirstOrDefault(rule => rule.PdfPage == CurrentPage)?.Label ?? CurrentPage.ToString();

    private static string FormatSize(long bytes)
    {
        if (bytes <= 0)
        {
            return "0 KB";
        }

        var units = new[] { "B", "KB", "MB", "GB" };
        var value = (double)bytes;
        var unitIndex = 0;
        while (value >= 1024 && unitIndex < units.Length - 1)
        {
            value /= 1024;
            unitIndex++;
        }

        return unitIndex == 0 ? $"{value:0} {units[unitIndex]}" : $"{value:0.0} {units[unitIndex]}";
    }
}

public sealed partial class PageLabelRule : ObservableObject
{
    [ObservableProperty]
    public partial int PdfPage { get; set; }

    [ObservableProperty]
    public partial string Label { get; set; } = string.Empty;
}

public sealed partial class OutlineItem : ObservableObject
{
    [ObservableProperty]
    public partial string Id { get; set; } = Guid.NewGuid().ToString("N");

    [ObservableProperty]
    public partial string Title { get; set; } = string.Empty;

    [ObservableProperty]
    public partial int Page { get; set; }

    [ObservableProperty]
    public partial int Level { get; set; }

    [JsonIgnore]
    public string IndentedTitle => $"{new string(' ', Math.Max(0, Level - 1) * 2)}{Title}";
}

public sealed partial class ChatConversation : ObservableObject
{
    [ObservableProperty]
    public partial string Id { get; set; } = Guid.NewGuid().ToString("N");

    [ObservableProperty]
    public partial string DocumentId { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string Title { get; set; } = "新对话";

    [ObservableProperty]
    public partial DateTimeOffset UpdatedAt { get; set; } = DateTimeOffset.Now;

    public ObservableCollection<ChatMessage> Messages { get; } = new();
}

public sealed partial class ChatMessage : ObservableObject
{
    [ObservableProperty]
    public partial string Id { get; set; } = Guid.NewGuid().ToString("N");

    [ObservableProperty]
    public partial ChatRole Role { get; set; }

    [ObservableProperty]
    public partial string Author { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string Content { get; set; } = string.Empty;

    [ObservableProperty]
    public partial DateTimeOffset CreatedAt { get; set; } = DateTimeOffset.Now;

    public ObservableCollection<ChatAttachment> Attachments { get; } = new();

    [JsonIgnore]
    public string Timestamp => CreatedAt.ToLocalTime().ToString("HH:mm");

    [JsonIgnore]
    public bool IsAssistant => Role == ChatRole.Assistant;
}

public sealed partial class ChatAttachment : ObservableObject
{
    [ObservableProperty]
    public partial string Id { get; set; } = Guid.NewGuid().ToString("N");

    [ObservableProperty]
    public partial AttachmentKind Kind { get; set; }

    [ObservableProperty]
    public partial string DocumentId { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string Title { get; set; } = string.Empty;

    [ObservableProperty]
    public partial int StartPage { get; set; }

    [ObservableProperty]
    public partial int EndPage { get; set; }

    [ObservableProperty]
    public partial string? FilePath { get; set; }

    [ObservableProperty]
    public partial double RegionX { get; set; }

    [ObservableProperty]
    public partial double RegionY { get; set; }

    [ObservableProperty]
    public partial double RegionWidth { get; set; }

    [ObservableProperty]
    public partial double RegionHeight { get; set; }

    [JsonIgnore]
    public string Detail => Kind switch
    {
        AttachmentKind.Page => $"第 {StartPage} 页",
        AttachmentKind.PageRange => $"第 {StartPage}-{EndPage} 页",
        AttachmentKind.Region => $"第 {StartPage} 页框选区域",
        _ => FilePath ?? string.Empty
    };
}

public sealed partial class WorkspaceArtifact : ObservableObject
{
    [ObservableProperty]
    public partial string Id { get; set; } = Guid.NewGuid().ToString("N");

    [ObservableProperty]
    public partial string Path { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string Content { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string MediaType { get; set; } = "text/plain";

    [ObservableProperty]
    public partial string Description { get; set; } = "MSP artifact";

    [ObservableProperty]
    public partial string SourceCommand { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string Actor { get; set; } = "agent";

    [ObservableProperty]
    public partial string SessionId { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string Preview { get; set; } = string.Empty;

    public ObservableCollection<string> SourcePaths { get; } = new();

    public ObservableCollection<string> SourceDocuments { get; } = new();

    public ObservableCollection<string> SourcePages { get; } = new();

    [ObservableProperty]
    public partial DateTimeOffset CreatedAt { get; set; } = DateTimeOffset.Now;

    [ObservableProperty]
    public partial DateTimeOffset UpdatedAt { get; set; } = DateTimeOffset.Now;

    [JsonIgnore]
    public long SizeBytes => Content.Length;
}

public sealed partial class MspTranscriptEntry : ObservableObject
{
    [ObservableProperty]
    public partial string Id { get; set; } = Guid.NewGuid().ToString("N");

    [ObservableProperty]
    public partial string Actor { get; set; } = "agent";

    [ObservableProperty]
    public partial string SessionId { get; set; } = "default";

    [ObservableProperty]
    public partial string CommandText { get; set; } = string.Empty;

    [ObservableProperty]
    public partial DateTimeOffset StartedAt { get; set; } = DateTimeOffset.Now;

    [ObservableProperty]
    public partial DateTimeOffset CompletedAt { get; set; } = DateTimeOffset.Now;

    [ObservableProperty]
    public partial int ExitCode { get; set; }

    [ObservableProperty]
    public partial string Stdout { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string Stderr { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string Decision { get; set; } = "Allow";

    [ObservableProperty]
    public partial string Effects { get; set; } = "None";

    [ObservableProperty]
    public partial string ArtifactsSummary { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string PolicyPreview { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string ProgressMessage { get; set; } = string.Empty;

    [ObservableProperty]
    public partial int? ProgressPercent { get; set; }

    [ObservableProperty]
    public partial bool IsRunning { get; set; }

    [ObservableProperty]
    public partial bool WasCanceled { get; set; }

    public static MspTranscriptEntry FromRecord(MspCommandTranscriptRecord record)
    {
        return new MspTranscriptEntry
        {
            Id = record.Id,
            Actor = record.Actor,
            SessionId = record.SessionId,
            CommandText = record.CommandText,
            StartedAt = record.StartedAt,
            CompletedAt = record.CompletedAt,
            ExitCode = record.ExitCode,
            Stdout = record.Stdout,
            Stderr = record.Stderr,
            Decision = record.Decision,
            Effects = record.Effects,
            ArtifactsSummary = record.ArtifactsSummary,
            PolicyPreview = record.PolicyPreview,
            ProgressMessage = record.ProgressMessage,
            ProgressPercent = record.ProgressPercent,
            WasCanceled = record.WasCanceled
        };
    }

    public MspCommandTranscriptRecord ToRecord()
    {
        return new MspCommandTranscriptRecord
        {
            Id = Id,
            Actor = Actor,
            SessionId = SessionId,
            CommandText = CommandText,
            StartedAt = StartedAt,
            CompletedAt = CompletedAt,
            ExitCode = ExitCode,
            Stdout = Stdout,
            Stderr = Stderr,
            Decision = Decision,
            Effects = Effects,
            ArtifactsSummary = ArtifactsSummary,
            PolicyPreview = PolicyPreview,
            ProgressMessage = ProgressMessage,
            ProgressPercent = ProgressPercent,
            WasCanceled = WasCanceled
        };
    }

    [JsonIgnore]
    public bool Succeeded => ExitCode == 0;

    [JsonIgnore]
    public bool IsApprovalRequired => Decision == "RequireConfirmation";

    [JsonIgnore]
    public bool CanCancel => IsRunning;

    [JsonIgnore]
    public bool IsProgressIndeterminate => IsRunning && ProgressPercent is null;

    [JsonIgnore]
    public double ProgressPercentValue => Math.Clamp(ProgressPercent ?? 0, 0, 100);

    [JsonIgnore]
    public string StatusLabel => IsRunning
        ? ProgressPercent is null ? "运行中" : $"运行 {ProgressPercent}%"
        : IsApprovalRequired ? "待确认" : WasCanceled ? "已取消" : Succeeded ? "完成" : $"失败 {ExitCode}";

    [JsonIgnore]
    public string CompletedLabel => CompletedAt.ToLocalTime().ToString("HH:mm:ss");

    [JsonIgnore]
    public string OutputPreview
    {
        get
        {
            var output = IsRunning && !string.IsNullOrWhiteSpace(ProgressMessage)
                ? ProgressMessage
                : IsApprovalRequired && !string.IsNullOrWhiteSpace(PolicyPreview)
                ? PolicyPreview
                : WasCanceled && !string.IsNullOrWhiteSpace(ProgressMessage)
                    ? ProgressMessage
                : string.IsNullOrWhiteSpace(Stdout) ? Stderr : Stdout;
            if (string.IsNullOrWhiteSpace(output))
            {
                return "(no output)";
            }

            output = output.Trim();
            return output.Length <= 900 ? output : output[..900] + "...";
        }
    }

    partial void OnCompletedAtChanged(DateTimeOffset value)
    {
        OnPropertyChanged(nameof(CompletedLabel));
    }

    partial void OnExitCodeChanged(int value)
    {
        NotifyStatusChanged();
    }

    partial void OnStdoutChanged(string value)
    {
        OnPropertyChanged(nameof(OutputPreview));
    }

    partial void OnStderrChanged(string value)
    {
        OnPropertyChanged(nameof(OutputPreview));
    }

    partial void OnDecisionChanged(string value)
    {
        NotifyStatusChanged();
    }

    partial void OnPolicyPreviewChanged(string value)
    {
        OnPropertyChanged(nameof(OutputPreview));
    }

    partial void OnProgressMessageChanged(string value)
    {
        OnPropertyChanged(nameof(OutputPreview));
    }

    partial void OnProgressPercentChanged(int? value)
    {
        OnPropertyChanged(nameof(ProgressPercentValue));
        OnPropertyChanged(nameof(IsProgressIndeterminate));
        OnPropertyChanged(nameof(StatusLabel));
    }

    partial void OnIsRunningChanged(bool value)
    {
        OnPropertyChanged(nameof(CanCancel));
        NotifyStatusChanged();
        OnPropertyChanged(nameof(IsProgressIndeterminate));
        OnPropertyChanged(nameof(OutputPreview));
    }

    partial void OnWasCanceledChanged(bool value)
    {
        NotifyStatusChanged();
        OnPropertyChanged(nameof(OutputPreview));
    }

    private void NotifyStatusChanged()
    {
        OnPropertyChanged(nameof(Succeeded));
        OnPropertyChanged(nameof(IsApprovalRequired));
        OnPropertyChanged(nameof(StatusLabel));
    }
}

public sealed partial class MspSessionEntry : ObservableObject
{
    [ObservableProperty]
    public partial string Id { get; set; } = "default";

    [ObservableProperty]
    public partial string Title { get; set; } = "MSP session";

    [ObservableProperty]
    public partial string Actor { get; set; } = "agent";

    [ObservableProperty]
    public partial DateTimeOffset StartedAt { get; set; } = DateTimeOffset.Now;

    [ObservableProperty]
    public partial DateTimeOffset UpdatedAt { get; set; } = DateTimeOffset.Now;

    [ObservableProperty]
    public partial string LastCommandText { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string LastDecision { get; set; } = "Allow";

    [ObservableProperty]
    public partial int LastExitCode { get; set; }

    [ObservableProperty]
    public partial string LastProgressMessage { get; set; } = string.Empty;

    [ObservableProperty]
    public partial int CommandCount { get; set; }

    [ObservableProperty]
    public partial int ApprovalCount { get; set; }

    public ObservableCollection<string> TranscriptIds { get; } = new();

    public ObservableCollection<string> ArtifactPaths { get; } = new();

    public static MspSessionEntry FromRecord(MspSessionRecord record)
    {
        var entry = new MspSessionEntry
        {
            Id = record.Id,
            Title = record.Title,
            Actor = record.Actor,
            StartedAt = record.StartedAt,
            UpdatedAt = record.UpdatedAt,
            LastCommandText = record.LastCommandText,
            LastDecision = record.LastDecision,
            LastExitCode = record.LastExitCode,
            LastProgressMessage = record.LastProgressMessage,
            CommandCount = record.CommandCount,
            ApprovalCount = record.ApprovalCount
        };

        foreach (var transcriptId in record.TranscriptIds)
        {
            entry.TranscriptIds.Add(transcriptId);
        }

        foreach (var artifactPath in record.ArtifactPaths)
        {
            entry.ArtifactPaths.Add(artifactPath);
        }

        return entry;
    }

    public MspSessionRecord ToRecord()
    {
        return new MspSessionRecord
        {
            Id = Id,
            Title = Title,
            Actor = Actor,
            StartedAt = StartedAt,
            UpdatedAt = UpdatedAt,
            LastCommandText = LastCommandText,
            LastDecision = LastDecision,
            LastExitCode = LastExitCode,
            LastProgressMessage = LastProgressMessage,
            CommandCount = CommandCount,
            ApprovalCount = ApprovalCount,
            TranscriptIds = TranscriptIds.ToArray(),
            ArtifactPaths = ArtifactPaths.ToArray()
        };
    }

    [JsonIgnore]
    public string Summary => $"{CommandCount} commands · {ArtifactPaths.Count} artifacts";
}

public sealed partial class PageImageItem : ObservableObject
{
    [ObservableProperty]
    public partial int PageNumber { get; set; }

    [ObservableProperty]
    public partial string Label { get; set; } = string.Empty;

    [ObservableProperty]
    public partial BitmapImage? Image { get; set; }
}

public sealed partial class NavigationEntry : ObservableObject
{
    [ObservableProperty]
    public partial string Id { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string ProjectId { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string? DocumentId { get; set; }

    [ObservableProperty]
    public partial bool IsProject { get; set; }

    [ObservableProperty]
    public partial string Title { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string Detail { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string Prefix { get; set; } = string.Empty;
}
