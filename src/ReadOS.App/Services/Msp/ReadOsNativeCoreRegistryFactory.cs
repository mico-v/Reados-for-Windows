using ReadOS.Msp.Hosting.Native;
using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Runtime;

namespace ReadOS.App.Services.Msp;

internal sealed class ReadOsNativeCoreRegistryFactory
{
    private static readonly string[] NativeCommandNames = ["pwd", "echo"];

    private readonly IMspNativeAdapterProvider nativeAdapterProvider;

    public ReadOsNativeCoreRegistryFactory(IMspNativeAdapterProvider nativeAdapterProvider)
    {
        this.nativeAdapterProvider = nativeAdapterProvider ??
            throw new ArgumentNullException(nameof(nativeAdapterProvider));
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
