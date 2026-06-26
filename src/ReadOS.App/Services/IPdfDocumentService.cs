using Microsoft.UI.Xaml.Media.Imaging;
using ReadOS.App.Models;

namespace ReadOS.App.Services;

public sealed record PdfDocumentInfo(int PageCount, IReadOnlyList<PageLabelRule> Labels, IReadOnlyList<OutlineItem> Outline);

public sealed record PdfTextHit(int PageNumber, string Preview);

public interface IPdfDocumentService
{
    Task<PdfDocumentInfo> InspectAsync(string path, CancellationToken cancellationToken = default);

    Task<BitmapImage> RenderPageAsync(string path, int pageNumber, double width, CancellationToken cancellationToken = default);

    Task<IReadOnlyList<PageImageItem>> RenderThumbnailsAsync(string path, int pageCount, int maxPages, CancellationToken cancellationToken = default);

    Task<IReadOnlyList<PdfTextHit>> SearchAsync(string path, string query, CancellationToken cancellationToken = default);

    Task<string> ExtractPageTextAsync(string path, int startPage, int endPage, CancellationToken cancellationToken = default);
}
