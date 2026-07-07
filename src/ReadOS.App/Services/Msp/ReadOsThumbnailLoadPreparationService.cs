using ReadOS.App.Models;

namespace ReadOS.App.Services.Msp;

internal readonly record struct ReadOsThumbnailLoadPreparation(
    bool ShouldLoad,
    LibraryItem? Document,
    int PageCount,
    int MaxPages,
    int SelectedPageNumber,
    bool ShouldClearThumbnails);

internal readonly record struct ReadOsThumbnailProjection(
    IReadOnlyList<PageImageItem> Thumbnails,
    PageImageItem? SelectedThumbnail);

internal sealed class ReadOsThumbnailLoadPreparationService
{
    private const int DefaultMaxThumbnailPages = 24;

    public ReadOsThumbnailLoadPreparation Prepare(LibraryItem? document, int currentPageNumber)
    {
        if (document is null || document.Kind != LibraryItemKind.Pdf)
        {
            return new ReadOsThumbnailLoadPreparation(
                false,
                null,
                0,
                DefaultMaxThumbnailPages,
                currentPageNumber,
                true);
        }

        return new ReadOsThumbnailLoadPreparation(
            true,
            document,
            document.PageCount,
            DefaultMaxThumbnailPages,
            currentPageNumber,
            true);
    }

    public ReadOsThumbnailProjection Project(
        LibraryItem document,
        IEnumerable<PageImageItem> thumbnails,
        int selectedPageNumber)
    {
        var projected = thumbnails.ToArray();
        foreach (var thumbnail in projected)
        {
            var label = document.PageLabels.FirstOrDefault(item => item.PdfPage == thumbnail.PageNumber)?.Label;
            thumbnail.Label = string.IsNullOrWhiteSpace(label)
                ? thumbnail.PageNumber.ToString()
                : label;
        }

        return new ReadOsThumbnailProjection(
            projected,
            projected.FirstOrDefault(item => item.PageNumber == selectedPageNumber));
    }
}
