using System.Collections.ObjectModel;

namespace ReadOS.App.Models;

public enum MaterialKind
{
    Pdf,
    Markdown,
    Note,
    Folder
}

public enum ActivityKind
{
    Text,
    Code,
    Materials,
    Changes,
    Answer
}

public enum NavigationEntryKind
{
    Project,
    Session
}

public sealed class NavigationEntry
{
    public required string Id { get; init; }

    public required string ProjectId { get; init; }

    public string? SessionId { get; init; }

    public required NavigationEntryKind Kind { get; init; }

    public required string Title { get; init; }

    public required string Detail { get; init; }

    public string Prefix => Kind == NavigationEntryKind.Project ? "▸" : "  ";

    public string TimeLabel => Detail;
}

public sealed class ProjectItem
{
    public required string Id { get; init; }

    public required string Name { get; set; }

    public required string Description { get; set; }

    public required string UpdatedLabel { get; set; }

    public ObservableCollection<SessionItem> Sessions { get; } = new();

    public ObservableCollection<MaterialItem> Materials { get; } = new();
}

public sealed class SessionItem
{
    public required string Id { get; init; }

    public required string ProjectId { get; init; }

    public required string Title { get; set; }

    public required string UpdatedLabel { get; set; }

    public ObservableCollection<ActivityItem> Activities { get; } = new();
}

public sealed class MaterialItem
{
    public required string Id { get; init; }

    public required string Name { get; set; }

    public required MaterialKind Kind { get; init; }

    public required string Detail { get; set; }

    public string KindLabel => Kind switch
    {
        MaterialKind.Pdf => "PDF",
        MaterialKind.Markdown => "MD",
        MaterialKind.Note => "NOTE",
        _ => "DIR"
    };
}

public sealed class ActivityItem
{
    public required string Id { get; init; }

    public required ActivityKind Kind { get; init; }

    public required string Title { get; set; }

    public required string Subtitle { get; set; }

    public string Body { get; set; } = string.Empty;

    public string Badge { get; set; } = string.Empty;

    public ObservableCollection<MaterialItem> Materials { get; } = new();

    public ObservableCollection<FileChangeItem> FileChanges { get; } = new();
}

public sealed class FileChangeItem
{
    public required string Path { get; init; }

    public required int Added { get; init; }

    public required int Removed { get; init; }

    public string DeltaText => $"+{Added} -{Removed}";
}

public sealed class WorkspaceSeed
{
    public ObservableCollection<ProjectItem> Projects { get; } = new();
}
