using ReadOS.Msp.Models;

namespace ReadOS.Msp.Audit;

public interface IMspAuditSink
{
    ValueTask RecordAsync(MspAuditRecord record, CancellationToken cancellationToken = default);
}
