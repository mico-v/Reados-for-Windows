using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsPresenterLoadPreparationServiceTests
{
    [Fact]
    public void Prepare_clears_presenter_when_document_is_missing()
    {
        var service = new ReadOsPresenterLoadPreparationService();

        var preparation = service.Prepare(null, currentPageNumber: 5);

        Assert.Equal(ReadOsPresenterLoadMode.Clear, preparation.Mode);
        Assert.Null(preparation.Document);
        Assert.Equal(0, preparation.PageNumber);
        Assert.True(preparation.ShouldDeactivateArtifactPreview);
        Assert.True(preparation.ShouldClearCurrentPageImage);
        Assert.True(preparation.ShouldClearPresenterText);
        Assert.True(preparation.ShouldNotifyActiveContext);
        Assert.False(preparation.ShouldRefreshPageSignals);
        Assert.False(preparation.ShouldLoadText);
        Assert.False(preparation.ShouldRenderPdfPage);
    }

    [Theory]
    [InlineData(LibraryItemKind.Markdown)]
    [InlineData(LibraryItemKind.Note)]
    public void Prepare_loads_text_documents_without_clearing_existing_text_first(LibraryItemKind kind)
    {
        var service = new ReadOsPresenterLoadPreparationService();
        var document = new LibraryItem
        {
            Kind = kind,
            Name = "Notes"
        };

        var preparation = service.Prepare(document, currentPageNumber: 3);

        Assert.Equal(ReadOsPresenterLoadMode.LoadText, preparation.Mode);
        Assert.Same(document, preparation.Document);
        Assert.Equal(0, preparation.PageNumber);
        Assert.True(preparation.ShouldDeactivateArtifactPreview);
        Assert.True(preparation.ShouldClearCurrentPageImage);
        Assert.False(preparation.ShouldClearPresenterText);
        Assert.True(preparation.ShouldNotifyActiveContext);
        Assert.False(preparation.ShouldRefreshPageSignals);
        Assert.True(preparation.ShouldLoadText);
        Assert.False(preparation.ShouldRenderPdfPage);
    }

    [Fact]
    public void Prepare_clears_presenter_for_empty_pdf_documents()
    {
        var service = new ReadOsPresenterLoadPreparationService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            PageCount = 0
        };

        var preparation = service.Prepare(document, currentPageNumber: 2);

        Assert.Equal(ReadOsPresenterLoadMode.Clear, preparation.Mode);
        Assert.Same(document, preparation.Document);
        Assert.True(preparation.ShouldClearCurrentPageImage);
        Assert.True(preparation.ShouldClearPresenterText);
        Assert.True(preparation.ShouldNotifyActiveContext);
        Assert.False(preparation.ShouldRefreshPageSignals);
    }

    [Fact]
    public void Prepare_renders_pdf_page_and_clamps_non_positive_page_to_one()
    {
        var service = new ReadOsPresenterLoadPreparationService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            PageCount = 12,
            CurrentPage = 4
        };

        var preparation = service.Prepare(document, currentPageNumber: 0);

        Assert.Equal(ReadOsPresenterLoadMode.RenderPdfPage, preparation.Mode);
        Assert.Same(document, preparation.Document);
        Assert.Equal(1, preparation.PageNumber);
        Assert.True(preparation.ShouldDeactivateArtifactPreview);
        Assert.False(preparation.ShouldClearCurrentPageImage);
        Assert.True(preparation.ShouldClearPresenterText);
        Assert.False(preparation.ShouldNotifyActiveContext);
        Assert.True(preparation.ShouldRefreshPageSignals);
        Assert.False(preparation.ShouldLoadText);
        Assert.True(preparation.ShouldRenderPdfPage);
    }

    [Fact]
    public void Prepare_preserves_positive_pdf_page_number_for_rendering()
    {
        var service = new ReadOsPresenterLoadPreparationService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            PageCount = 12
        };

        var preparation = service.Prepare(document, currentPageNumber: 9);

        Assert.Equal(ReadOsPresenterLoadMode.RenderPdfPage, preparation.Mode);
        Assert.Equal(9, preparation.PageNumber);
    }
}
