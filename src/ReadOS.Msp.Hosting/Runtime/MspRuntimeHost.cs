using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Hosting.Runtime;

public sealed class MspRuntimeHost
{
    public MspRuntimeHost(MspCommandContext context, MspRuntime runtime)
        : this(context, runtime, new MspRuntimeCommandHost(runtime))
    {
    }

    public MspRuntimeHost(MspCommandContext context, MspRuntime runtime, IMspCommandHost commandHost)
    {
        Context = context ?? throw new ArgumentNullException(nameof(context));
        Runtime = runtime ?? throw new ArgumentNullException(nameof(runtime));
        CommandHost = commandHost ?? throw new ArgumentNullException(nameof(commandHost));
    }

    public MspCommandContext Context { get; }

    public MspRuntime Runtime { get; }

    public IMspCommandHost CommandHost { get; }
}
