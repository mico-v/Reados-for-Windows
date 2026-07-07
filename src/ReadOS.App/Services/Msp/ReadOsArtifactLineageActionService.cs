using ReadOS.App.Models;

namespace ReadOS.App.Services.Msp;

internal readonly record struct ReadOsArtifactLineageAction(
    bool ShouldApply,
    WorkspaceArtifact? Artifact,
    string? StatusMessage);

internal sealed class ReadOsArtifactLineageActionService
{
    private readonly ReadOsArtifactService artifactService;

    public ReadOsArtifactLineageActionService(ReadOsArtifactService artifactService)
    {
        this.artifactService = artifactService;
    }

    public ReadOsArtifactLineageAction Resolve(
        ArtifactLineageItem? item,
        IEnumerable<WorkspaceArtifact>? artifacts)
    {
        if (item is null)
        {
            return new ReadOsArtifactLineageAction(false, null, null);
        }

        var artifact = artifactService.FindArtifact(artifacts ?? Array.Empty<WorkspaceArtifact>(), item.Path);
        if (artifact is null)
        {
            return new ReadOsArtifactLineageAction(
                true,
                null,
                $"该来源不是可打开的产物：{item.Path}");
        }

        return new ReadOsArtifactLineageAction(
            true,
            artifact,
            $"已打开来源产物：{artifact.Path}");
    }
}
