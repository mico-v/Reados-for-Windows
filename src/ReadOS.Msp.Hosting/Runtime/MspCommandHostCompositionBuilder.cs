using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Hosting.Runtime;

public sealed class MspCommandHostCompositionBuilder
{
    private readonly Func<MspCommandRegistry> coreRegistryFactory;

    public MspCommandHostCompositionBuilder()
        : this(MspRuntime.CreateDefaultRegistry)
    {
    }

    public MspCommandHostCompositionBuilder(Func<MspCommandRegistry> coreRegistryFactory)
    {
        ArgumentNullException.ThrowIfNull(coreRegistryFactory);

        this.coreRegistryFactory = coreRegistryFactory;
    }

    public MspCommandHostComposition Build(IEnumerable<IMspCommand> hostCommands)
    {
        return Build(new MspCommandPack("Host commands", hostCommands));
    }

    public MspCommandHostComposition Build(MspCommandPack hostCommandPack)
    {
        ArgumentNullException.ThrowIfNull(hostCommandPack);

        var registry = coreRegistryFactory();
        if (registry is null)
        {
            throw new InvalidOperationException("Core registry factory returned null.");
        }

        ValidateCoreRegistryCommands(registry.Commands);

        var coreCommandNames = registry.Commands
            .Select(command => command.Name)
            .Order(StringComparer.OrdinalIgnoreCase)
            .ToArray();
        var coreCommandNameSet = coreCommandNames.ToHashSet(StringComparer.OrdinalIgnoreCase);
        var hostCommandNames = new List<string>();

        foreach (var command in hostCommandPack.Commands)
        {
            registry.Register(command);
            hostCommandNames.Add(command.Name);
        }

        return new MspCommandHostComposition
        {
            Registry = registry,
            HostCommandPackName = hostCommandPack.Name,
            CoreCommandNames = coreCommandNames,
            HostCommandNames = hostCommandNames.ToArray(),
            OverriddenCoreCommandNames = hostCommandPack.CommandNames
                .Where(coreCommandNameSet.Contains)
                .Distinct(StringComparer.OrdinalIgnoreCase)
                .Order(StringComparer.OrdinalIgnoreCase)
                .ToArray(),
            CommandNames = registry.Commands
                .Select(command => command.Name)
                .Order(StringComparer.OrdinalIgnoreCase)
                .ToArray()
        };
    }

    private static void ValidateCoreRegistryCommands(IReadOnlyCollection<IMspCommand> commands)
    {
        foreach (var command in commands)
        {
            if (command is null)
            {
                throw new InvalidOperationException("Core registry contains a null command.");
            }

            var validationError = MspCommandNameValidation.GetError(command.Name);
            switch (validationError)
            {
                case MspCommandNameValidationError.Empty:
                    throw new InvalidOperationException("Core registry contains a command with an empty name.");
                case MspCommandNameValidationError.LeadingOrTrailingWhitespace:
                    throw new InvalidOperationException(
                        $"Core registry contains command name '{command.Name}' with leading or trailing whitespace.");
                case MspCommandNameValidationError.Whitespace:
                    throw new InvalidOperationException(
                        $"Core registry contains command name '{command.Name}' with whitespace.");
                case MspCommandNameValidationError.UnsupportedCharacters:
                    throw new InvalidOperationException(
                        $"Core registry contains command name '{command.Name}' with unsupported characters.");
            }
        }
    }
}
