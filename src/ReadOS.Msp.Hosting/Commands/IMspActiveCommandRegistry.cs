namespace ReadOS.Msp.Hosting.Commands;

public interface IMspActiveCommandRegistry
{
    MspActiveCommandRegistration Begin(string commandId, CancellationToken cancellationToken);

    bool CanCancel(string? commandId, bool isRunning);

    bool Cancel(string? commandId, bool isRunning);
}
