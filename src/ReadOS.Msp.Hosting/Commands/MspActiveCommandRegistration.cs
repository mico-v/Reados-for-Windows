namespace ReadOS.Msp.Hosting.Commands;

public sealed class MspActiveCommandRegistration : IDisposable
{
    private readonly MspActiveCommandRegistry owner;
    private readonly CancellationTokenSource cancellation;
    private bool disposed;

    internal MspActiveCommandRegistration(
        MspActiveCommandRegistry owner,
        string commandId,
        CancellationTokenSource cancellation)
    {
        this.owner = owner;
        this.cancellation = cancellation;
        CommandId = commandId;
    }

    public string CommandId { get; }

    public CancellationToken Token => cancellation.Token;

    public bool IsCancellationRequested => cancellation.IsCancellationRequested;

    public void Cancel()
    {
        cancellation.Cancel();
    }

    public void Dispose()
    {
        if (disposed)
        {
            return;
        }

        disposed = true;
        owner.Complete(this);
        cancellation.Dispose();
    }
}
