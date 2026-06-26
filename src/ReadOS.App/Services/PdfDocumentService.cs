using System.Text;
using Microsoft.UI.Xaml.Media.Imaging;
using ReadOS.App.Models;
using UglyToad.PdfPig.DocumentLayoutAnalysis.TextExtractor;
using UglyToad.PdfPig.Outline;
using Windows.Storage;
using Windows.Storage.Streams;
using WinPdfDocument = Windows.Data.Pdf.PdfDocument;
using WinPdfPageRenderOptions = Windows.Data.Pdf.PdfPageRenderOptions;
using PigPdfDocument = UglyToad.PdfPig.PdfDocument;

namespace ReadOS.App.Services;

public sealed class PdfDocumentService : IPdfDocumentService
{
    public async Task<PdfDocumentInfo> InspectAsync(string path, CancellationToken cancellationToken = default)
    {
        var pageCount = 0;
        var outline = new List<OutlineItem>();
        try
        {
            using var document = PigPdfDocument.Open(path);
            pageCount = document.NumberOfPages;

            if (document.TryGetBookmarks(out var bookmarks, true))
            {
                AddBookmarks(outline, bookmarks.Roots);
            }
        }
        catch
        {
            var storageFile = await StorageFile.GetFileFromPathAsync(path);
            var document = await WinPdfDocument.LoadFromFileAsync(storageFile);
            pageCount = (int)document.PageCount;
        }

        var labels = Enumerable.Range(1, Math.Max(0, pageCount))
            .Select(page => new PageLabelRule { PdfPage = page, Label = page.ToString() })
            .ToList();

        if (outline.Count == 0 && pageCount > 0)
        {
            outline.Add(new OutlineItem { Title = "开始阅读", Page = 1, Level = 1 });
        }

        return new PdfDocumentInfo(pageCount, labels, outline);
    }

    public async Task<BitmapImage> RenderPageAsync(string path, int pageNumber, double width, CancellationToken cancellationToken = default)
    {
        var storageFile = await StorageFile.GetFileFromPathAsync(path);
        var document = await WinPdfDocument.LoadFromFileAsync(storageFile);
        if (document.PageCount == 0)
        {
            return new BitmapImage();
        }

        var index = (uint)Math.Clamp(pageNumber - 1, 0, (int)document.PageCount - 1);
        using var page = document.GetPage(index);
        using var randomStream = new InMemoryRandomAccessStream();
        var options = new WinPdfPageRenderOptions();
        if (width > 0)
        {
            options.DestinationWidth = (uint)Math.Clamp(width, 320, 1800);
        }

        await page.RenderToStreamAsync(randomStream, options);
        randomStream.Seek(0);
        var bitmap = new BitmapImage();
        await bitmap.SetSourceAsync(randomStream);
        return bitmap;
    }

    public async Task<IReadOnlyList<PageImageItem>> RenderThumbnailsAsync(string path, int pageCount, int maxPages, CancellationToken cancellationToken = default)
    {
        var count = Math.Min(pageCount, maxPages);
        var result = new List<PageImageItem>(count);
        for (var page = 1; page <= count; page++)
        {
            cancellationToken.ThrowIfCancellationRequested();
            result.Add(new PageImageItem
            {
                PageNumber = page,
                Label = page.ToString(),
                Image = await RenderPageAsync(path, page, 180, cancellationToken)
            });
        }

        return result;
    }

    public Task<IReadOnlyList<PdfTextHit>> SearchAsync(string path, string query, CancellationToken cancellationToken = default)
    {
        return Task.Run<IReadOnlyList<PdfTextHit>>(() =>
        {
            var hits = new List<PdfTextHit>();
            if (string.IsNullOrWhiteSpace(query))
            {
                return hits;
            }

            using var document = PigPdfDocument.Open(path);
            foreach (var page in document.GetPages())
            {
                cancellationToken.ThrowIfCancellationRequested();
                var text = ContentOrderTextExtractor.GetText(page);
                var index = text.IndexOf(query, StringComparison.OrdinalIgnoreCase);
                if (index < 0)
                {
                    continue;
                }

                var start = Math.Max(0, index - 60);
                var length = Math.Min(text.Length - start, query.Length + 120);
                var preview = NormalizeWhitespace(text.Substring(start, length));
                hits.Add(new PdfTextHit(page.Number, preview));
                if (hits.Count >= 50)
                {
                    break;
                }
            }

            return hits;
        }, cancellationToken);
    }

    public Task<string> ExtractPageTextAsync(string path, int startPage, int endPage, CancellationToken cancellationToken = default)
    {
        return Task.Run(() =>
        {
            using var document = PigPdfDocument.Open(path);
            var builder = new StringBuilder();
            var from = Math.Clamp(Math.Min(startPage, endPage), 1, document.NumberOfPages);
            var to = Math.Clamp(Math.Max(startPage, endPage), 1, document.NumberOfPages);
            for (var pageNumber = from; pageNumber <= to; pageNumber++)
            {
                cancellationToken.ThrowIfCancellationRequested();
                var page = document.GetPage(pageNumber);
                builder.AppendLine($"[PDF 第 {pageNumber} 页]");
                builder.AppendLine(ContentOrderTextExtractor.GetText(page));
                builder.AppendLine();
            }

            return builder.ToString().Trim();
        }, cancellationToken);
    }

    private static void AddBookmarks(ICollection<OutlineItem> outline, IReadOnlyList<BookmarkNode> nodes)
    {
        foreach (var node in nodes)
        {
            var pageNumber = node is DocumentBookmarkNode documentNode ? documentNode.PageNumber : 0;
            if (!string.IsNullOrWhiteSpace(node.Title))
            {
                outline.Add(new OutlineItem
                {
                    Title = node.Title,
                    Page = Math.Max(1, pageNumber),
                    Level = Math.Max(1, node.Level + 1)
                });
            }

            if (!node.IsLeaf)
            {
                AddBookmarks(outline, node.Children);
            }
        }
    }

    private static string NormalizeWhitespace(string value)
    {
        var builder = new StringBuilder(value.Length);
        var previousWasWhitespace = false;
        foreach (var character in value)
        {
            if (char.IsWhiteSpace(character))
            {
                if (!previousWasWhitespace)
                {
                    builder.Append(' ');
                    previousWasWhitespace = true;
                }
            }
            else
            {
                builder.Append(character);
                previousWasWhitespace = false;
            }
        }

        return builder.ToString().Trim();
    }
}
