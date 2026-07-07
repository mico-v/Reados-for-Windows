using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsOutlineEditingServiceTests
{
    [Fact]
    public void GenerateOutline_noops_when_prerequisites_are_missing()
    {
        var service = new ReadOsOutlineEditingService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            PageCount = 10
        };

        var withoutWorkspace = service.GenerateOutline(false, document, hasPdfDocument: true, "Chapter 1 Intro");
        var withoutDocument = service.GenerateOutline(true, null, hasPdfDocument: true, "Chapter 1 Intro");
        var withoutPdf = service.GenerateOutline(true, document, hasPdfDocument: false, "Chapter 1 Intro");

        Assert.False(withoutWorkspace.ShouldSave);
        Assert.False(withoutDocument.ShouldSave);
        Assert.False(withoutPdf.ShouldSave);
        Assert.Empty(document.Outline);
    }

    [Fact]
    public void ExtractOutlineItems_reads_page_markers_and_supported_heading_shapes()
    {
        var service = new ReadOsOutlineEditingService();
        var text = string.Join(
            "\n",
            "[PDF 第 7 页]",
            "Chapter 1 Foundations",
            "plain sentence that should be ignored",
            "1.2 Evidence model",
            "[PDF 第 9 页]",
            "第十二章 结论",
            "1.2.3.4 Deep topic");

        var items = service.ExtractOutlineItems(text);

        Assert.Equal(4, items.Count);
        Assert.Equal("Chapter 1 Foundations", items[0].Title);
        Assert.Equal(7, items[0].Page);
        Assert.Equal(1, items[0].Level);
        Assert.Equal("1.2 Evidence model", items[1].Title);
        Assert.Equal(2, items[1].Level);
        Assert.Equal(9, items[2].Page);
        Assert.Equal("1.2.3.4 Deep topic", items[3].Title);
        Assert.Equal(4, items[3].Level);
    }

    [Fact]
    public void GenerateOutline_replaces_document_outline_and_caps_generated_items()
    {
        var service = new ReadOsOutlineEditingService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            PageCount = 100
        };
        document.Outline.Add(new OutlineItem
        {
            Title = "old",
            Page = 1,
            Level = 1
        });
        var text = string.Join("\n", Enumerable.Range(1, 90).Select(page => $"Section {page} Generated"));

        var result = service.GenerateOutline(true, document, hasPdfDocument: true, text);

        Assert.True(result.ShouldSave);
        Assert.True(result.ShouldRefreshOutline);
        Assert.False(result.ShouldClearTitleDraft);
        Assert.Equal(80, document.Outline.Count);
        Assert.Equal("Section 1 Generated", document.Outline[0].Title);
        Assert.Equal("Section 80 Generated", document.Outline[^1].Title);
        Assert.Equal("已生成 80 条目录项，可继续手动编辑。", result.StatusMessage);
    }

    [Fact]
    public void GenerateOutline_uses_fallback_page_steps_when_no_headings_are_found()
    {
        var service = new ReadOsOutlineEditingService();
        var document = new LibraryItem
        {
            Kind = LibraryItemKind.Pdf,
            PageCount = 25
        };

        var result = service.GenerateOutline(true, document, hasPdfDocument: true, "not a heading");

        Assert.True(result.ShouldSave);
        Assert.Equal(new[] { 1, 11, 21 }, document.Outline.Select(item => item.Page));
        Assert.Equal(new[] { "第 1 页起", "第 11 页起", "第 21 页起" }, document.Outline.Select(item => item.Title));
        Assert.Equal("已生成 3 条目录项，可继续手动编辑。", result.StatusMessage);
    }

    [Fact]
    public void AddOutlineItem_adds_trimmed_title_at_current_page_and_clears_draft()
    {
        var service = new ReadOsOutlineEditingService();
        var document = new LibraryItem();

        var result = service.AddOutlineItem(true, document, currentPageNumber: 6, "  Topic  ");

        var item = Assert.Single(document.Outline);
        Assert.True(result.ShouldSave);
        Assert.True(result.ShouldRefreshOutline);
        Assert.True(result.ShouldClearTitleDraft);
        Assert.Equal("Topic", item.Title);
        Assert.Equal(6, item.Page);
        Assert.Equal(1, item.Level);
    }

    [Fact]
    public void AddOutlineItem_uses_existing_default_title_and_minimum_page()
    {
        var service = new ReadOsOutlineEditingService();
        var document = new LibraryItem();

        var result = service.AddOutlineItem(true, document, currentPageNumber: 0, string.Empty);

        var item = Assert.Single(document.Outline);
        Assert.True(result.ShouldSave);
        Assert.Equal("第 0 页", item.Title);
        Assert.Equal(1, item.Page);
    }

    [Fact]
    public void DeleteOutlineItem_removes_selected_item()
    {
        var service = new ReadOsOutlineEditingService();
        var document = new LibraryItem();
        var selected = new OutlineItem
        {
            Id = "selected",
            Title = "selected",
            Page = 2
        };
        document.Outline.Add(new OutlineItem
        {
            Id = "other",
            Title = "other",
            Page = 1
        });
        document.Outline.Add(selected);

        var result = service.DeleteOutlineItem(true, document, selected);

        Assert.True(result.ShouldSave);
        Assert.True(result.ShouldRefreshOutline);
        Assert.Equal(new[] { "other" }, document.Outline.Select(item => item.Id));
    }

    [Fact]
    public void DeleteOutlineItem_noops_when_prerequisites_are_missing()
    {
        var service = new ReadOsOutlineEditingService();
        var document = new LibraryItem();

        var withoutWorkspace = service.DeleteOutlineItem(false, document, new OutlineItem());
        var withoutDocument = service.DeleteOutlineItem(true, null, new OutlineItem());
        var withoutSelection = service.DeleteOutlineItem(true, document, null);

        Assert.False(withoutWorkspace.ShouldSave);
        Assert.False(withoutDocument.ShouldSave);
        Assert.False(withoutSelection.ShouldSave);
    }
}
