using ReadOS.Msp.Hosting.Native;

namespace ReadOS.Msp.Hosting.Tests.Native;

public sealed class LazyMspNativeAdapterProviderTests
{
    [Fact]
    public void Provider_creates_adapter_lazily_once_and_disposes_created_adapter()
    {
        var factoryCalls = 0;
        var adapter = new StubAdapter();
        var provider = new LazyMspNativeAdapterProvider(() =>
        {
            factoryCalls++;
            return adapter;
        });

        Assert.False(provider.IsAdapterCreated);
        Assert.Equal(0, factoryCalls);
        Assert.Same(adapter, provider.GetRequiredAdapter());
        Assert.Same(adapter, provider.GetRequiredAdapter());
        Assert.True(provider.IsAdapterCreated);
        Assert.Equal(1, factoryCalls);

        provider.Dispose();

        Assert.True(adapter.IsDisposed);
        Assert.Throws<ObjectDisposedException>(() => provider.GetRequiredAdapter());
    }

    [Fact]
    public void Dispose_does_not_create_unused_adapter()
    {
        var factoryCalls = 0;
        var provider = new LazyMspNativeAdapterProvider(() =>
        {
            factoryCalls++;
            return new StubAdapter();
        });

        provider.Dispose();

        Assert.Equal(0, factoryCalls);
        Assert.False(provider.IsAdapterCreated);
    }

    [Fact]
    public void Provider_sanitizes_and_caches_factory_failure()
    {
        const string secret = @"V:\private\native-secret";
        var factoryCalls = 0;
        using var provider = new LazyMspNativeAdapterProvider(() =>
        {
            factoryCalls++;
            throw new InvalidOperationException(secret);
        });

        var first = Assert.Throws<MspNativeAdapterException>(() =>
            provider.GetRequiredAdapter());
        var second = Assert.Throws<MspNativeAdapterException>(() =>
            provider.GetRequiredAdapter());

        Assert.Equal(MspNativeFailureKind.LibraryUnavailable, first.FailureKind);
        Assert.Same(first, second);
        Assert.Equal(1, factoryCalls);
        Assert.DoesNotContain(secret, first.ToString());
        Assert.Null(first.InnerException);
    }

    [Fact]
    public void Provider_rejects_null_factory_result_as_library_unavailable()
    {
        using var provider = new LazyMspNativeAdapterProvider(() => null!);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            provider.GetRequiredAdapter());

        Assert.Equal(MspNativeFailureKind.LibraryUnavailable, exception.FailureKind);
    }

    private sealed class StubAdapter : IMspNativeAdapter
    {
        public bool IsDisposed { get; private set; }

        public MspNativeCommandResult Execute(MspNativeCommandRequest request)
        {
            throw new NotSupportedException();
        }

        public MspNativeShellParseResult Parse(MspNativeShellParseRequest request)
        {
            throw new NotSupportedException();
        }

        public MspNativeWorkspacePathResult NormalizeWorkspacePath(
            MspNativeWorkspacePathRequest request)
        {
            throw new NotSupportedException();
        }

        public void Dispose()
        {
            IsDisposed = true;
        }
    }
}
