using System.Text.RegularExpressions;
using ReadOS.App.Models;

namespace ReadOS.App.Services.Msp;

internal readonly record struct ReadOsOutlineEditResult(
    bool ShouldSave,
    bool ShouldRefreshOutline,
    bool ShouldClearTitleDraft,
    string? StatusMessage);

internal sealed class ReadOsOutlineEditingService
{
    private const int MaxGeneratedOutlineItems = 80;

    public ReadOsOutlineEditResult GenerateOutline(
        bool hasWorkspace,
        LibraryItem? document,
        bool hasPdfDocument,
        string extractedText)
    {
        if (!hasWorkspace || document is null || !hasPdfDocument)
        {
            return new ReadOsOutlineEditResult(false, false, false, null);
        }

        var generated = ExtractOutlineItems(extractedText);
        if (generated.Count == 0)
        {
            generated = BuildFallbackOutline(document.PageCount);
        }

        document.Outline.Clear();
        foreach (var item in generated.Take(MaxGeneratedOutlineItems))
        {
            document.Outline.Add(item);
        }

        return new ReadOsOutlineEditResult(
            true,
            true,
            false,
            $"已生成 {document.Outline.Count} 条目录项，可继续手动编辑。");
    }

    public ReadOsOutlineEditResult AddOutlineItem(
        bool hasWorkspace,
        LibraryItem? document,
        int currentPageNumber,
        string titleDraft)
    {
        if (!hasWorkspace || document is null)
        {
            return new ReadOsOutlineEditResult(false, false, false, null);
        }

        var title = string.IsNullOrWhiteSpace(titleDraft)
            ? $"第 {currentPageNumber} 页"
            : titleDraft.Trim();
        document.Outline.Add(new OutlineItem
        {
            Title = title,
            Page = Math.Max(1, currentPageNumber),
            Level = 1
        });

        return new ReadOsOutlineEditResult(true, true, true, null);
    }

    public ReadOsOutlineEditResult DeleteOutlineItem(
        bool hasWorkspace,
        LibraryItem? document,
        OutlineItem? selectedItem)
    {
        if (!hasWorkspace || document is null || selectedItem is null)
        {
            return new ReadOsOutlineEditResult(false, false, false, null);
        }

        document.Outline.Remove(selectedItem);
        return new ReadOsOutlineEditResult(true, true, false, null);
    }

    public List<OutlineItem> ExtractOutlineItems(string text)
    {
        var items = new List<OutlineItem>();
        var currentPage = 1;
        foreach (var rawLine in text.Split('\n'))
        {
            var line = rawLine.Trim();
            var pageMatch = Regex.Match(line, @"^\[PDF 第 (?<page>\d+) 页\]");
            if (pageMatch.Success && int.TryParse(pageMatch.Groups["page"].Value, out var parsedPage))
            {
                currentPage = parsedPage;
                continue;
            }

            if (line.Length is < 4 or > 90)
            {
                continue;
            }

            var isHeading =
                Regex.IsMatch(line, @"^(chapter|section)\s+\d+", RegexOptions.IgnoreCase) ||
                Regex.IsMatch(line, @"^第.{1,12}[章节篇]") ||
                Regex.IsMatch(line, @"^\d+(\.\d+){0,3}\s+\S+");
            if (!isHeading)
            {
                continue;
            }

            var level = line.Count(character => character == '.') + 1;
            items.Add(new OutlineItem
            {
                Title = line,
                Page = currentPage,
                Level = Math.Clamp(level, 1, 4)
            });
        }

        return items;
    }

    private static List<OutlineItem> BuildFallbackOutline(int pageCount)
    {
        var items = new List<OutlineItem>();
        for (var page = 1; page <= pageCount; page += 10)
        {
            items.Add(new OutlineItem
            {
                Title = $"第 {page} 页起",
                Page = page,
                Level = 1
            });
        }

        return items;
    }
}
