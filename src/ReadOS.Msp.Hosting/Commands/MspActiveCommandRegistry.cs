namespace ReadOS.Msp.Hosting.Commands;

public sealed class MspActiveCommandRegistry : IMspActiveCommandRegistry
{
    private MspActiveCommandRegistration? activeCommand;

    public MspActiveCommandRegistration Begin(string commandId, CancellationToken cancellationToken)
    {
        var registration = new MspActiveCommandRegistration(
            this,
            commandId,
            CancellationTokenSource.CreateLinkedTokenSource(cancellationToken));
        activeCommand = registration;
        return registration;
    }

    public bool CanCancel(string? commandId, bool isRunning)
    {
        return isRunning &&
            !string.IsNullOrWhiteSpace(commandId) &&
            activeCommand is not null &&
            string.Equals(activeCommand.CommandId, commandId, StringComparison.Ordinal);
    }

    public bool Cancel(string? commandId, bool isRunning)
    {
        if (!CanCancel(commandId, isRunning))
        {
            return false;
        }

        activeCommand?.Cancel();
        return true;
    }

    internal void Complete(MspActiveCommandRegistration registration)
    {
        if (ReferenceEquals(activeCommand, registration))
        {
            activeCommand = null;
        }
    }
}
