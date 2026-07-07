using Windows.ApplicationModel.DataTransfer;

namespace ReadOS.App.Services;

public sealed class ClipboardService : IClipboardService
{
    public Task SetTextAsync(string text, CancellationToken cancellationToken = default)
    {
        var package = new DataPackage();
        package.SetText(text);
        Clipboard.SetContent(package);
        return Task.CompletedTask;
    }
}
