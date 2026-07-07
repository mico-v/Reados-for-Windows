using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsReaderAttachmentServiceTests
{
    [Fact]
    public void AttachCurrentPage_noops_without_document_or_current_page()
    {
        var service = CreateService();
        var document = CreatePdfDocument();

        var missingDocument = service.AttachCurrentPage(null, 3, "doc.pdf");
        var missingPage = service.AttachCurrentPage(document, 0, "doc.pdf");

        Assert.False(missingDocument.ShouldApply);
        Assert.False(missingPage.ShouldApply);
        Assert.Empty(missingDocument.Attachments);
        Assert.Empty(missingPage.Attachments);
    }

    [Fact]
    public void AttachCurrentPage_builds_file_attachment_for_text_documents()
    {
        var service = CreateService();
        var document = new LibraryItem
        {
            Id = "doc",
            Kind = LibraryItemKind.Markdown,
            Name = "Notes.md"
        };

        var result = service.AttachCurrentPage(document, 0, "notes.md");

        var attachment = Assert.Single(result.Attachments);
        Assert.True(result.ShouldApply);
        Assert.Equal(AttachmentKind.File, attachment.Kind);
        Assert.Equal("doc", attachment.DocumentId);
        Assert.Equal("Notes.md · 全文", attachment.Title);
        Assert.Equal("notes.md", attachment.FilePath);
    }

    [Fact]
    public void AttachCurrentPage_builds_single_page_attachment_for_pdf_documents()
    {
        var service = CreateService();
        var document = CreatePdfDocument();

        var result = service.AttachCurrentPage(document, 4, "doc.pdf");

        var attachment = Assert.Single(result.Attachments);
        Assert.True(result.ShouldApply);
        Assert.Equal(AttachmentKind.Page, attachment.Kind);
        Assert.Equal("doc", attachment.DocumentId);
        Assert.Equal("Guide.pdf · 第 4 页", attachment.Title);
        Assert.Equal(4, attachment.StartPage);
        Assert.Equal(4, attachment.EndPage);
    }

    [Fact]
    public void AttachRange_builds_file_attachment_and_status_for_text_documents()
    {
        var service = CreateService();
        var document = new LibraryItem
        {
            Id = "doc",
            Kind = LibraryItemKind.Note,
            Name = "Note"
        };

        var result = service.AttachRange(document, "note.txt", "3-5");

        var attachment = Assert.Single(result.Attachments);
        Assert.True(result.ShouldApply);
        Assert.Equal(AttachmentKind.File, attachment.Kind);
        Assert.Equal("文本资料已作为全文附件加入当前对话。", result.StatusMessage);
        Assert.Null(result.PageRangeDraft);
    }

    [Fact]
    public void AttachRange_reports_existing_prompt_when_range_is_invalid()
    {
        var service = CreateService();
        var document = CreatePdfDocument();

        var result = service.AttachRange(document, "doc.pdf", "missing");

        Assert.False(result.ShouldApply);
        Assert.Empty(result.Attachments);
        Assert.Equal("请输入页码范围，例如 3-5, 8。", result.StatusMessage);
        Assert.Null(result.PageRangeDraft);
    }

    [Fact]
    public void AttachRange_parses_numbers_labels_and_reversed_ranges()
    {
        var service = CreateService();
        var document = CreatePdfDocument();
        document.PageLabels.Add(new PageLabelRule
        {
            PdfPage = 5,
            Label = "v"
        });

        var result = service.AttachRange(document, "doc.pdf", "3-5, v, 9-7");

        Assert.True(result.ShouldApply);
        Assert.Equal(string.Empty, result.PageRangeDraft);
        Assert.Equal(3, result.Attachments.Count);
        Assert.Equal(AttachmentKind.PageRange, result.Attachments[0].Kind);
        Assert.Equal(3, result.Attachments[0].StartPage);
        Assert.Equal(5, result.Attachments[0].EndPage);
        Assert.Equal(AttachmentKind.Page, result.Attachments[1].Kind);
        Assert.Equal(5, result.Attachments[1].StartPage);
        Assert.Equal(5, result.Attachments[1].EndPage);
        Assert.Equal(7, result.Attachments[2].StartPage);
        Assert.Equal(9, result.Attachments[2].EndPage);
    }

    [Fact]
    public void AttachRegion_noops_without_document_or_current_page()
    {
        var service = CreateService();
        var document = CreatePdfDocument();

        var missingDocument = service.AttachRegion(null, 2, 0.1, 0.2, 0.3, 0.4, "explain");
        var missingPage = service.AttachRegion(document, 0, 0.1, 0.2, 0.3, 0.4, "explain");

        Assert.False(missingDocument.ShouldApply);
        Assert.False(missingPage.ShouldApply);
    }

    [Fact]
    public void AttachRegion_builds_region_attachment_with_context_pages_and_prompt()
    {
        var service = CreateService();
        var document = CreatePdfDocument();

        var result = service.AttachRegion(document, 1, 0.1, 0.2, 0.3, 0.4, "explain region");

        var attachment = Assert.Single(result.Attachments);
        Assert.True(result.ShouldApply);
        Assert.Equal(AttachmentKind.Region, attachment.Kind);
        Assert.Equal("Guide.pdf · 第 1 页红框区域", attachment.Title);
        Assert.Equal(1, attachment.StartPage);
        Assert.Equal(2, attachment.EndPage);
        Assert.Equal(0.1, attachment.RegionX);
        Assert.Equal(0.2, attachment.RegionY);
        Assert.Equal(0.3, attachment.RegionWidth);
        Assert.Equal(0.4, attachment.RegionHeight);
        Assert.True(result.ShouldExitRegionMode);
        Assert.Equal("explain region", result.ComposerDraft);
        Assert.Equal("已添加红框区域和上下文页面。", result.StatusMessage);
    }

    private static ReadOsReaderAttachmentService CreateService()
    {
        return new ReadOsReaderAttachmentService(new ReadOsPageNavigationService());
    }

    private static LibraryItem CreatePdfDocument()
    {
        return new LibraryItem
        {
            Id = "doc",
            Kind = LibraryItemKind.Pdf,
            Name = "Guide.pdf",
            PageCount = 10
        };
    }
}
