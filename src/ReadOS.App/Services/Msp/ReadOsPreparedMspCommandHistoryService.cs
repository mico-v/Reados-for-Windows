using System.Collections.ObjectModel;
using ReadOS.App.Models;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsPreparedMspCommandHistoryService
{
    private readonly int maxCommands;

    public ReadOsPreparedMspCommandHistoryService(int maxCommands)
    {
        this.maxCommands = Math.Max(1, maxCommands);
    }

    public PreparedMspCommand? Record(
        ObservableCollection<PreparedMspCommand> commands,
        string commandText,
        string statusMessage,
        DateTimeOffset createdAt)
    {
        commandText = commandText.Trim();
        if (string.IsNullOrWhiteSpace(commandText))
        {
            return null;
        }

        var existing = commands.FirstOrDefault(command =>
            string.Equals(command.CommandText, commandText, StringComparison.Ordinal));
        if (existing is not null)
        {
            commands.Remove(existing);
        }

        var preparedCommand = new PreparedMspCommand
        {
            Title = NormalizeTitle(statusMessage),
            Detail = commandText,
            CommandText = commandText,
            CreatedAt = createdAt
        };
        commands.Insert(0, preparedCommand);

        while (commands.Count > maxCommands)
        {
            commands.RemoveAt(commands.Count - 1);
        }

        return preparedCommand;
    }

    public void Promote(
        ObservableCollection<PreparedMspCommand> commands,
        PreparedMspCommand command)
    {
        var index = commands.IndexOf(command);
        if (index <= 0)
        {
            return;
        }

        commands.RemoveAt(index);
        commands.Insert(0, command);
    }

    public string NormalizeTitle(string statusMessage)
    {
        var title = statusMessage.Trim();
        return title.Length == 0
            ? "Prepared MSP command"
            : title.TrimEnd('。', '.', '!');
    }
}
