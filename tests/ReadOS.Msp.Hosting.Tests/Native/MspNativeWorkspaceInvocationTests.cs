using ReadOS.Msp.Hosting.Native;

namespace ReadOS.Msp.Hosting.Tests.Native;

public sealed class MspNativeWorkspaceInvocationTests
{
    [Fact]
    public void Topology_freezes_backends_and_orders_mounts_for_longest_prefix_routing()
    {
        var shared = new StubWorkspace();
        var baseBackend = Backend(1, new StubWorkspace());
        var invocation = new MspNativeWorkspaceInvocation(
            baseBackend,
            [
                Mount("/media", Backend(2, shared)),
                Mount("/media/archive", Backend(3, new StubWorkspace())),
                Mount("/docs", Backend(2, shared))
            ]);

        Assert.Equal(1UL, invocation.CallbackBase!.Id);
        Assert.Equal(
            ["/media/archive", "/media", "/docs"],
            invocation.Mounts.Select(mount => mount.Path));
        Assert.True(invocation.TryGetBackend(1, out var resolvedBase));
        Assert.Same(baseBackend.Workspace, resolvedBase);
        Assert.True(invocation.TryGetBackend(2, out var resolvedShared));
        Assert.Same(shared, resolvedShared);
        Assert.False(invocation.TryGetBackend(99, out _));
        Assert.Throws<NotSupportedException>(() =>
            ((IList<MspNativeWorkspaceMount>)invocation.Mounts).Add(
                Mount("/later", Backend(9, new StubWorkspace()))));
    }

    [Theory]
    [InlineData("/")]
    [InlineData("relative")]
    [InlineData("/trailing/")]
    [InlineData("/double//separator")]
    [InlineData("/dot/./segment")]
    [InlineData("/parent/../segment")]
    [InlineData("/.msp")]
    [InlineData("/docs/.MSP/private")]
    [InlineData("/back\\slash")]
    [InlineData("/line\nbreak")]
    public void Topology_rejects_root_noncanonical_hidden_or_control_mounts(string path)
    {
        var exception = Assert.Throws<ArgumentException>(() =>
            new MspNativeWorkspaceInvocation(
                mounts: [Mount(path, Backend(1, new StubWorkspace()))]));

        Assert.DoesNotContain(path, exception.Message, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void Topology_rejects_duplicate_mounts_after_strict_validation()
    {
        Assert.Throws<ArgumentException>(() => new MspNativeWorkspaceInvocation(
            mounts:
            [
                Mount("/media", Backend(1, new StubWorkspace())),
                Mount("/media", Backend(2, new StubWorkspace()))
            ]));
    }

    [Fact]
    public void Topology_treats_mount_paths_as_case_sensitive()
    {
        var invocation = new MspNativeWorkspaceInvocation(
            mounts:
            [
                Mount("/Media", Backend(1, new StubWorkspace())),
                Mount("/media", Backend(2, new StubWorkspace()))
            ]);

        Assert.Equal(2, invocation.Mounts.Count);
    }

    [Fact]
    public void Topology_rejects_more_than_32_mounts()
    {
        var mounts = Enumerable.Range(1, MspNativeWorkspaceAbiV1.MaximumMountCount + 1)
            .Select(index => Mount($"/mount-{index}", Backend((ulong)index, new StubWorkspace())))
            .ToArray();

        Assert.Throws<ArgumentOutOfRangeException>(() =>
            new MspNativeWorkspaceInvocation(mounts: mounts));
    }

    [Fact]
    public void Topology_rejects_zero_or_conflicting_backend_identifiers()
    {
        Assert.Throws<ArgumentOutOfRangeException>(() =>
            new MspNativeWorkspaceInvocation(
                callbackBase: Backend(0, new StubWorkspace())));

        Assert.Throws<ArgumentException>(() => new MspNativeWorkspaceInvocation(
            callbackBase: Backend(7, new StubWorkspace()),
            mounts: [Mount("/media", Backend(7, new StubWorkspace()))]));
    }

    [Fact]
    public void Topology_requires_at_least_one_callback_backend()
    {
        Assert.Throws<ArgumentException>(() => new MspNativeWorkspaceInvocation());
    }

    private static MspNativeWorkspaceBackend Backend(
        ulong id,
        IMspNativeReadOnlyWorkspace workspace)
    {
        return new MspNativeWorkspaceBackend
        {
            Id = id,
            Workspace = workspace
        };
    }

    private static MspNativeWorkspaceMount Mount(
        string path,
        MspNativeWorkspaceBackend backend)
    {
        return new MspNativeWorkspaceMount
        {
            Path = path,
            Backend = backend
        };
    }

    private sealed class StubWorkspace : IMspNativeReadOnlyWorkspace
    {
        public ValueTask<MspNativeWorkspaceFileInfo> StatAsync(
            string virtualPath,
            CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public ValueTask<IReadOnlyList<MspNativeWorkspaceDirectoryEntry>> ListDirectoryAsync(
            string virtualPath,
            CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public ValueTask<ReadOnlyMemory<byte>> ReadFileRangeAsync(
            string virtualPath,
            ulong offset,
            int length,
            CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }
    }
}
