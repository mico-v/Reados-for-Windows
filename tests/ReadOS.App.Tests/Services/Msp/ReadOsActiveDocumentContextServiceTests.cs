using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsActiveDocumentContextServiceTests
{
    [Fact]
    public void Resolve_clears_presenter_for_missing_document_without_overwriting_drafts()
    {
        var service = new ReadOsActiveDocumentContextService();

        var context = service.Resolve(null);

        Assert.False(context.HasDocument);
        Assert.Equal(0, context.CurrentPageNumber);
        Assert.Null(context.DocumentNameDraft);
        Assert.Null(context.PageJumpText);
        Assert.Null(context.CurrentPageLabelDraft);
        Assert.True(context.ShouldClearPresenter);
        Assert.False(context.ShouldLoadCurrentPage);
        Assert.False(context.ShouldLoadThumbnails);
    }

    [Fact]
    public void Resolve_clamps_pdf_current_page_and_projects_document_drafts()
    {
        var service = new ReadOsActiveDocumentContextService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            Name = "Guide.pdf",
            PageCount = 12,
            CurrentPage = 99
        };
        document.PageLabels.Add(new PageLabelRule
        {
            PdfPage = 12,
            Label = "xii"
        });

        var context = service.Resolve(document);

        Assert.True(context.HasDocument);
        Assert.Equal(12, context.CurrentPageNumber);
        Assert.Equal("Guide.pdf", context.DocumentNameDraft);
        Assert.Equal("12", context.PageJumpText);
        Assert.Equal("99", context.CurrentPageLabelDraft);
        Assert.False(context.ShouldClearPresenter);
        Assert.True(context.ShouldLoadCurrentPage);
        Assert.True(context.ShouldLoadThumbnails);
    }

    [Fact]
    public void Resolve_uses_page_zero_for_empty_documents()
    {
        var service = new ReadOsActiveDocumentContextService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            Name = "Empty.pdf",
            PageCount = 0,
            CurrentPage = 3
        };

        var context = service.Resolve(document);

        Assert.True(context.HasDocument);
        Assert.Equal(1, context.CurrentPageNumber);
        Assert.Equal("1", context.PageJumpText);
        Assert.Equal("3", context.CurrentPageLabelDraft);
        Assert.True(context.ShouldLoadCurrentPage);
        Assert.True(context.ShouldLoadThumbnails);
    }

    [Fact]
    public void Resolve_projects_markdown_documents_for_text_loading_and_thumbnail_sync()
    {
        var service = new ReadOsActiveDocumentContextService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Markdown,
            Name = "Notes.md",
            PageCount = 0,
            CurrentPage = 1
        };

        var context = service.Resolve(document);

        Assert.True(context.HasDocument);
        Assert.Equal(1, context.CurrentPageNumber);
        Assert.Equal("Notes.md", context.DocumentNameDraft);
        Assert.Equal("1", context.PageJumpText);
        Assert.True(context.ShouldLoadCurrentPage);
        Assert.True(context.ShouldLoadThumbnails);
    }
}
