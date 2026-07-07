using ReadOS.App.Models;

namespace ReadOS.App.Services.Msp;

internal readonly record struct ReadOsPageNavigation(
    bool ShouldNavigate,
    int TargetPage,
    string? PageJumpText,
    string? CurrentPageLabelDraft,
    bool ShouldLoadCurrentPage,
    bool ShouldSaveWorkspace);

internal sealed class ReadOsPageNavigationService
{
    public ReadOsPageNavigation Prepare(
        bool hasWorkspace,
        LibraryItem? document,
        int requestedPage)
    {
        if (!hasWorkspace || document is null || document.PageCount == 0)
        {
            return new ReadOsPageNavigation(false, 0, null, null, false, false);
        }

        var targetPage = Math.Clamp(requestedPage, 1, document.PageCount);
        return new ReadOsPageNavigation(
            true,
            targetPage,
            targetPage.ToString(),
            ResolvePageLabel(document, targetPage),
            true,
            true);
    }

    public int ResolvePage(LibraryItem? document, string text)
    {
        if (document is null || string.IsNullOrWhiteSpace(text))
        {
            return 0;
        }

        var normalized = text.Trim();
        if (int.TryParse(normalized, out var page))
        {
            return Math.Clamp(page, 1, document.PageCount);
        }

        var label = document.PageLabels.FirstOrDefault(item =>
            string.Equals(item.Label, normalized, StringComparison.OrdinalIgnoreCase));
        return label?.PdfPage ?? 0;
    }

    private static string ResolvePageLabel(LibraryItem document, int pageNumber)
    {
        var label = document.PageLabels.FirstOrDefault(item => item.PdfPage == pageNumber)?.Label;
        return string.IsNullOrWhiteSpace(label) ? pageNumber.ToString() : label;
    }
}
