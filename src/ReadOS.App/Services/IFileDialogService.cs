using Microsoft.UI.Xaml;

namespace ReadOS.App.Services;

public interface IFileDialogService
{
    Task<IReadOnlyList<string>> PickPdfFilesAsync(Window window);

    Task<string?> PickWorkspaceImportAsync(Window window);

    Task<string?> PickWorkspaceExportAsync(Window window, string suggestedName);
}
