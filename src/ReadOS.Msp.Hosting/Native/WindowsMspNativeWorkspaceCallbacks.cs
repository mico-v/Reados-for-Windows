using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Hosting.Native;

internal interface IMspNativeWorkspaceMemory
{
    nint Allocate(byte[] bytes);

    void ZeroAndFree(nint pointer, int length);
}

internal sealed class HGlobalMspNativeWorkspaceMemory : IMspNativeWorkspaceMemory
{
    private static readonly byte[] ZeroChunk = new byte[4096];

    public nint Allocate(byte[] bytes)
    {
        ArgumentNullException.ThrowIfNull(bytes);
        if (bytes.Length == 0)
        {
            return nint.Zero;
        }

        var pointer = Marshal.AllocHGlobal(bytes.Length);
        try
        {
            Marshal.Copy(bytes, 0, pointer, bytes.Length);
            return pointer;
        }
        catch
        {
            Marshal.FreeHGlobal(pointer);
            throw;
        }
    }

    public void ZeroAndFree(nint pointer, int length)
    {
        if (pointer == nint.Zero)
        {
            return;
        }

        try
        {
            var offset = 0;
            while (offset < length)
            {
                var count = Math.Min(ZeroChunk.Length, length - offset);
                Marshal.Copy(ZeroChunk, 0, pointer + offset, count);
                offset += count;
            }
        }
        finally
        {
            Marshal.FreeHGlobal(pointer);
        }
    }
}

/// <summary>
/// Owns one invocation-scoped reverse-P/Invoke context. Native code may borrow
/// the returned host table only while this object is alive and undisposed; it
/// must never retain, release, or call the context after the invocation returns.
/// </summary>
internal sealed class WindowsMspNativeWorkspaceCallbacks : IDisposable
{
    private static readonly AsyncLocal<InvocationState?> CurrentCallbackState = new();

    private static readonly Encoding StrictUtf8 = new UTF8Encoding(
        encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: true);

    private static readonly JsonSerializerOptions JsonOptions = CreateJsonOptions();

    // Static fields are process-lifetime strong roots for every function pointer
    // published in MspNativeWorkspaceHostV1.
    private static readonly MspNativeWorkspaceInvokeV1Function InvokeDelegate = Invoke;
    private static readonly MspNativeWorkspaceFreeV1Function FreeDelegate = Free;
    private static readonly MspNativeWorkspaceIsCancelledV1Function IsCancelledDelegate =
        IsCancelled;

    private static readonly nint InvokePointer =
        Marshal.GetFunctionPointerForDelegate(InvokeDelegate);
    private static readonly nint FreePointer =
        Marshal.GetFunctionPointerForDelegate(FreeDelegate);
    private static readonly nint IsCancelledPointer =
        Marshal.GetFunctionPointerForDelegate(IsCancelledDelegate);

    private readonly object disposeGate = new();
    private readonly InvocationState state;
    private GCHandle stateHandle;
    private bool disposed;

    public WindowsMspNativeWorkspaceCallbacks(
        MspNativeWorkspaceInvocation invocation,
        CancellationToken cancellationToken = default)
        : this(invocation, cancellationToken, new HGlobalMspNativeWorkspaceMemory())
    {
    }

    internal WindowsMspNativeWorkspaceCallbacks(
        MspNativeWorkspaceInvocation invocation,
        CancellationToken cancellationToken,
        IMspNativeWorkspaceMemory memory)
    {
        ArgumentNullException.ThrowIfNull(invocation);
        ArgumentNullException.ThrowIfNull(memory);
        if (IntPtr.Size != 8)
        {
            throw new PlatformNotSupportedException(
                "The native workspace callback ABI requires a 64-bit process.");
        }

        state = new InvocationState(invocation, cancellationToken, memory);
        stateHandle = GCHandle.Alloc(state, GCHandleType.Normal);
        Host = new MspNativeWorkspaceHostV1
        {
            Size = MspNativeWorkspaceAbiV1.HostSize,
            MajorVersion = MspNativeWorkspaceAbiV1.MajorVersion,
            MinorVersion = MspNativeWorkspaceAbiV1.MinorVersion,
            Reserved = 0,
            Capabilities = MspNativeWorkspaceAbiV1.RequiredCapabilities,
            Context = GCHandle.ToIntPtr(stateHandle),
            Invoke = InvokePointer,
            Free = FreePointer,
            IsCancelled = IsCancelledPointer,
            Reserved2 = 0
        };
    }

    internal MspNativeWorkspaceHostV1 Host { get; }

    internal bool IsPoisoned => state.IsPoisoned;

    internal int OutstandingAllocationCount => state.OutstandingAllocationCount;

    public void Dispose()
    {
        lock (disposeGate)
        {
            if (disposed)
            {
                return;
            }

            if (state.IsCurrentCallbackFlow)
            {
                throw new InvalidOperationException(
                    "A native workspace callback cannot dispose its own invocation context.");
            }

            disposed = true;
            state.CloseAndDrain();
            if (stateHandle.IsAllocated)
            {
                stateHandle.Free();
            }

            // Keep the rooted delegate instances visibly alive through context
            // cleanup. They are also held by static fields for process lifetime.
            GC.KeepAlive(InvokeDelegate);
            GC.KeepAlive(FreeDelegate);
            GC.KeepAlive(IsCancelledDelegate);
        }
    }

    private static int Invoke(
        nint context,
        in MspNativeWorkspaceRequestV1 request,
        ref nint response,
        ref ulong responseLength)
    {
        response = nint.Zero;
        responseLength = 0;

        var state = TryGetState(context);
        if (state is null)
        {
            return (int)MspNativeWorkspaceCallbackStatusV1.InvalidArgument;
        }

        var entry = state.TryEnterCallback();
        if (entry != CallbackEntryResult.Entered)
        {
            return entry == CallbackEntryResult.Reentrant
                ? (int)MspNativeWorkspaceCallbackStatusV1.Io
                : (int)MspNativeWorkspaceCallbackStatusV1.InvalidArgument;
        }

        try
        {
            if (state.IsPoisoned)
            {
                return (int)MspNativeWorkspaceCallbackStatusV1.Io;
            }

            var validation = ValidateRequest(state, request, out var operation, out var path);
            if (validation != MspNativeWorkspaceCallbackStatusV1.Ok)
            {
                return (int)validation;
            }

            if (state.IsCancellationRequested)
            {
                return (int)MspNativeWorkspaceCallbackStatusV1.Canceled;
            }

            if (!state.Invocation.TryGetBackend(request.BackendId, out var workspace))
            {
                return (int)MspNativeWorkspaceCallbackStatusV1.InvalidArgument;
            }

            return operation switch
            {
                MspNativeWorkspaceOperationV1.Stat => InvokeStat(
                    state,
                    workspace,
                    path,
                    ref response,
                    ref responseLength),
                MspNativeWorkspaceOperationV1.ListDirectory => InvokeListDirectory(
                    state,
                    workspace,
                    path,
                    ref response,
                    ref responseLength),
                MspNativeWorkspaceOperationV1.ReadFileRange => InvokeReadFileRange(
                    state,
                    workspace,
                    path,
                    request.Offset,
                    checked((int)request.Length),
                    ref response,
                    ref responseLength),
                _ => (int)MspNativeWorkspaceCallbackStatusV1.InvalidArgument
            };
        }
        catch (Exception exception)
        {
            response = nint.Zero;
            responseLength = 0;
            return (int)MapException(state, exception);
        }
        finally
        {
            state.ExitCallback();
        }
    }

    private static void Free(nint context, nint response, ulong responseLength)
    {
        var state = TryGetState(context);
        if (state is null)
        {
            return;
        }

        var entry = state.TryEnterCallback();
        if (entry != CallbackEntryResult.Entered)
        {
            return;
        }

        try
        {
            state.Free(response, responseLength);
        }
        catch
        {
            state.Poison();
        }
        finally
        {
            state.ExitCallback();
        }
    }

    private static int IsCancelled(nint context)
    {
        var state = TryGetState(context);
        if (state is null)
        {
            return -1;
        }

        var entry = state.TryEnterCallback();
        if (entry != CallbackEntryResult.Entered)
        {
            return -1;
        }

        try
        {
            return state.IsPoisoned
                ? -1
                : state.IsCancellationRequested ? 1 : 0;
        }
        catch
        {
            state.Poison();
            return -1;
        }
        finally
        {
            state.ExitCallback();
        }
    }

    private static int InvokeStat(
        InvocationState state,
        IMspNativeReadOnlyWorkspace workspace,
        string path,
        ref nint response,
        ref ulong responseLength)
    {
        var info = WaitOnWorker(() => workspace.StatAsync(path, state.CancellationToken));
        var wireInfo = FreezeInfo(info);
        return PublishJson(
            state,
            wireInfo,
            MspNativeWorkspaceAbiV1.MaximumStatResponseBytes,
            ref response,
            ref responseLength);
    }

    private static int InvokeListDirectory(
        InvocationState state,
        IMspNativeReadOnlyWorkspace workspace,
        string path,
        ref nint response,
        ref ulong responseLength)
    {
        var entries = WaitOnWorker(
            () => workspace.ListDirectoryAsync(path, state.CancellationToken));
        if (entries is null)
        {
            throw new InvalidOperationException(
                "The managed workspace returned a null directory listing.");
        }
        if (entries.Count > MspNativeWorkspaceAbiV1.MaximumDirectoryEntries)
        {
            return (int)MspNativeWorkspaceCallbackStatusV1.LimitExceeded;
        }

        var seen = new HashSet<string>(StringComparer.Ordinal);
        var frozen = new MspNativeWorkspaceDirectoryEntryWireV1[entries.Count];
        long maximumSerializedBytes = 2;
        for (var index = 0; index < entries.Count; index++)
        {
            var entry = entries[index] ?? throw new InvalidOperationException(
                "The managed workspace returned a null directory entry.");
            ValidateEntryName(entry.Name);
            if (!seen.Add(entry.Name))
            {
                throw new InvalidOperationException(
                    "The managed workspace returned duplicate directory entries.");
            }

            var info = FreezeInfo(entry.Info);
            maximumSerializedBytes = checked(
                maximumSerializedBytes + EstimateMaximumSerializedEntryBytes(entry.Name, info));
            if (maximumSerializedBytes > MspNativeWorkspaceAbiV1.MaximumListResponseBytes)
            {
                return (int)MspNativeWorkspaceCallbackStatusV1.LimitExceeded;
            }

            frozen[index] = new MspNativeWorkspaceDirectoryEntryWireV1
            {
                Name = entry.Name,
                Info = info
            };
        }

        return PublishJson(
            state,
            frozen,
            MspNativeWorkspaceAbiV1.MaximumListResponseBytes,
            ref response,
            ref responseLength);
    }

    private static int InvokeReadFileRange(
        InvocationState state,
        IMspNativeReadOnlyWorkspace workspace,
        string path,
        ulong offset,
        int length,
        ref nint response,
        ref ulong responseLength)
    {
        var bytes = WaitOnWorker(
            () => workspace.ReadFileRangeAsync(
                path,
                offset,
                length,
                state.CancellationToken));
        if (bytes.Length > length ||
            bytes.Length > MspNativeWorkspaceAbiV1.MaximumReadRangeBytes)
        {
            return (int)MspNativeWorkspaceCallbackStatusV1.Io;
        }

        return PublishBytes(
            state,
            bytes.ToArray(),
            MspNativeWorkspaceAbiV1.MaximumReadRangeBytes,
            ref response,
            ref responseLength);
    }

    private static int PublishJson<T>(
        InvocationState state,
        T value,
        int maximumBytes,
        ref nint response,
        ref ulong responseLength)
    {
        byte[] bytes;
        try
        {
            bytes = JsonSerializer.SerializeToUtf8Bytes(value, JsonOptions);
        }
        catch (Exception exception) when (exception is JsonException or NotSupportedException)
        {
            return (int)MspNativeWorkspaceCallbackStatusV1.Io;
        }

        return PublishBytes(
            state,
            bytes,
            maximumBytes,
            ref response,
            ref responseLength);
    }

    private static int PublishBytes(
        InvocationState state,
        byte[] bytes,
        int maximumBytes,
        ref nint response,
        ref ulong responseLength)
    {
        try
        {
            if (bytes.Length > maximumBytes)
            {
                return (int)MspNativeWorkspaceCallbackStatusV1.LimitExceeded;
            }

            if (state.IsCancellationRequested)
            {
                return (int)MspNativeWorkspaceCallbackStatusV1.Canceled;
            }

            if (state.IsPoisoned)
            {
                return (int)MspNativeWorkspaceCallbackStatusV1.Io;
            }

            if (!state.TryAllocate(bytes, out var pointer, out var length))
            {
                return state.IsPoisoned
                    ? (int)MspNativeWorkspaceCallbackStatusV1.Io
                    : (int)MspNativeWorkspaceCallbackStatusV1.LimitExceeded;
            }

            if (state.IsCancellationRequested || state.IsPoisoned)
            {
                state.ReleaseOwned(pointer, length);
                return state.IsCancellationRequested
                    ? (int)MspNativeWorkspaceCallbackStatusV1.Canceled
                    : (int)MspNativeWorkspaceCallbackStatusV1.Io;
            }

            response = pointer;
            responseLength = length;
            return (int)MspNativeWorkspaceCallbackStatusV1.Ok;
        }
        finally
        {
            CryptographicOperations.ZeroMemory(bytes);
        }
    }

    private static MspNativeWorkspaceCallbackStatusV1 ValidateRequest(
        InvocationState state,
        in MspNativeWorkspaceRequestV1 request,
        out MspNativeWorkspaceOperationV1 operation,
        out string path)
    {
        operation = default;
        path = string.Empty;
        if (request.Size != MspNativeWorkspaceAbiV1.RequestSize ||
            request.Reserved != 0 ||
            request.BackendId == 0 ||
            !Enum.IsDefined(typeof(MspNativeWorkspaceOperationV1), request.Operation))
        {
            return MspNativeWorkspaceCallbackStatusV1.InvalidArgument;
        }

        operation = (MspNativeWorkspaceOperationV1)request.Operation;
        if (operation is MspNativeWorkspaceOperationV1.Stat or
            MspNativeWorkspaceOperationV1.ListDirectory)
        {
            if (request.Offset != 0 || request.Length != 0)
            {
                return MspNativeWorkspaceCallbackStatusV1.InvalidArgument;
            }
        }
        else if (request.Length > MspNativeWorkspaceAbiV1.MaximumReadRangeBytes ||
            request.Length > int.MaxValue)
        {
            return MspNativeWorkspaceCallbackStatusV1.LimitExceeded;
        }

        var pathStatus = TryReadPath(request.PathUtf8, request.PathLength, out path);
        if (pathStatus != MspNativeWorkspaceCallbackStatusV1.Ok)
        {
            return pathStatus;
        }

        if (state.IsPoisoned)
        {
            return MspNativeWorkspaceCallbackStatusV1.Io;
        }

        return MspNativeWorkspaceCallbackStatusV1.Ok;
    }

    private static MspNativeWorkspaceCallbackStatusV1 TryReadPath(
        nint pointer,
        ulong length,
        out string path)
    {
        path = string.Empty;
        if (pointer == nint.Zero ||
            length == 0 ||
            length > MspNativeWorkspaceAbiV1.MaximumPathBytes ||
            length > int.MaxValue)
        {
            return MspNativeWorkspaceCallbackStatusV1.InvalidArgument;
        }

        var bytes = new byte[checked((int)length)];
        try
        {
            Marshal.Copy(pointer, bytes, 0, bytes.Length);
            try
            {
                path = StrictUtf8.GetString(bytes);
            }
            catch (DecoderFallbackException)
            {
                return MspNativeWorkspaceCallbackStatusV1.InvalidPath;
            }
        }
        finally
        {
            CryptographicOperations.ZeroMemory(bytes);
        }

        if (path.Contains('\0') ||
            path.Contains('\\') ||
            path.Any(char.IsControl) ||
            !path.StartsWith("/", StringComparison.Ordinal) ||
            !string.Equals(MspPathUtility.Normalize(path), path, StringComparison.Ordinal))
        {
            return MspNativeWorkspaceCallbackStatusV1.InvalidPath;
        }

        if (path
            .Split('/', StringSplitOptions.RemoveEmptyEntries)
            .Any(component => string.Equals(component, ".msp", StringComparison.OrdinalIgnoreCase)))
        {
            return MspNativeWorkspaceCallbackStatusV1.HiddenPath;
        }

        return MspNativeWorkspaceCallbackStatusV1.Ok;
    }

    private static void ValidateEntryName(string name)
    {
        if (string.IsNullOrWhiteSpace(name) ||
            name is "." or ".." ||
            name.Contains('/') ||
            name.Contains('\\') ||
            name.Contains('\0') ||
            name.Any(char.IsControl) ||
            StrictUtf8.GetByteCount(name) > MspNativeWorkspaceAbiV1.MaximumEntryNameBytes)
        {
            throw new InvalidOperationException(
                "The managed workspace returned an invalid directory entry name.");
        }
    }

    private static MspNativeWorkspaceFileInfoWireV1 FreezeInfo(
        MspNativeWorkspaceFileInfo info)
    {
        if (info is null)
        {
            throw new InvalidOperationException(
                "The managed workspace returned null file metadata.");
        }
        if (!Enum.IsDefined(info.FileType))
        {
            throw new InvalidOperationException(
                "The managed workspace returned an invalid file type.");
        }

        if (info.FileIdentity is not { } identity)
        {
            return new MspNativeWorkspaceFileInfoWireV1
            {
                FileType = info.FileType,
                SizeBytes = info.SizeBytes,
                ModificationTimeUnixMs = info.ModificationTimeUnixMs,
                FileIdentity = null
            };
        }

        if (identity.Length == 0 ||
            identity.Contains('/') ||
            identity.Contains('\\') ||
            identity.Contains('\0') ||
            identity.Any(char.IsControl) ||
            StrictUtf8.GetByteCount(identity) >
                MspNativeWorkspaceAbiV1.MaximumFileIdentityBytes)
        {
            throw new InvalidOperationException(
                "The managed workspace returned an invalid file identity.");
        }

        return new MspNativeWorkspaceFileInfoWireV1
        {
            FileType = info.FileType,
            SizeBytes = info.SizeBytes,
            ModificationTimeUnixMs = info.ModificationTimeUnixMs,
            FileIdentity = identity
        };
    }

    private static long EstimateMaximumSerializedEntryBytes(
        string name,
        MspNativeWorkspaceFileInfoWireV1 info)
    {
        // System.Text.Json can use a six-byte \uXXXX escape for each UTF-16
        // code unit. The fixed allowance covers property names, punctuation,
        // enum text, and the maximum decimal forms of all numeric fields.
        return 256L +
            checked(name.Length * 6L) +
            checked((info.FileIdentity?.Length ?? 0) * 6L);
    }

    private static T WaitOnWorker<T>(Func<ValueTask<T>> operation)
    {
        return Task.Run(
            async () => await operation().ConfigureAwait(false),
            CancellationToken.None).GetAwaiter().GetResult();
    }

    private static MspNativeWorkspaceCallbackStatusV1 MapException(
        InvocationState state,
        Exception exception)
    {
        if (exception is OperationCanceledException && state.IsCancellationRequested)
        {
            return MspNativeWorkspaceCallbackStatusV1.Canceled;
        }

        if (exception is MspNativeWorkspaceException workspaceException)
        {
            return workspaceException.ErrorKind switch
            {
                MspNativeWorkspaceErrorKind.NotFound =>
                    MspNativeWorkspaceCallbackStatusV1.NotFound,
                MspNativeWorkspaceErrorKind.NotDirectory =>
                    MspNativeWorkspaceCallbackStatusV1.NotDirectory,
                MspNativeWorkspaceErrorKind.IsDirectory =>
                    MspNativeWorkspaceCallbackStatusV1.IsDirectory,
                MspNativeWorkspaceErrorKind.AccessDenied =>
                    MspNativeWorkspaceCallbackStatusV1.AccessDenied,
                MspNativeWorkspaceErrorKind.HiddenPath =>
                    MspNativeWorkspaceCallbackStatusV1.HiddenPath,
                MspNativeWorkspaceErrorKind.InvalidPath =>
                    MspNativeWorkspaceCallbackStatusV1.InvalidPath,
                MspNativeWorkspaceErrorKind.LimitExceeded =>
                    MspNativeWorkspaceCallbackStatusV1.LimitExceeded,
                MspNativeWorkspaceErrorKind.Unsupported =>
                    MspNativeWorkspaceCallbackStatusV1.Unsupported,
                _ => MspNativeWorkspaceCallbackStatusV1.Io
            };
        }

        return exception switch
        {
            ArgumentException => MspNativeWorkspaceCallbackStatusV1.InvalidArgument,
            NotSupportedException => MspNativeWorkspaceCallbackStatusV1.Unsupported,
            OutOfMemoryException => MspNativeWorkspaceCallbackStatusV1.LimitExceeded,
            _ => MspNativeWorkspaceCallbackStatusV1.Io
        };
    }

    private static InvocationState? TryGetState(nint context)
    {
        if (context == nint.Zero)
        {
            return null;
        }

        try
        {
            return GCHandle.FromIntPtr(context).Target as InvocationState;
        }
        catch
        {
            return null;
        }
    }

    private static JsonSerializerOptions CreateJsonOptions()
    {
        var options = new JsonSerializerOptions
        {
            PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
            PropertyNameCaseInsensitive = false,
            DefaultIgnoreCondition = JsonIgnoreCondition.Never
        };
        options.Converters.Add(new JsonStringEnumConverter(
            JsonNamingPolicy.CamelCase,
            allowIntegerValues: false));
        return options;
    }

    private enum CallbackEntryResult
    {
        Entered,
        Closed,
        Reentrant
    }

    private sealed class InvocationState
    {
        private readonly object allocationGate = new();
        private readonly Dictionary<nint, Allocation> allocations = new();
        private readonly ManualResetEventSlim callbackIdle = new(initialState: true);
        private readonly IMspNativeWorkspaceMemory memory;
        private int callbackActive;
        private int closing;
        private int poisoned;

        public InvocationState(
            MspNativeWorkspaceInvocation invocation,
            CancellationToken cancellationToken,
            IMspNativeWorkspaceMemory memory)
        {
            Invocation = invocation;
            CancellationToken = cancellationToken;
            this.memory = memory;
        }

        public MspNativeWorkspaceInvocation Invocation { get; }

        public CancellationToken CancellationToken { get; }

        public bool IsCancellationRequested => CancellationToken.IsCancellationRequested;

        public bool IsPoisoned => Volatile.Read(ref poisoned) != 0;

        public bool IsCurrentCallbackFlow =>
            Volatile.Read(ref callbackActive) != 0 &&
            ReferenceEquals(CurrentCallbackState.Value, this);

        public int OutstandingAllocationCount
        {
            get
            {
                lock (allocationGate)
                {
                    return allocations.Count;
                }
            }
        }

        public CallbackEntryResult TryEnterCallback()
        {
            if (Volatile.Read(ref closing) != 0)
            {
                return CallbackEntryResult.Closed;
            }

            if (Interlocked.CompareExchange(ref callbackActive, 1, 0) != 0)
            {
                Poison();
                return CallbackEntryResult.Reentrant;
            }

            callbackIdle.Reset();
            CurrentCallbackState.Value = this;
            if (Volatile.Read(ref closing) != 0)
            {
                ExitCallback();
                return CallbackEntryResult.Closed;
            }

            return CallbackEntryResult.Entered;
        }

        public void ExitCallback()
        {
            if (ReferenceEquals(CurrentCallbackState.Value, this))
            {
                CurrentCallbackState.Value = null;
            }

            Volatile.Write(ref callbackActive, 0);
            callbackIdle.Set();
        }

        public bool TryAllocate(byte[] bytes, out nint pointer, out ulong length)
        {
            pointer = nint.Zero;
            length = checked((ulong)bytes.Length);
            if (bytes.Length == 0)
            {
                return true;
            }

            nint allocated;
            try
            {
                allocated = memory.Allocate(bytes);
            }
            catch
            {
                return false;
            }

            if (allocated == nint.Zero)
            {
                Poison();
                return false;
            }

            lock (allocationGate)
            {
                if (Volatile.Read(ref closing) != 0 ||
                    IsPoisoned ||
                    !allocations.TryAdd(allocated, new Allocation(allocated, bytes.Length)))
                {
                    Poison();
                    TryZeroAndFree(allocated, bytes.Length);
                    return false;
                }
            }

            pointer = allocated;
            return true;
        }

        public void Free(nint pointer, ulong length)
        {
            if (pointer == nint.Zero)
            {
                return;
            }

            Allocation allocation;
            lock (allocationGate)
            {
                if (!allocations.TryGetValue(pointer, out allocation) ||
                    length != checked((ulong)allocation.Length))
                {
                    Poison();
                    return;
                }

                allocations.Remove(pointer);
            }

            TryZeroAndFree(allocation.Pointer, allocation.Length);
        }

        public void ReleaseOwned(nint pointer, ulong length)
        {
            Free(pointer, length);
        }

        public void Poison()
        {
            Interlocked.Exchange(ref poisoned, 1);
        }

        public void CloseAndDrain()
        {
            Interlocked.Exchange(ref closing, 1);
            callbackIdle.Wait();

            Allocation[] remaining;
            lock (allocationGate)
            {
                remaining = allocations.Values.ToArray();
                allocations.Clear();
            }

            foreach (var allocation in remaining)
            {
                TryZeroAndFree(allocation.Pointer, allocation.Length);
            }

            callbackIdle.Dispose();
        }

        private void TryZeroAndFree(nint pointer, int length)
        {
            try
            {
                memory.ZeroAndFree(pointer, length);
            }
            catch
            {
                Poison();
            }
        }

        private readonly record struct Allocation(nint Pointer, int Length);
    }
}
