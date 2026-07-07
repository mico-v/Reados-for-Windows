using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Hosting.Runtime;

public sealed class MspCommandPack
{
    public MspCommandPack(string name, IEnumerable<IMspCommand> commands)
    {
        if (string.IsNullOrWhiteSpace(name))
        {
            throw new ArgumentException("Command pack name must not be empty.", nameof(name));
        }

        ArgumentNullException.ThrowIfNull(commands);

        Name = name.Trim();
        Commands = commands.ToArray();
        ValidateCommands(Commands);
        CommandNames = Commands.Select(command => command.Name).ToArray();
    }

    public string Name { get; }

    public IReadOnlyList<IMspCommand> Commands { get; }

    public IReadOnlyList<string> CommandNames { get; }

    private static void ValidateCommands(IReadOnlyList<IMspCommand> commands)
    {
        var commandNames = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

        for (var index = 0; index < commands.Count; index++)
        {
            var command = commands[index];
            if (command is null)
            {
                throw new ArgumentException($"Command pack contains a null command at index {index}.", nameof(commands));
            }

            ThrowIfInvalidCommandName(command.Name, index);

            if (!commandNames.Add(command.Name))
            {
                throw new ArgumentException($"Command pack contains duplicate command name '{command.Name}'.", nameof(commands));
            }
        }
    }

    private static void ThrowIfInvalidCommandName(string commandName, int index)
    {
        var validationError = MspCommandNameValidation.GetError(commandName);
        switch (validationError)
        {
            case MspCommandNameValidationError.Empty:
                throw new ArgumentException($"Command pack contains a command with an empty name at index {index}.", "commands");
            case MspCommandNameValidationError.LeadingOrTrailingWhitespace:
                throw new ArgumentException(
                    $"Command pack contains command name '{commandName}' with leading or trailing whitespace.",
                    "commands");
            case MspCommandNameValidationError.Whitespace:
                throw new ArgumentException(
                    $"Command pack contains command name '{commandName}' with whitespace.",
                    "commands");
            case MspCommandNameValidationError.UnsupportedCharacters:
                throw new ArgumentException(
                    $"Command pack contains command name '{commandName}' with unsupported characters.",
                    "commands");
        }
    }
}
