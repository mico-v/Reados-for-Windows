using ReadOS.App.Models;
using ReadOS.App.Services;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsDocumentSearchServiceTests
{
    [Fact]
    public void Prepare_clears_and_noops_when_search_is_unavailable()
    {
        var service = new ReadOsDocumentSearchService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf
        };

        var missingDocument = service.Prepare(null, hasPdfDocument: false, "alpha");
        var nonPdf = service.Prepare(document, hasPdfDocument: false, "alpha");
        var blankQuery = service.Prepare(document, hasPdfDocument: true, " ");

        Assert.True(missingDocument.ShouldClearResults);
        Assert.False(missingDocument.ShouldSearch);
        Assert.True(nonPdf.ShouldClearResults);
        Assert.False(nonPdf.ShouldSearch);
        Assert.True(blankQuery.ShouldClearResults);
        Assert.False(blankQuery.ShouldSearch);
    }

    [Fact]
    public void Prepare_searches_pdf_documents_and_preserves_query_text()
    {
        var service = new ReadOsDocumentSearchService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf
        };

        var search = service.Prepare(document, hasPdfDocument: true, " alpha ");

        Assert.True(search.ShouldClearResults);
        Assert.True(search.ShouldSearch);
        Assert.Same(document, search.Document);
        Assert.Equal(" alpha ", search.Query);
    }

    [Fact]
    public void ProjectResults_returns_empty_status_for_no_hits()
    {
        var service = new ReadOsDocumentSearchService();

        var result = service.ProjectResults(Array.Empty<PdfTextHit>());

        Assert.Empty(result.Hits);
        Assert.Equal("未找到匹配内容。", result.StatusMessage);
    }

    [Fact]
    public void ProjectResults_returns_hit_count_status()
    {
        var service = new ReadOsDocumentSearchService();
        var hits = new[]
        {
            new PdfTextHit(2, "alpha"),
            new PdfTextHit(5, "beta")
        };

        var result = service.ProjectResults(hits);

        Assert.Same(hits, result.Hits);
        Assert.Equal("找到 2 个匹配项。", result.StatusMessage);
    }

    [Fact]
    public void ResolveRoute_noops_for_missing_hit()
    {
        var service = new ReadOsDocumentSearchService();

        var route = service.ResolveRoute(null);

        Assert.False(route.ShouldNavigate);
        Assert.Equal(0, route.PageNumber);
    }

    [Fact]
    public void ResolveRoute_targets_hit_page()
    {
        var service = new ReadOsDocumentSearchService();

        var route = service.ResolveRoute(new PdfTextHit(7, "match"));

        Assert.True(route.ShouldNavigate);
        Assert.Equal(7, route.PageNumber);
    }
}
