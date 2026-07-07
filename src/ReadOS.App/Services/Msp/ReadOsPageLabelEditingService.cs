using ReadOS.App.Models;

namespace ReadOS.App.Services.Msp;

internal readonly record struct ReadOsPageLabelEditResult(
    bool ShouldSave,
    string? CurrentPageLabelDraft,
    string? StatusMessage);

internal sealed class ReadOsPageLabelEditingService
{
    public ReadOsPageLabelEditResult SaveLabel(
        bool hasWorkspace,
        LibraryItem? document,
        int currentPageNumber,
        string labelDraft)
    {
        if (!hasWorkspace || document is null || currentPageNumber <= 0)
        {
            return new ReadOsPageLabelEditResult(false, null, null);
        }

        var label = document.PageLabels.FirstOrDefault(item => item.PdfPage == currentPageNumber);
        if (label is null)
        {
            label = new PageLabelRule { PdfPage = currentPageNumber };
            document.PageLabels.Add(label);
        }

        label.Label = NormalizeLabel(labelDraft, currentPageNumber);
        return new ReadOsPageLabelEditResult(
            true,
            label.Label,
            $"已保存第 {currentPageNumber} 页标签：{label.Label}");
    }

    public ReadOsPageLabelEditResult AutoMap(
        bool hasWorkspace,
        LibraryItem? document)
    {
        if (!hasWorkspace || document is null || document.PageCount == 0)
        {
            return new ReadOsPageLabelEditResult(false, null, null);
        }

        document.PageLabels.Clear();
        for (var page = 1; page <= document.PageCount; page++)
        {
            document.PageLabels.Add(new PageLabelRule
            {
                PdfPage = page,
                Label = CreateDefaultLabel(page)
            });
        }

        return new ReadOsPageLabelEditResult(
            true,
            document.CurrentPageLabel,
            "已生成可编辑页码映射。");
    }

    private static string NormalizeLabel(string labelDraft, int currentPageNumber)
    {
        return string.IsNullOrWhiteSpace(labelDraft)
            ? currentPageNumber.ToString()
            : labelDraft.Trim();
    }

    private static string CreateDefaultLabel(int page)
    {
        return page switch
        {
            1 => "cover",
            2 => "i",
            3 => "ii",
            4 => "iii",
            _ => (page - 4).ToString()
        };
    }
}
