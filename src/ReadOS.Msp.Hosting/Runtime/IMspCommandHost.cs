using ReadOS.Msp.Models;

namespace ReadOS.Msp.Hosting.Runtime;

public interface IMspCommandHost
{
    ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandRequest request,
        CancellationToken cancellationToken = default);

    IAsyncEnumerable<MspCommandEvent> ExecuteStreamingAsync(
        MspCommandRequest request,
        CancellationToken cancellationToken = default);
}
