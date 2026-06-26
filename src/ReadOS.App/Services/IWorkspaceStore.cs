using ReadOS.App.Models;

namespace ReadOS.App.Services;

public interface IWorkspaceStore
{
    string WorkspaceRoot { get; }

    string LibraryRoot { get; }

    Task<WorkspaceState> LoadAsync(CancellationToken cancellationToken = default);

    Task SaveAsync(WorkspaceState state, CancellationToken cancellationToken = default);

    Task<ProjectItem> CreateProjectAsync(WorkspaceState state, string name, CancellationToken cancellationToken = default);

    Task<LibraryItem> ImportDocumentAsync(WorkspaceState state, ProjectItem project, string sourcePath, CancellationToken cancellationToken = default);

    Task RenameDocumentAsync(WorkspaceState state, LibraryItem document, string newName, CancellationToken cancellationToken = default);

    Task DeleteDocumentAsync(WorkspaceState state, ProjectItem project, LibraryItem document, CancellationToken cancellationToken = default);

    Task<string> ExportWorkspaceAsync(WorkspaceState state, string destinationPath, CancellationToken cancellationToken = default);

    Task<WorkspaceState> ImportWorkspaceAsync(string sourcePath, CancellationToken cancellationToken = default);

    string GetAbsolutePath(LibraryItem item);
}
