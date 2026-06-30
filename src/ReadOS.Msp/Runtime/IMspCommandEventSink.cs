using ReadOS.Msp.Models;

namespace ReadOS.Msp.Runtime;

public interface IMspCommandEventSink
{
    ValueTask PublishAsync(MspCommandEvent commandEvent, CancellationToken cancellationToken = default);
}
