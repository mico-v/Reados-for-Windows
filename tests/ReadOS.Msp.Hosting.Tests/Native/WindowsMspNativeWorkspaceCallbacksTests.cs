using System.Runtime.InteropServices;
using System.Text;
using ReadOS.Msp.Hosting.Native;

namespace ReadOS.Msp.Hosting.Tests.Native;

public sealed class WindowsMspNativeWorkspaceCallbacksTests
{
    [Fact]
    public void Host_publishes_frozen_version_capabilities_context_and_rooted_callbacks()
    {
        using var harness = CreateHarness(new TestWorkspace());

        var host = harness.Host;
        Assert.Equal((uint)MspNativeWorkspaceAbiV1.HostSize, host.Size);
        Assert.Equal(1U, host.MajorVersion);
        Assert.Equal(0U, host.MinorVersion);
        Assert.Equal(0U, host.Reserved);
        Assert.Equal(0xFUL, host.Capabilities);
        Assert.NotEqual(nint.Zero, host.Context);
        Assert.NotEqual(nint.Zero, host.Invoke);
        Assert.NotEqual(nint.Zero, host.Free);
        Assert.NotEqual(nint.Zero, host.IsCancelled);
        Assert.Equal(0UL, host.Reserved2);

        GC.Collect();
        GC.WaitForPendingFinalizers();
        GC.Collect();

        var result = harness.Invoke(MspNativeWorkspaceOperationV1.Stat, "/");
        Assert.Equal(MspNativeWorkspaceCallbackStatusV1.Ok, result.Status);
        harness.Free(result);
    }

    [Fact]
    public void Stat_returns_strict_camel_case_json_without_any_path_field()
    {
        var workspace = new TestWorkspace
        {
            StatHandler = (_, _) => ValueTask.FromResult(new MspNativeWorkspaceFileInfo
            {
                FileType = MspNativeWorkspaceFileType.RegularFile,
                SizeBytes = 4,
                ModificationTimeUnixMs = 123,
                FileIdentity = "volume:0001"
            })
        };
        using var harness = CreateHarness(workspace);

        var result = harness.Invoke(MspNativeWorkspaceOperationV1.Stat, "/secret.txt");

        Assert.Equal(MspNativeWorkspaceCallbackStatusV1.Ok, result.Status);
        Assert.Equal(
            "{\"fileType\":\"regularFile\",\"sizeBytes\":4," +
            "\"modificationTimeUnixMs\":123,\"fileIdentity\":\"volume:0001\"}",
            result.CopyUtf8());
        Assert.DoesNotContain("secret", result.CopyUtf8(), StringComparison.OrdinalIgnoreCase);
        Assert.Equal(1, harness.Owner.OutstandingAllocationCount);

        harness.Free(result);
        Assert.Equal(0, harness.Owner.OutstandingAllocationCount);
    }

    [Fact]
    public void List_returns_frozen_entry_metadata_without_backend_paths()
    {
        var workspace = new TestWorkspace
        {
            ListHandler = (_, _) => ValueTask.FromResult<IReadOnlyList<MspNativeWorkspaceDirectoryEntry>>(
            [
                new MspNativeWorkspaceDirectoryEntry
                {
                    Name = "资料",
                    Info = new MspNativeWorkspaceFileInfo
                    {
                        FileType = MspNativeWorkspaceFileType.Directory
                    }
                },
                new MspNativeWorkspaceDirectoryEntry
                {
                    Name = "a.txt",
                    Info = new MspNativeWorkspaceFileInfo
                    {
                        FileType = MspNativeWorkspaceFileType.RegularFile,
                        SizeBytes = 2
                    }
                }
            ])
        };
        using var harness = CreateHarness(workspace);

        var result = harness.Invoke(MspNativeWorkspaceOperationV1.ListDirectory, "/mounted");
        var json = result.CopyUtf8();

        Assert.Equal(MspNativeWorkspaceCallbackStatusV1.Ok, result.Status);
        Assert.Equal(
            "[{\"name\":\"\\u8D44\\u6599\",\"info\":{\"fileType\":\"directory\"," +
            "\"sizeBytes\":null,\"modificationTimeUnixMs\":null,\"fileIdentity\":null}}," +
            "{\"name\":\"a.txt\",\"info\":{\"fileType\":\"regularFile\"," +
            "\"sizeBytes\":2,\"modificationTimeUnixMs\":null,\"fileIdentity\":null}}]",
            json);
        Assert.DoesNotContain("mounted", json, StringComparison.OrdinalIgnoreCase);
        harness.Free(result);
    }

    [Fact]
    public void Read_range_returns_authoritative_raw_bytes_including_nul_and_non_utf8()
    {
        var expected = new byte[] { 0x00, 0xff, (byte)'A', (byte)'\n' };
        var workspace = new TestWorkspace
        {
            ReadHandler = (_, offset, length, _) =>
            {
                Assert.Equal(7UL, offset);
                Assert.Equal(4, length);
                return ValueTask.FromResult<ReadOnlyMemory<byte>>(expected);
            }
        };
        using var harness = CreateHarness(workspace);

        var result = harness.Invoke(
            MspNativeWorkspaceOperationV1.ReadFileRange,
            "/binary.bin",
            offset: 7,
            length: 4);

        Assert.Equal(MspNativeWorkspaceCallbackStatusV1.Ok, result.Status);
        Assert.Equal(expected, result.CopyBytes());
        harness.Free(result);
    }

    [Fact]
    public void Managed_operations_run_on_a_worker_without_a_synchronization_context()
    {
        SynchronizationContext? observed = new SynchronizationContext();
        var workspace = new TestWorkspace
        {
            StatHandler = (_, _) =>
            {
                observed = SynchronizationContext.Current;
                return ValueTask.FromResult(DirectoryInfo());
            }
        };
        using var harness = CreateHarness(workspace);
        var previous = SynchronizationContext.Current;
        SynchronizationContext.SetSynchronizationContext(new SynchronizationContext());
        try
        {
            var result = harness.Invoke(MspNativeWorkspaceOperationV1.Stat, "/");
            Assert.Equal(MspNativeWorkspaceCallbackStatusV1.Ok, result.Status);
            harness.Free(result);
        }
        finally
        {
            SynchronizationContext.SetSynchronizationContext(previous);
        }

        Assert.Null(observed);
    }

    [Theory]
    [InlineData(MspNativeWorkspaceErrorKind.NotFound, 2)]
    [InlineData(MspNativeWorkspaceErrorKind.NotDirectory, 3)]
    [InlineData(MspNativeWorkspaceErrorKind.IsDirectory, 4)]
    [InlineData(MspNativeWorkspaceErrorKind.AccessDenied, 5)]
    [InlineData(MspNativeWorkspaceErrorKind.HiddenPath, 6)]
    [InlineData(MspNativeWorkspaceErrorKind.InvalidPath, 7)]
    [InlineData(MspNativeWorkspaceErrorKind.LimitExceeded, 8)]
    [InlineData(MspNativeWorkspaceErrorKind.Unsupported, 9)]
    [InlineData(MspNativeWorkspaceErrorKind.Io, 11)]
    public void Closed_workspace_errors_map_without_messages_or_payloads(
        MspNativeWorkspaceErrorKind error,
        int expected)
    {
        var workspace = new TestWorkspace
        {
            StatHandler = (_, _) => ValueTask.FromException<MspNativeWorkspaceFileInfo>(
                new MspNativeWorkspaceException(error))
        };
        using var harness = CreateHarness(workspace);

        var result = harness.Invoke(MspNativeWorkspaceOperationV1.Stat, "/file");

        Assert.Equal((MspNativeWorkspaceCallbackStatusV1)expected, result.Status);
        Assert.Equal(nint.Zero, result.Pointer);
        Assert.Equal(0UL, result.Length);
    }

    [Fact]
    public void Arbitrary_managed_exception_is_sanitized_to_io_without_host_path_disclosure()
    {
        var secret = @"C:\private\workspace\secret.txt";
        var workspace = new TestWorkspace
        {
            StatHandler = (_, _) => ValueTask.FromException<MspNativeWorkspaceFileInfo>(
                new InvalidOperationException(secret))
        };
        using var harness = CreateHarness(workspace);

        var result = harness.Invoke(MspNativeWorkspaceOperationV1.Stat, "/file");

        Assert.Equal(MspNativeWorkspaceCallbackStatusV1.Io, result.Status);
        Assert.Equal(nint.Zero, result.Pointer);
        Assert.Equal(0UL, result.Length);
        Assert.DoesNotContain(secret, result.ToString(), StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public async Task Cancellation_is_observable_before_and_during_a_workspace_operation()
    {
        using var beforeCancellation = new CancellationTokenSource();
        beforeCancellation.Cancel();
        var neverCalled = new TestWorkspace
        {
            StatHandler = (_, _) => throw new InvalidOperationException("must not be called")
        };
        using (var beforeHarness = CreateHarness(neverCalled, beforeCancellation.Token))
        {
            Assert.Equal(1, beforeHarness.IsCancelled());
            var before = beforeHarness.Invoke(MspNativeWorkspaceOperationV1.Stat, "/");
            Assert.Equal(MspNativeWorkspaceCallbackStatusV1.Canceled, before.Status);
        }

        using var duringCancellation = new CancellationTokenSource();
        var entered = new ManualResetEventSlim();
        var blocking = new TestWorkspace
        {
            StatHandler = async (_, cancellationToken) =>
            {
                entered.Set();
                await Task.Delay(Timeout.InfiniteTimeSpan, cancellationToken);
                return DirectoryInfo();
            }
        };
        using var duringHarness = CreateHarness(blocking, duringCancellation.Token);
        var invocation = Task.Run(() =>
            duringHarness.Invoke(MspNativeWorkspaceOperationV1.Stat, "/"));
        Assert.True(entered.Wait(TimeSpan.FromSeconds(10)));

        duringCancellation.Cancel();
        var result = await invocation;

        Assert.Equal(MspNativeWorkspaceCallbackStatusV1.Canceled, result.Status);
        Assert.Equal(1, duringHarness.IsCancelled());
    }

    [Fact]
    public void Concurrent_or_reentrant_callback_poisoning_fails_the_outer_call_closed()
    {
        CallbackHarness? harness = null;
        var nestedStatus = MspNativeWorkspaceCallbackStatusV1.Ok;
        var workspace = new TestWorkspace();
        workspace.StatHandler = (_, _) =>
        {
            var nested = harness!.Invoke(MspNativeWorkspaceOperationV1.Stat, "/nested");
            nestedStatus = nested.Status;
            return ValueTask.FromResult(DirectoryInfo());
        };
        using (harness = CreateHarness(workspace))
        {
            var outer = harness.Invoke(MspNativeWorkspaceOperationV1.Stat, "/");

            Assert.Equal(MspNativeWorkspaceCallbackStatusV1.Io, nestedStatus);
            Assert.Equal(MspNativeWorkspaceCallbackStatusV1.Io, outer.Status);
            Assert.Equal(nint.Zero, outer.Pointer);
            Assert.True(harness.Owner.IsPoisoned);
        }
    }

    [Fact]
    public void Exact_free_zeroes_and_releases_once_while_duplicate_free_only_poisons()
    {
        var memory = new RecordingMemory();
        using var harness = CreateHarness(new TestWorkspace(), memory: memory);
        var result = harness.Invoke(
            MspNativeWorkspaceOperationV1.ReadFileRange,
            "/file",
            length: 4);

        harness.Free(result);

        Assert.Equal(1, memory.FreeCount);
        Assert.Equal(new byte[] { 1, 2, 3, 4 }, memory.LastBytesBeforeZero);
        Assert.All(memory.LastBytesAfterZero, value => Assert.Equal(0, value));
        Assert.False(harness.Owner.IsPoisoned);

        harness.Free(result);

        Assert.Equal(1, memory.FreeCount);
        Assert.True(harness.Owner.IsPoisoned);
    }

    [Fact]
    public void Mismatched_free_tuple_is_not_freed_and_dispose_cleans_the_allocation()
    {
        var memory = new RecordingMemory();
        var harness = CreateHarness(new TestWorkspace(), memory: memory);
        var result = harness.Invoke(
            MspNativeWorkspaceOperationV1.ReadFileRange,
            "/file",
            length: 4);

        harness.Free(result with { Length = result.Length + 1 });

        Assert.Equal(0, memory.FreeCount);
        Assert.Equal(1, harness.Owner.OutstandingAllocationCount);
        Assert.True(harness.Owner.IsPoisoned);

        harness.Dispose();

        Assert.Equal(1, memory.FreeCount);
        Assert.All(memory.LastBytesAfterZero, value => Assert.Equal(0, value));
    }

    [Fact]
    public void Null_free_is_a_safe_noop_for_any_length()
    {
        using var harness = CreateHarness(new TestWorkspace());

        harness.Free(new CallbackResult(
            MspNativeWorkspaceCallbackStatusV1.Ok,
            nint.Zero,
            ulong.MaxValue));

        Assert.False(harness.Owner.IsPoisoned);
    }

    [Fact]
    public async Task Dispose_waits_for_an_inflight_callback_before_releasing_context()
    {
        var entered = new ManualResetEventSlim();
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var workspace = new TestWorkspace
        {
            StatHandler = async (_, _) =>
            {
                entered.Set();
                await release.Task;
                return DirectoryInfo();
            }
        };
        var harness = CreateHarness(workspace);
        var invocation = Task.Run(() =>
            harness.Invoke(MspNativeWorkspaceOperationV1.Stat, "/"));
        Assert.True(entered.Wait(TimeSpan.FromSeconds(10)));

        var disposal = Task.Run(harness.Dispose);
        await Task.Delay(100);
        Assert.False(disposal.IsCompleted);

        release.SetResult();
        var result = await invocation;
        await disposal;

        Assert.True(result.Status is
            MspNativeWorkspaceCallbackStatusV1.Io or
            MspNativeWorkspaceCallbackStatusV1.InvalidArgument);
    }

    [Fact]
    public void Callback_flow_cannot_dispose_its_own_context()
    {
        CallbackHarness? harness = null;
        var workspace = new TestWorkspace
        {
            StatHandler = (_, _) =>
            {
                harness!.Dispose();
                return ValueTask.FromResult(DirectoryInfo());
            }
        };
        harness = CreateHarness(workspace);
        try
        {
            var result = harness.Invoke(MspNativeWorkspaceOperationV1.Stat, "/");

            Assert.Equal(MspNativeWorkspaceCallbackStatusV1.Io, result.Status);
        }
        finally
        {
            harness.Dispose();
        }
    }

    [Fact]
    public void Invalid_request_fields_paths_and_ranges_fail_before_backend_access()
    {
        var calls = 0;
        var workspace = new TestWorkspace
        {
            StatHandler = (_, _) =>
            {
                calls++;
                return ValueTask.FromResult(DirectoryInfo());
            }
        };
        using var harness = CreateHarness(workspace);

        Assert.Equal(
            MspNativeWorkspaceCallbackStatusV1.InvalidArgument,
            harness.Invoke(
                MspNativeWorkspaceOperationV1.Stat,
                "/",
                mutate: request => request.WithSize(12)).Status);
        Assert.Equal(
            MspNativeWorkspaceCallbackStatusV1.InvalidArgument,
            harness.Invoke(
                MspNativeWorkspaceOperationV1.Stat,
                "/",
                mutate: request => request.WithReserved(1)).Status);
        Assert.Equal(
            MspNativeWorkspaceCallbackStatusV1.InvalidArgument,
            harness.Invoke(
                MspNativeWorkspaceOperationV1.Stat,
                "/",
                backendId: 0).Status);
        Assert.Equal(
            MspNativeWorkspaceCallbackStatusV1.InvalidPath,
            harness.InvokeRawPath(
                MspNativeWorkspaceOperationV1.Stat,
                [0xff]).Status);
        Assert.Equal(
            MspNativeWorkspaceCallbackStatusV1.InvalidPath,
            harness.Invoke(
                MspNativeWorkspaceOperationV1.Stat,
                "/not/../canonical").Status);
        Assert.Equal(
            MspNativeWorkspaceCallbackStatusV1.HiddenPath,
            harness.Invoke(
                MspNativeWorkspaceOperationV1.Stat,
                "/.MSP/private").Status);
        Assert.Equal(
            MspNativeWorkspaceCallbackStatusV1.LimitExceeded,
            harness.Invoke(
                MspNativeWorkspaceOperationV1.ReadFileRange,
                "/file",
                length: MspNativeWorkspaceAbiV1.MaximumReadRangeBytes + 1).Status);
        Assert.Equal(0, calls);
    }

    [Fact]
    public void Invalid_backend_listing_and_oversized_range_fail_without_allocating()
    {
        var workspace = new TestWorkspace
        {
            ListHandler = (_, _) => ValueTask.FromResult<IReadOnlyList<MspNativeWorkspaceDirectoryEntry>>(
            [
                Entry("duplicate"),
                Entry("duplicate")
            ]),
            ReadHandler = (_, _, length, _) =>
                ValueTask.FromResult<ReadOnlyMemory<byte>>(new byte[length + 1])
        };
        using var harness = CreateHarness(workspace);

        var listing = harness.Invoke(MspNativeWorkspaceOperationV1.ListDirectory, "/");
        var range = harness.Invoke(
            MspNativeWorkspaceOperationV1.ReadFileRange,
            "/file",
            length: 4);

        Assert.Equal(MspNativeWorkspaceCallbackStatusV1.Io, listing.Status);
        Assert.Equal(MspNativeWorkspaceCallbackStatusV1.Io, range.Status);
        Assert.Equal(0, harness.Owner.OutstandingAllocationCount);
    }

    [Fact]
    public void Directory_json_limit_is_rejected_before_allocating_an_unbounded_payload()
    {
        var entries = Enumerable.Range(0, 500)
            .Select(index => Entry($"{index:D4}-{new string('x', 3500)}"))
            .ToArray();
        var workspace = new TestWorkspace
        {
            ListHandler = (_, _) =>
                ValueTask.FromResult<IReadOnlyList<MspNativeWorkspaceDirectoryEntry>>(entries)
        };
        var memory = new RecordingMemory();
        using var harness = CreateHarness(workspace, memory: memory);

        var result = harness.Invoke(MspNativeWorkspaceOperationV1.ListDirectory, "/");

        Assert.Equal(MspNativeWorkspaceCallbackStatusV1.LimitExceeded, result.Status);
        Assert.Equal(0, memory.AllocationCount);
        Assert.Equal(0, harness.Owner.OutstandingAllocationCount);
    }

    private static CallbackHarness CreateHarness(
        TestWorkspace workspace,
        CancellationToken cancellationToken = default,
        RecordingMemory? memory = null)
    {
        var invocation = new MspNativeWorkspaceInvocation(
            callbackBase: new MspNativeWorkspaceBackend
            {
                Id = 1,
                Workspace = workspace
            });
        return new CallbackHarness(invocation, cancellationToken, memory ?? new RecordingMemory());
    }

    private static MspNativeWorkspaceFileInfo DirectoryInfo()
    {
        return new MspNativeWorkspaceFileInfo
        {
            FileType = MspNativeWorkspaceFileType.Directory
        };
    }

    private static MspNativeWorkspaceDirectoryEntry Entry(string name)
    {
        return new MspNativeWorkspaceDirectoryEntry
        {
            Name = name,
            Info = new MspNativeWorkspaceFileInfo
            {
                FileType = MspNativeWorkspaceFileType.RegularFile
            }
        };
    }

    private sealed class CallbackHarness : IDisposable
    {
        private readonly MspNativeWorkspaceInvokeV1Function invoke;
        private readonly MspNativeWorkspaceFreeV1Function free;
        private readonly MspNativeWorkspaceIsCancelledV1Function isCancelled;

        public CallbackHarness(
            MspNativeWorkspaceInvocation invocation,
            CancellationToken cancellationToken,
            IMspNativeWorkspaceMemory memory)
        {
            Owner = new WindowsMspNativeWorkspaceCallbacks(
                invocation,
                cancellationToken,
                memory);
            Host = Owner.Host;
            invoke = Marshal.GetDelegateForFunctionPointer<MspNativeWorkspaceInvokeV1Function>(
                Host.Invoke);
            free = Marshal.GetDelegateForFunctionPointer<MspNativeWorkspaceFreeV1Function>(
                Host.Free);
            isCancelled =
                Marshal.GetDelegateForFunctionPointer<MspNativeWorkspaceIsCancelledV1Function>(
                    Host.IsCancelled);
        }

        public WindowsMspNativeWorkspaceCallbacks Owner { get; }

        public MspNativeWorkspaceHostV1 Host { get; }

        public CallbackResult Invoke(
            MspNativeWorkspaceOperationV1 operation,
            string path,
            ulong backendId = 1,
            ulong offset = 0,
            int length = 0,
            Func<MspNativeWorkspaceRequestV1, MspNativeWorkspaceRequestV1>? mutate = null)
        {
            return InvokeRawPath(
                operation,
                Encoding.UTF8.GetBytes(path),
                backendId,
                offset,
                checked((ulong)length),
                mutate);
        }

        public CallbackResult InvokeRawPath(
            MspNativeWorkspaceOperationV1 operation,
            byte[] path,
            ulong backendId = 1,
            ulong offset = 0,
            ulong length = 0,
            Func<MspNativeWorkspaceRequestV1, MspNativeWorkspaceRequestV1>? mutate = null)
        {
            var pathPointer = Marshal.AllocHGlobal(Math.Max(1, path.Length));
            try
            {
                if (path.Length > 0)
                {
                    Marshal.Copy(path, 0, pathPointer, path.Length);
                }

                var request = new MspNativeWorkspaceRequestV1
                {
                    Size = MspNativeWorkspaceAbiV1.RequestSize,
                    Operation = (uint)operation,
                    BackendId = backendId,
                    PathUtf8 = pathPointer,
                    PathLength = checked((ulong)path.Length),
                    Offset = offset,
                    Length = length,
                    Reserved = 0
                };
                if (mutate is not null)
                {
                    request = mutate(request);
                }

                var response = new nint(123);
                ulong responseLength = 456;
                var status = invoke(
                    Host.Context,
                    in request,
                    ref response,
                    ref responseLength);
                return new CallbackResult(
                    (MspNativeWorkspaceCallbackStatusV1)status,
                    response,
                    responseLength);
            }
            finally
            {
                Marshal.FreeHGlobal(pathPointer);
            }
        }

        public void Free(CallbackResult result)
        {
            free(Host.Context, result.Pointer, result.Length);
        }

        public int IsCancelled()
        {
            return isCancelled(Host.Context);
        }

        public void Dispose()
        {
            Owner.Dispose();
        }
    }

    private readonly record struct CallbackResult(
        MspNativeWorkspaceCallbackStatusV1 Status,
        nint Pointer,
        ulong Length)
    {
        public byte[] CopyBytes()
        {
            if (Pointer == nint.Zero || Length == 0)
            {
                return Array.Empty<byte>();
            }

            var bytes = new byte[checked((int)Length)];
            Marshal.Copy(Pointer, bytes, 0, bytes.Length);
            return bytes;
        }

        public string CopyUtf8()
        {
            return Encoding.UTF8.GetString(CopyBytes());
        }
    }

    private sealed class TestWorkspace : IMspNativeReadOnlyWorkspace
    {
        public Func<string, CancellationToken, ValueTask<MspNativeWorkspaceFileInfo>>
            StatHandler { get; set; } = (_, _) => ValueTask.FromResult(DirectoryInfo());

        public Func<string, CancellationToken,
            ValueTask<IReadOnlyList<MspNativeWorkspaceDirectoryEntry>>>
            ListHandler { get; set; } = (_, _) =>
                ValueTask.FromResult<IReadOnlyList<MspNativeWorkspaceDirectoryEntry>>(
                    Array.Empty<MspNativeWorkspaceDirectoryEntry>());

        public Func<string, ulong, int, CancellationToken, ValueTask<ReadOnlyMemory<byte>>>
            ReadHandler { get; set; } = (_, _, length, _) =>
                ValueTask.FromResult<ReadOnlyMemory<byte>>(
                    new byte[] { 1, 2, 3, 4 }.AsMemory(0, Math.Min(4, length)));

        public ValueTask<MspNativeWorkspaceFileInfo> StatAsync(
            string virtualPath,
            CancellationToken cancellationToken = default)
        {
            return StatHandler(virtualPath, cancellationToken);
        }

        public ValueTask<IReadOnlyList<MspNativeWorkspaceDirectoryEntry>> ListDirectoryAsync(
            string virtualPath,
            CancellationToken cancellationToken = default)
        {
            return ListHandler(virtualPath, cancellationToken);
        }

        public ValueTask<ReadOnlyMemory<byte>> ReadFileRangeAsync(
            string virtualPath,
            ulong offset,
            int length,
            CancellationToken cancellationToken = default)
        {
            return ReadHandler(virtualPath, offset, length, cancellationToken);
        }
    }

    private sealed class RecordingMemory : IMspNativeWorkspaceMemory
    {
        private readonly object gate = new();

        public int FreeCount { get; private set; }

        public int AllocationCount { get; private set; }

        public byte[] LastBytesBeforeZero { get; private set; } = Array.Empty<byte>();

        public byte[] LastBytesAfterZero { get; private set; } = Array.Empty<byte>();

        public nint Allocate(byte[] bytes)
        {
            AllocationCount++;
            var pointer = Marshal.AllocHGlobal(bytes.Length);
            Marshal.Copy(bytes, 0, pointer, bytes.Length);
            return pointer;
        }

        public void ZeroAndFree(nint pointer, int length)
        {
            lock (gate)
            {
                LastBytesBeforeZero = new byte[length];
                Marshal.Copy(pointer, LastBytesBeforeZero, 0, length);
                for (var index = 0; index < length; index++)
                {
                    Marshal.WriteByte(pointer, index, 0);
                }

                LastBytesAfterZero = new byte[length];
                Marshal.Copy(pointer, LastBytesAfterZero, 0, length);
                Marshal.FreeHGlobal(pointer);
                FreeCount++;
            }
        }
    }
}

internal static class MspNativeWorkspaceRequestV1TestExtensions
{
    public static MspNativeWorkspaceRequestV1 WithSize(
        this MspNativeWorkspaceRequestV1 request,
        uint size)
    {
        request.Size = size;
        return request;
    }

    public static MspNativeWorkspaceRequestV1 WithReserved(
        this MspNativeWorkspaceRequestV1 request,
        ulong reserved)
    {
        request.Reserved = reserved;
        return request;
    }
}
