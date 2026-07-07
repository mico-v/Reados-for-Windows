using ReadOS.App.Models;
using ReadOS.App.Services;

namespace ReadOS.App.Services.Msp;

internal readonly record struct ReadOsDocumentSearchPreparation(
    bool ShouldClearResults,
    bool ShouldSearch,
    LibraryItem? Document,
    string Query);

internal readonly record struct ReadOsDocumentSearchResult(
    IReadOnlyList<PdfTextHit> Hits,
    string StatusMessage);

internal readonly record struct ReadOsDocumentSearchRoute(
    bool ShouldNavigate,
    int PageNumber);

internal sealed class ReadOsDocumentSearchService
{
    public ReadOsDocumentSearchPreparation Prepare(
        LibraryItem? document,
        bool hasPdfDocument,
        string query)
    {
        if (document is null || !hasPdfDocument || string.IsNullOrWhiteSpace(query))
        {
            return new ReadOsDocumentSearchPreparation(true, false, null, string.Empty);
        }

        return new ReadOsDocumentSearchPreparation(true, true, document, query);
    }

    public ReadOsDocumentSearchResult ProjectResults(IReadOnlyList<PdfTextHit> hits)
    {
        return new ReadOsDocumentSearchResult(
            hits,
            hits.Count == 0 ? "未找到匹配内容。" : $"找到 {hits.Count} 个匹配项。");
    }

    public ReadOsDocumentSearchRoute ResolveRoute(PdfTextHit? hit)
    {
        return hit is null
            ? new ReadOsDocumentSearchRoute(false, 0)
            : new ReadOsDocumentSearchRoute(true, hit.PageNumber);
    }
}
