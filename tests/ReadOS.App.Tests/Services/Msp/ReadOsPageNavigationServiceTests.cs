using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsPageNavigationServiceTests
{
    [Fact]
    public void Prepare_noops_when_workspace_or_document_is_missing()
    {
        var service = new ReadOsPageNavigationService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            PageCount = 5
        };

        var withoutWorkspace = service.Prepare(false, document, requestedPage: 3);
        var withoutDocument = service.Prepare(true, null, requestedPage: 3);

        Assert.False(withoutWorkspace.ShouldNavigate);
        Assert.False(withoutWorkspace.ShouldLoadCurrentPage);
        Assert.False(withoutWorkspace.ShouldSaveWorkspace);
        Assert.False(withoutDocument.ShouldNavigate);
    }

    [Fact]
    public void Prepare_noops_for_empty_documents()
    {
        var service = new ReadOsPageNavigationService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            PageCount = 0
        };

        var navigation = service.Prepare(true, document, requestedPage: 1);

        Assert.False(navigation.ShouldNavigate);
        Assert.Equal(0, navigation.TargetPage);
        Assert.Null(navigation.PageJumpText);
        Assert.Null(navigation.CurrentPageLabelDraft);
    }

    [Theory]
    [InlineData(-3, 1)]
    [InlineData(4, 4)]
    [InlineData(99, 12)]
    public void Prepare_clamps_target_page_and_requests_load_and_save(int requestedPage, int expectedPage)
    {
        var service = new ReadOsPageNavigationService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            PageCount = 12
        };

        var navigation = service.Prepare(true, document, requestedPage);

        Assert.True(navigation.ShouldNavigate);
        Assert.Equal(expectedPage, navigation.TargetPage);
        Assert.Equal(expectedPage.ToString(), navigation.PageJumpText);
        Assert.True(navigation.ShouldLoadCurrentPage);
        Assert.True(navigation.ShouldSaveWorkspace);
    }

    [Fact]
    public void Prepare_projects_page_label_for_target_page()
    {
        var service = new ReadOsPageNavigationService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            PageCount = 8
        };
        document.PageLabels.Add(new PageLabelRule
        {
            PdfPage = 3,
            Label = "iii"
        });

        var navigation = service.Prepare(true, document, requestedPage: 3);

        Assert.Equal("iii", navigation.CurrentPageLabelDraft);
    }

    [Theory]
    [InlineData("", 0)]
    [InlineData("2", 2)]
    [InlineData("99", 5)]
    [InlineData("-1", 1)]
    public void ResolvePage_handles_empty_and_numeric_text(string text, int expectedPage)
    {
        var service = new ReadOsPageNavigationService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            PageCount = 5
        };

        var page = service.ResolvePage(document, text);

        Assert.Equal(expectedPage, page);
    }

    [Fact]
    public void ResolvePage_matches_labels_case_insensitively()
    {
        var service = new ReadOsPageNavigationService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            PageCount = 8
        };
        document.PageLabels.Add(new PageLabelRule
        {
            PdfPage = 4,
            Label = "IV"
        });

        var page = service.ResolvePage(document, "iv");

        Assert.Equal(4, page);
    }

    [Fact]
    public void ResolvePage_returns_zero_for_missing_document_or_unknown_label()
    {
        var service = new ReadOsPageNavigationService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            PageCount = 5
        };

        Assert.Equal(0, service.ResolvePage(null, "1"));
        Assert.Equal(0, service.ResolvePage(document, "missing"));
    }
}
