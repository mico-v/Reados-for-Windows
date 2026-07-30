namespace ReadOS.Msp.Hosting.Native;

/// <summary>
/// Owns one shared native adapter for a command-host lifetime.
/// Dispose only after all commands using the provider have drained.
/// </summary>
public interface IMspNativeAdapterProvider : IDisposable
{
    bool IsAdapterCreated { get; }

    IMspNativeAdapter GetRequiredAdapter();
}

public sealed class LazyMspNativeAdapterProvider : IMspNativeAdapterProvider
{
    private readonly object gate = new();
    private readonly Lazy<IMspNativeAdapter> adapter;
    private bool disposed;

    public LazyMspNativeAdapterProvider(Func<IMspNativeAdapter> adapterFactory)
    {
        ArgumentNullException.ThrowIfNull(adapterFactory);

        adapter = new Lazy<IMspNativeAdapter>(
            () => CreateAdapter(adapterFactory),
            LazyThreadSafetyMode.ExecutionAndPublication);
    }

    public bool IsAdapterCreated => adapter.IsValueCreated;

    public IMspNativeAdapter GetRequiredAdapter()
    {
        lock (gate)
        {
            ObjectDisposedException.ThrowIf(disposed, this);
            return adapter.Value;
        }
    }

    public void Dispose()
    {
        lock (gate)
        {
            if (disposed)
            {
                return;
            }

            disposed = true;
            if (adapter.IsValueCreated)
            {
                adapter.Value.Dispose();
            }
        }
    }

    private static IMspNativeAdapter CreateAdapter(Func<IMspNativeAdapter> adapterFactory)
    {
        try
        {
            return adapterFactory()
                ?? throw MspNativeAdapterException.Create(
                    MspNativeFailureKind.LibraryUnavailable,
                    null);
        }
        catch (MspNativeAdapterException)
        {
            throw;
        }
        catch (Exception exception) when (
            exception is not OperationCanceledException && !IsFatal(exception))
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.LibraryUnavailable,
                null);
        }
    }

    private static bool IsFatal(Exception exception)
    {
        return exception is OutOfMemoryException or StackOverflowException;
    }
}
