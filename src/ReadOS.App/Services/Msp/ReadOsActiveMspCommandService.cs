using ReadOS.App.Models;
using ReadOS.Msp.Hosting.Commands;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsActiveMspCommandService
{
    private readonly IMspActiveCommandRegistry activeCommands;

    public ReadOsActiveMspCommandService()
        : this(new MspActiveCommandRegistry())
    {
    }

    public ReadOsActiveMspCommandService(IMspActiveCommandRegistry activeCommands)
    {
        this.activeCommands = activeCommands;
    }

    public MspActiveCommandRegistration Begin(string entryId, CancellationToken cancellationToken)
    {
        return activeCommands.Begin(entryId, cancellationToken);
    }

    public bool CanCancel(MspTranscriptEntry? entry)
    {
        return entry is not null &&
            activeCommands.CanCancel(entry.Id, entry.IsRunning);
    }

    public bool Cancel(MspTranscriptEntry? entry)
    {
        return entry is not null &&
            activeCommands.Cancel(entry.Id, entry.IsRunning);
    }
}
