using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsPageLabelEditingServiceTests
{
    [Fact]
    public void SaveLabel_noops_when_prerequisites_are_missing()
    {
        var service = new ReadOsPageLabelEditingService();
        var document = new LibraryItem
        {
            PageCount = 5
        };

        var withoutWorkspace = service.SaveLabel(false, document, 2, "ii");
        var withoutDocument = service.SaveLabel(true, null, 2, "ii");
        var withoutPage = service.SaveLabel(true, document, 0, "ii");

        Assert.False(withoutWorkspace.ShouldSave);
        Assert.False(withoutDocument.ShouldSave);
        Assert.False(withoutPage.ShouldSave);
        Assert.Empty(document.PageLabels);
    }

    [Fact]
    public void SaveLabel_creates_new_label_and_trims_draft()
    {
        var service = new ReadOsPageLabelEditingService();
        var document = new LibraryItem
        {
            PageCount = 5
        };

        var result = service.SaveLabel(true, document, 3, " iii ");

        var label = Assert.Single(document.PageLabels);
        Assert.True(result.ShouldSave);
        Assert.Equal(3, label.PdfPage);
        Assert.Equal("iii", label.Label);
        Assert.Equal("iii", result.CurrentPageLabelDraft);
        Assert.Equal("已保存第 3 页标签：iii", result.StatusMessage);
    }

    [Fact]
    public void SaveLabel_updates_existing_label()
    {
        var service = new ReadOsPageLabelEditingService();
        var document = new LibraryItem
        {
            PageCount = 5
        };
        document.PageLabels.Add(new PageLabelRule
        {
            PdfPage = 4,
            Label = "old"
        });

        var result = service.SaveLabel(true, document, 4, "iv");

        var label = Assert.Single(document.PageLabels);
        Assert.True(result.ShouldSave);
        Assert.Equal("iv", label.Label);
        Assert.Equal("iv", result.CurrentPageLabelDraft);
    }

    [Fact]
    public void SaveLabel_falls_back_to_page_number_for_blank_draft()
    {
        var service = new ReadOsPageLabelEditingService();
        var document = new LibraryItem
        {
            PageCount = 5
        };

        var result = service.SaveLabel(true, document, 5, " ");

        var label = Assert.Single(document.PageLabels);
        Assert.Equal("5", label.Label);
        Assert.Equal("5", result.CurrentPageLabelDraft);
    }

    [Fact]
    public void AutoMap_noops_when_prerequisites_are_missing()
    {
        var service = new ReadOsPageLabelEditingService();
        var document = new LibraryItem
        {
            PageCount = 0
        };

        var withoutWorkspace = service.AutoMap(false, new LibraryItem { PageCount = 3 });
        var withoutDocument = service.AutoMap(true, null);
        var emptyDocument = service.AutoMap(true, document);

        Assert.False(withoutWorkspace.ShouldSave);
        Assert.False(withoutDocument.ShouldSave);
        Assert.False(emptyDocument.ShouldSave);
        Assert.Empty(document.PageLabels);
    }

    [Fact]
    public void AutoMap_replaces_labels_with_existing_default_mapping()
    {
        var service = new ReadOsPageLabelEditingService();
        var document = new LibraryItem
        {
            PageCount = 7,
            CurrentPage = 5
        };
        document.PageLabels.Add(new PageLabelRule
        {
            PdfPage = 1,
            Label = "old"
        });

        var result = service.AutoMap(true, document);

        Assert.True(result.ShouldSave);
        Assert.Equal(
            new[] { "cover", "i", "ii", "iii", "1", "2", "3" },
            document.PageLabels.OrderBy(item => item.PdfPage).Select(item => item.Label));
        Assert.Equal("1", result.CurrentPageLabelDraft);
        Assert.Equal("已生成可编辑页码映射。", result.StatusMessage);
    }
}
