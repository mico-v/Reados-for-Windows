using ReadOS.Msp.Models;

namespace ReadOS.Msp.Workspace;

public interface IMspWorkspace
{
    string NormalizePath(string path, string workingDirectory = "/");

    ValueTask<bool> ExistsAsync(string path, CancellationToken cancellationToken = default);

    ValueTask<IReadOnlyList<MspWorkspaceEntry>> ListAsync(string path, CancellationToken cancellationToken = default);

    ValueTask<string?> TryReadTextAsync(string path, CancellationToken cancellationToken = default);

    ValueTask WriteTextAsync(string path, string content, CancellationToken cancellationToken = default);

    ValueTask WriteTextAsync(
        string path,
        string content,
        MspArtifact? artifact,
        CancellationToken cancellationToken = default)
    {
        return WriteTextAsync(path, content, cancellationToken);
    }
}
