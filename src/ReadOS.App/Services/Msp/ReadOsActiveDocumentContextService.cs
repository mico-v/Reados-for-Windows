using ReadOS.App.Models;

namespace ReadOS.App.Services.Msp;

internal readonly record struct ReadOsActiveDocumentContext(
    bool HasDocument,
    int CurrentPageNumber,
    string? DocumentNameDraft,
    string? PageJumpText,
    string? CurrentPageLabelDraft,
    bool ShouldClearPresenter,
    bool ShouldLoadCurrentPage,
    bool ShouldLoadThumbnails);

internal sealed class ReadOsActiveDocumentContextService
{
    public ReadOsActiveDocumentContext Resolve(LibraryItem? document)
    {
        if (document is null)
        {
            return new ReadOsActiveDocumentContext(
                false,
                0,
                null,
                null,
                null,
                true,
                false,
                false);
        }

        var currentPage = Math.Clamp(
            document.CurrentPage,
            document.PageCount > 0 ? 1 : 0,
            Math.Max(1, document.PageCount));

        return new ReadOsActiveDocumentContext(
            true,
            currentPage,
            document.Name,
            currentPage.ToString(),
            document.CurrentPageLabel,
            false,
            true,
            true);
    }
}
