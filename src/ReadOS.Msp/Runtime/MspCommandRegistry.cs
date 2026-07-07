namespace ReadOS.Msp.Runtime;

public sealed class MspCommandRegistry
{
    private readonly Dictionary<string, IMspCommand> commands = new(StringComparer.OrdinalIgnoreCase);

    public IReadOnlyCollection<IMspCommand> Commands => commands.Values;

    public MspCommandRegistry Register(IMspCommand command)
    {
        ArgumentNullException.ThrowIfNull(command);
        ValidateCommandName(command.Name);

        commands[command.Name] = command;
        return this;
    }

    public bool TryGet(string? name, out IMspCommand command)
    {
        if (string.IsNullOrWhiteSpace(name))
        {
            command = null!;
            return false;
        }

        return commands.TryGetValue(name, out command!);
    }

    private static void ValidateCommandName(string? commandName)
    {
        if (string.IsNullOrWhiteSpace(commandName))
        {
            throw new ArgumentException("Command name cannot be empty.", "command");
        }

        if (!string.Equals(commandName, commandName.Trim(), StringComparison.Ordinal))
        {
            throw new ArgumentException(
                $"Command name '{commandName}' cannot contain leading or trailing whitespace.",
                "command");
        }

        if (commandName.Any(char.IsWhiteSpace))
        {
            throw new ArgumentException(
                $"Command name '{commandName}' cannot contain whitespace.",
                "command");
        }

        if (commandName.Any(character => !IsCommandNameCharacter(character)))
        {
            throw new ArgumentException(
                $"Command name '{commandName}' contains unsupported characters.",
                "command");
        }
    }

    private static bool IsCommandNameCharacter(char character)
    {
        return char.IsAsciiLetterOrDigit(character) ||
            character is '-' or '_' or '.';
    }
}
