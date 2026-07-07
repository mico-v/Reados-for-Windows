using ReadOS.Msp.Models;
using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Hosting.Runtime;

public sealed class MspRuntimeCommandHost : IMspCommandHost
{
    private readonly MspRuntime runtime;

    public MspRuntimeCommandHost(MspRuntime runtime)
    {
        this.runtime = runtime ?? throw new ArgumentNullException(nameof(runtime));
    }

    public ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandRequest request,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(request);

        return runtime.ExecuteAsync(request, cancellationToken);
    }

    public IAsyncEnumerable<MspCommandEvent> ExecuteStreamingAsync(
        MspCommandRequest request,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(request);

        return runtime.ExecuteStreamingAsync(request, cancellationToken);
    }
}
