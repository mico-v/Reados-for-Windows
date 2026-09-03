using ReadOS.Msp.Hosting.Native;
using ReadOS.Msp.Hosting.Native.RuntimeFfi;
using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Runtime;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsNativeCoreRegistryFactory
{
    private static readonly string[] NativeCommandNames = ["pwd", "echo", "ls", "cat"];

    private readonly IMspNativeAdapterProvider nativeAdapterProvider;
    private readonly MspCommandRuntimeFfiEchoCommandAdapter? runtimeFfiEchoCommandAdapter;

    public ReadOsNativeCoreRegistryFactory(
        IMspNativeAdapterProvider nativeAdapterProvider,
        MspCommandRuntimeFfiEchoCommandAdapter? runtimeFfiEchoCommandAdapter = null)
    {
        this.nativeAdapterProvider = nativeAdapterProvider ??
            throw new ArgumentNullException(nameof(nativeAdapterProvider));
        this.runtimeFfiEchoCommandAdapter = runtimeFfiEchoCommandAdapter;
    }

    public MspCommandRegistry Create()
    {
        var registry = MspRuntime.CreateDefaultRegistry();
        foreach (var commandName in NativeCommandNames)
        {
            if (!registry.TryGet(commandName, out var managedCommand))
            {
                throw new InvalidOperationException(
                    $"The managed MSP registry is missing the required {commandName} command.");
            }

            if (commandName == "echo" && runtimeFfiEchoCommandAdapter is not null)
            {
                // The optional adapter is the complete canonical echo definition.
                // Do not wrap it in the legacy native route; MspRuntime still owns
                // parsing, policy, terminal result, and exactly-once audit.
                registry.Register(runtimeFfiEchoCommandAdapter);
                continue;
            }

            registry.Register(new MspNativeBackedCommand(
                managedCommand,
                nativeAdapterProvider));
        }

        return registry;
    }

    public static void ValidateHostCommandPack(MspCommandPack hostCommandPack)
    {
        ArgumentNullException.ThrowIfNull(hostCommandPack);

        var protectedOverride = hostCommandPack.CommandNames.FirstOrDefault(name =>
            NativeCommandNames.Contains(name, StringComparer.OrdinalIgnoreCase));
        if (protectedOverride is not null)
        {
            throw new InvalidOperationException(
                $"The ReadOS host command pack cannot override native-owned command '{protectedOverride}'.");
        }
    }
}
