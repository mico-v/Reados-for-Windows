namespace ReadOS.Msp.Runtime;

public sealed class MspCommandRegistry
{
    private readonly Dictionary<string, IMspCommand> commands = new(StringComparer.OrdinalIgnoreCase);

    public IReadOnlyCollection<IMspCommand> Commands => commands.Values;

    public MspCommandRegistry Register(IMspCommand command)
    {
        commands[command.Name] = command;
        return this;
    }

    public bool TryGet(string name, out IMspCommand command)
    {
        return commands.TryGetValue(name, out command!);
    }
}
