using ReadOS.App.Models;

namespace ReadOS.App.Services.Msp;

internal readonly record struct ReadOsReaderAttachmentResult(
    bool ShouldApply,
    IReadOnlyList<ChatAttachment> Attachments,
    string? PageRangeDraft,
    string? ComposerDraft,
    bool ShouldExitRegionMode,
    string? StatusMessage);

internal sealed class ReadOsReaderAttachmentService
{
    private readonly ReadOsPageNavigationService pageNavigationService;

    public ReadOsReaderAttachmentService(ReadOsPageNavigationService pageNavigationService)
    {
        this.pageNavigationService = pageNavigationService;
    }

    public ReadOsReaderAttachmentResult AttachCurrentPage(
        LibraryItem? document,
        int currentPageNumber,
        string documentPath)
    {
        if (document is null)
        {
            return Noop();
        }

        if (document.Kind is LibraryItemKind.Markdown or LibraryItemKind.Note)
        {
            return Apply(new[]
            {
                CreateFileAttachment(document, documentPath)
            });
        }

        if (currentPageNumber <= 0)
        {
            return Noop();
        }

        return Apply(new[]
        {
            new ChatAttachment
            {
                Kind = AttachmentKind.Page,
                DocumentId = document.Id,
                Title = $"{document.Name} · 第 {currentPageNumber} 页",
                StartPage = currentPageNumber,
                EndPage = currentPageNumber
            }
        });
    }

    public ReadOsReaderAttachmentResult AttachRange(
        LibraryItem? document,
        string documentPath,
        string rangeDraft)
    {
        if (document is null)
        {
            return Noop();
        }

        if (document.Kind is LibraryItemKind.Markdown or LibraryItemKind.Note)
        {
            return new ReadOsReaderAttachmentResult(
                true,
                new[] { CreateFileAttachment(document, documentPath) },
                null,
                null,
                false,
                "文本资料已作为全文附件加入当前对话。");
        }

        var ranges = ParseRanges(document, rangeDraft);
        if (ranges.Count == 0)
        {
            return new ReadOsReaderAttachmentResult(
                false,
                Array.Empty<ChatAttachment>(),
                null,
                null,
                false,
                "请输入页码范围，例如 3-5, 8。");
        }

        return new ReadOsReaderAttachmentResult(
            true,
            ranges.Select(range => new ChatAttachment
            {
                Kind = range.Start == range.End ? AttachmentKind.Page : AttachmentKind.PageRange,
                DocumentId = document.Id,
                Title = $"{document.Name} · 第 {range.Start}-{range.End} 页",
                StartPage = range.Start,
                EndPage = range.End
            }).ToArray(),
            string.Empty,
            null,
            false,
            null);
    }

    public ReadOsReaderAttachmentResult AttachRegion(
        LibraryItem? document,
        int currentPageNumber,
        double x,
        double y,
        double width,
        double height,
        string regionPrompt)
    {
        if (document is null || currentPageNumber <= 0)
        {
            return Noop();
        }

        return new ReadOsReaderAttachmentResult(
            true,
            new[]
            {
                new ChatAttachment
                {
                    Kind = AttachmentKind.Region,
                    DocumentId = document.Id,
                    Title = $"{document.Name} · 第 {currentPageNumber} 页红框区域",
                    StartPage = Math.Max(1, currentPageNumber - 1),
                    EndPage = Math.Min(document.PageCount, currentPageNumber + 1),
                    RegionX = x,
                    RegionY = y,
                    RegionWidth = width,
                    RegionHeight = height
                }
            },
            null,
            regionPrompt,
            true,
            "已添加红框区域和上下文页面。");
    }

    private IReadOnlyList<(int Start, int End)> ParseRanges(LibraryItem document, string text)
    {
        var ranges = new List<(int Start, int End)>();
        if (string.IsNullOrWhiteSpace(text))
        {
            return ranges;
        }

        foreach (var part in text.Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries))
        {
            var edges = part.Split('-', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
            var start = pageNavigationService.ResolvePage(document, edges[0]);
            var end = edges.Length > 1 ? pageNavigationService.ResolvePage(document, edges[1]) : start;
            if (start <= 0 || end <= 0)
            {
                continue;
            }

            ranges.Add((Math.Min(start, end), Math.Max(start, end)));
        }

        return ranges;
    }

    private static ChatAttachment CreateFileAttachment(LibraryItem document, string documentPath)
    {
        return new ChatAttachment
        {
            Kind = AttachmentKind.File,
            DocumentId = document.Id,
            Title = $"{document.Name} · 全文",
            FilePath = string.Empty
        };
    }

    private static ReadOsReaderAttachmentResult Apply(IReadOnlyList<ChatAttachment> attachments)
    {
        return new ReadOsReaderAttachmentResult(
            true,
            attachments,
            null,
            null,
            false,
            null);
    }

    private static ReadOsReaderAttachmentResult Noop()
    {
        return new ReadOsReaderAttachmentResult(
            false,
            Array.Empty<ChatAttachment>(),
            null,
            null,
            false,
            null);
    }
}
