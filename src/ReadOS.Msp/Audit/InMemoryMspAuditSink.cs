using ReadOS.Msp.Models;

namespace ReadOS.Msp.Audit;

public sealed class InMemoryMspAuditSink : IMspAuditSink
{
    private readonly List<MspAuditRecord> records = new();

    public IReadOnlyList<MspAuditRecord> Records => records;

    public ValueTask RecordAsync(MspAuditRecord record, CancellationToken cancellationToken = default)
    {
        records.Add(record);
        return ValueTask.CompletedTask;
    }
}
