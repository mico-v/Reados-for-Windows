using ReadOS.App.Models;

namespace ReadOS.App.Services.Msp;

internal enum ReadOsPresenterLoadMode
{
    Clear,
    LoadText,
    RenderPdfPage
}

internal readonly record struct ReadOsPresenterLoadPreparation(
    ReadOsPresenterLoadMode Mode,
    LibraryItem? Document,
    int PageNumber,
    bool ShouldDeactivateArtifactPreview,
    bool ShouldClearCurrentPageImage,
    bool ShouldClearPresenterText,
    bool ShouldNotifyActiveContext,
    bool ShouldRefreshPageSignals)
{
    public bool ShouldLoadText => Mode == ReadOsPresenterLoadMode.LoadText && Document is not null;

    public bool ShouldRenderPdfPage => Mode == ReadOsPresenterLoadMode.RenderPdfPage && Document is not null;
}

internal sealed class ReadOsPresenterLoadPreparationService
{
    public ReadOsPresenterLoadPreparation Prepare(LibraryItem? document, int currentPageNumber)
    {
        if (document is null)
        {
            return Clear(document);
        }

        if (document.Kind is LibraryItemKind.Markdown or LibraryItemKind.Note)
        {
            return new ReadOsPresenterLoadPreparation(
                ReadOsPresenterLoadMode.LoadText,
                document,
                0,
                true,
                true,
                false,
                true,
                false);
        }

        if (document.Kind != LibraryItemKind.Pdf || document.PageCount == 0)
        {
            return Clear(document);
        }

        return new ReadOsPresenterLoadPreparation(
            ReadOsPresenterLoadMode.RenderPdfPage,
            document,
            Math.Max(1, currentPageNumber),
            true,
            false,
            true,
            false,
            true);
    }

    private static ReadOsPresenterLoadPreparation Clear(LibraryItem? document)
    {
        return new ReadOsPresenterLoadPreparation(
            ReadOsPresenterLoadMode.Clear,
            document,
            0,
            true,
            true,
            true,
            true,
            false);
    }
}
