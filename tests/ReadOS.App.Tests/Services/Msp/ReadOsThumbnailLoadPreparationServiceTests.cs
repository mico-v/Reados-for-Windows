using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsThumbnailLoadPreparationServiceTests
{
    [Fact]
    public void Prepare_clears_thumbnails_when_document_is_missing()
    {
        var service = new ReadOsThumbnailLoadPreparationService();

        var preparation = service.Prepare(null, currentPageNumber: 8);

        Assert.False(preparation.ShouldLoad);
        Assert.Null(preparation.Document);
        Assert.Equal(0, preparation.PageCount);
        Assert.Equal(24, preparation.MaxPages);
        Assert.Equal(8, preparation.SelectedPageNumber);
        Assert.True(preparation.ShouldClearThumbnails);
    }

    [Fact]
    public void Prepare_clears_thumbnails_for_non_pdf_documents()
    {
        var service = new ReadOsThumbnailLoadPreparationService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Markdown,
            PageCount = 3
        };

        var preparation = service.Prepare(document, currentPageNumber: 2);

        Assert.False(preparation.ShouldLoad);
        Assert.Null(preparation.Document);
        Assert.Equal(0, preparation.PageCount);
        Assert.Equal(2, preparation.SelectedPageNumber);
        Assert.True(preparation.ShouldClearThumbnails);
    }

    [Fact]
    public void Prepare_loads_pdf_thumbnails_with_existing_page_limit()
    {
        var service = new ReadOsThumbnailLoadPreparationService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            PageCount = 42
        };

        var preparation = service.Prepare(document, currentPageNumber: 11);

        Assert.True(preparation.ShouldLoad);
        Assert.Same(document, preparation.Document);
        Assert.Equal(42, preparation.PageCount);
        Assert.Equal(24, preparation.MaxPages);
        Assert.Equal(11, preparation.SelectedPageNumber);
        Assert.True(preparation.ShouldClearThumbnails);
    }

    [Fact]
    public void Project_applies_document_page_labels_and_falls_back_to_page_number()
    {
        var service = new ReadOsThumbnailLoadPreparationService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            PageCount = 3
        };
        document.PageLabels.Add(new PageLabelRule
        {
            PdfPage = 2,
            Label = "ii"
        });
        document.PageLabels.Add(new PageLabelRule
        {
            PdfPage = 3,
            Label = " "
        });
        var thumbnails = new[]
        {
            new PageImageItem { PageNumber = 1, Label = "old" },
            new PageImageItem { PageNumber = 2, Label = "old" },
            new PageImageItem { PageNumber = 3, Label = "old" }
        };

        var projection = service.Project(document, thumbnails, selectedPageNumber: 2);

        Assert.Equal(new[] { "1", "ii", "3" }, projection.Thumbnails.Select(item => item.Label));
        Assert.Same(thumbnails[2 - 1], projection.SelectedThumbnail);
    }

    [Fact]
    public void Project_returns_null_selection_when_current_page_is_not_rendered()
    {
        var service = new ReadOsThumbnailLoadPreparationService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            PageCount = 5
        };
        var thumbnails = new[]
        {
            new PageImageItem { PageNumber = 1 },
            new PageImageItem { PageNumber = 2 }
        };

        var projection = service.Project(document, thumbnails, selectedPageNumber: 5);

        Assert.Null(projection.SelectedThumbnail);
    }
}
