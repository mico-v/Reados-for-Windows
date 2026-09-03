using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Runtime.InteropServices;

namespace ReadOS.Msp.Hosting.Native.RuntimeFfi;

public sealed class MspCommandRuntimeFfiLibrary : IMspCommandRuntimeFfiAdapter
{
    private readonly object gate = new();
    private readonly MspCommandRuntimeFfiNativeExports exports;
    private readonly MspCommandRuntimeFfiModuleLease moduleLease;
    private readonly MspCommandRuntimeFfiLimits limits;
    private readonly object moduleIdentity = new();
    private int disposed;

    private MspCommandRuntimeFfiLibrary(
        IMspCommandRuntimeFfiNativeLibrary nativeLibrary,
        MspCommandRuntimeFfiNativeExports exports,
        MspCommandRuntimeFfiLimits limits)
    {
        this.exports = exports;
        this.limits = limits;
        moduleLease = new MspCommandRuntimeFfiModuleLease(nativeLibrary);
        AbiVersion = MspCommandRuntimeFfiAbi.AbiVersion;
        HeaderVersion = MspCommandRuntimeFfiAbi.HeaderVersion;
    }

    public uint AbiVersion { get; }

    public string HeaderVersion { get; }

    public static MspCommandRuntimeFfiLibrary Load(
        string fullyQualifiedLibraryPath,
        MspCommandRuntimeFfiOptions? options = null)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(fullyQualifiedLibraryPath);
        var resolvedOptions = options ?? new MspCommandRuntimeFfiOptions();
        return Load(
            fullyQualifiedLibraryPath,
            resolvedOptions,
            new SystemMspCommandRuntimeFfiNativeLibraryLoader());
    }

    internal static MspCommandRuntimeFfiLibrary Load(
        string fullyQualifiedLibraryPath,
        MspCommandRuntimeFfiOptions options,
        IMspCommandRuntimeFfiNativeLibraryLoader loader)
    {
        ArgumentNullException.ThrowIfNull(options);
        ArgumentNullException.ThrowIfNull(loader);
        IMspCommandRuntimeFfiNativeLibrary? nativeLibrary = null;
        try
        {
            var path = options.ValidateAndGetFullPath(fullyQualifiedLibraryPath);
            nativeLibrary = loader.TryLoad(path)
                ?? throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LibraryUnavailable);
            var exports = BindExports(nativeLibrary);
            ValidateAbi(exports);
            return new MspCommandRuntimeFfiLibrary(nativeLibrary, exports, options.Limits);
        }
        catch (MspCommandRuntimeFfiException)
        {
            nativeLibrary?.Dispose();
            throw;
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            nativeLibrary?.Dispose();
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LibraryUnavailable);
        }
    }

    internal static MspCommandRuntimeFfiLibrary CreateForTests(
        IMspCommandRuntimeFfiNativeLibrary nativeLibrary,
        MspCommandRuntimeFfiNativeExports exports,
        MspCommandRuntimeFfiLimits? limits = null)
    {
        ArgumentNullException.ThrowIfNull(nativeLibrary);
        ArgumentNullException.ThrowIfNull(exports);
        var resolvedLimits = limits ?? new MspCommandRuntimeFfiLimits();
        resolvedLimits.Validate();
        ValidateAbi(exports);
        return new MspCommandRuntimeFfiLibrary(nativeLibrary, exports, resolvedLimits);
    }

    public MspCommandRuntimeFfiRuntime CreateRuntime()
    {
        var lease = AcquireNewHandleLease();
        nint nativeHandle;
        try
        {
            nativeHandle = exports.RuntimeCreate();
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            lease.Dispose();
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.NativeInternalError);
        }

        if (IsInvalidNativeHandle(nativeHandle))
        {
            lease.Dispose();
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.NullHandle);
        }

        MspCommandRuntimeFfiRuntimeSafeHandle? safeHandle = null;
        try
        {
            safeHandle = new MspCommandRuntimeFfiRuntimeSafeHandle(
                nativeHandle,
                exports.RuntimeFree,
                lease);
            return new MspCommandRuntimeFfiRuntime(moduleIdentity, safeHandle);
        }
        catch
        {
            safeHandle?.Dispose();
            if (safeHandle is null)
            {
                try
                {
                    exports.RuntimeFree(nativeHandle);
                }
                catch (Exception exception) when (!IsFatal(exception))
                {
                }

                lease.Dispose();
            }

            throw;
        }
    }

    public MspCommandRuntimeFfiWorkspace CreateWorkspace()
    {
        var lease = AcquireNewHandleLease();
        nint nativeHandle;
        try
        {
            nativeHandle = exports.WorkspaceCreate();
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            lease.Dispose();
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.NativeInternalError);
        }

        if (IsInvalidNativeHandle(nativeHandle))
        {
            lease.Dispose();
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.NullHandle);
        }

        MspCommandRuntimeFfiWorkspaceSafeHandle? safeHandle = null;
        try
        {
            safeHandle = new MspCommandRuntimeFfiWorkspaceSafeHandle(
                nativeHandle,
                exports.WorkspaceFree,
                lease);
            return new MspCommandRuntimeFfiWorkspace(
                moduleIdentity,
                safeHandle,
                exports.WorkspacePutFile,
                limits);
        }
        catch
        {
            safeHandle?.Dispose();
            if (safeHandle is null)
            {
                try
                {
                    exports.WorkspaceFree(nativeHandle);
                }
                catch (Exception exception) when (!IsFatal(exception))
                {
                }

                lease.Dispose();
            }

            throw;
        }
    }

    public MspCommandRuntimeFfiResult Execute(
        MspCommandRuntimeFfiRuntime runtime,
        MspCommandRuntimeFfiWorkspace workspace,
        MspCommandRuntimeFfiRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);
        var requestJson = MspCommandRuntimeFfiRequestSerializer.Serialize(request, limits);
        try
        {
            return ExecuteJson(runtime, workspace, requestJson);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(requestJson);
        }
    }

    public MspCommandRuntimeFfiResult ExecuteJson(
        MspCommandRuntimeFfiRuntime runtime,
        MspCommandRuntimeFfiWorkspace workspace,
        ReadOnlySpan<byte> requestJsonUtf8)
    {
        ArgumentNullException.ThrowIfNull(runtime);
        ArgumentNullException.ThrowIfNull(workspace);
        ThrowIfDisposed();
        EnsureMatchingLibrary(runtime, workspace);
        var request = ValidateRawRequest(requestJsonUtf8);

        try
        {
            // The workspace gate is held through the complete native invocation.
            // The runtime reference prevents its SafeHandle from closing concurrently.
            using var workspaceOperation = workspace.EnterOperation();
            using var runtimeReference = runtime.AcquireNativeReference();
            using var workspaceReference = workspace.AcquireNativeReferenceUnderOperation();
            using var requestPin = PinnedBuffer.Create(request);
            IDisposable? resultLease = null;
            nint nativeResult;
            try
            {
                resultLease = AcquireExistingHandleLease();
                nativeResult = exports.ExecuteJson(
                    runtimeReference.Pointer,
                    workspaceReference.Pointer,
                    requestPin.Pointer,
                    checked((nuint)request.Length));
            }
            catch (MspCommandRuntimeFfiException)
            {
                resultLease?.Dispose();
                throw;
            }
            catch (Exception exception) when (!IsFatal(exception))
            {
                resultLease?.Dispose();
                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.NativeInternalError);
            }

            if (IsInvalidNativeHandle(nativeResult))
            {
                resultLease!.Dispose();
                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.NullResult);
            }

            MspCommandRuntimeFfiResultSafeHandle? resultHandle = null;
            try
            {
                resultHandle = new MspCommandRuntimeFfiResultSafeHandle(
                    nativeResult,
                    exports.ResultFree,
                    resultLease!);
                return CopyResult(resultHandle);
            }
            catch
            {
                resultHandle?.Dispose();
                if (resultHandle is null)
                {
                    // The constructor cannot normally fail, but ownership remains
                    // ours if it does.
                    try
                    {
                        exports.ResultFree(nativeResult);
                    }
                    catch (Exception exception) when (!IsFatal(exception))
                    {
                    }

                    resultLease.Dispose();
                }

                throw;
            }
        }
        finally
        {
            CryptographicOperations.ZeroMemory(request);
        }
    }

    public void Dispose()
    {
        lock (gate)
        {
            if (Interlocked.Exchange(ref disposed, 1) == 0)
            {
                moduleLease.ReleaseRoot();
            }
        }

        GC.SuppressFinalize(this);
    }

    private void ThrowIfDisposed()
    {
        if (Volatile.Read(ref disposed) != 0)
        {
            throw new ObjectDisposedException(nameof(MspCommandRuntimeFfiLibrary));
        }
    }

    private IDisposable AcquireNewHandleLease()
    {
        lock (gate)
        {
            if (Volatile.Read(ref disposed) != 0)
            {
                throw new ObjectDisposedException(nameof(MspCommandRuntimeFfiLibrary));
            }

            if (!moduleLease.TryAcquire(out var lease) || lease is null)
            {
                throw new ObjectDisposedException(nameof(MspCommandRuntimeFfiLibrary));
            }

            return lease;
        }
    }

    private IDisposable AcquireExistingHandleLease()
    {
        if (!moduleLease.TryAcquireExisting(out var lease) || lease is null)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.NullResult);
        }

        return lease;
    }

    private void EnsureMatchingLibrary(
        MspCommandRuntimeFfiRuntime runtime,
        MspCommandRuntimeFfiWorkspace workspace)
    {
        if (!ReferenceEquals(runtime.ModuleIdentity, moduleIdentity) ||
            !ReferenceEquals(workspace.ModuleIdentity, moduleIdentity))
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.CrossLibraryHandle);
        }
    }

    private byte[] ValidateRawRequest(ReadOnlySpan<byte> requestJsonUtf8)
    {
        if (requestJsonUtf8.Length == 0)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }

        if (requestJsonUtf8.Length > limits.MaximumJsonRequestBytes)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LimitExceeded);
        }

        if (requestJsonUtf8.Contains((byte)0))
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }

        try
        {
            _ = MspCommandRuntimeFfiUtf8.Strict.GetCharCount(requestJsonUtf8);
        }
        catch (DecoderFallbackException)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidArgument);
        }

        var request = requestJsonUtf8.ToArray();
        MspCommandRuntimeFfiRequestSerializer.ValidateRaw(request, limits);
        return request;
    }

    private MspCommandRuntimeFfiResult CopyResult(
        MspCommandRuntimeFfiResultSafeHandle resultHandle)
    {
        var resultPointer = resultHandle.DangerousGetHandle();
        int exitCode;
        try
        {
            exitCode = exports.ResultExitCode(resultPointer);
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.NativeInternalError);
        }

        var stdout = CopyBuffer(exports.ResultStdoutData, resultPointer, limits.MaximumOutputBytes);
        byte[]? stderr = null;
        byte[]? diagnostic = null;
        try
        {
            stderr = CopyBuffer(exports.ResultStderrData, resultPointer, limits.MaximumOutputBytes);
            diagnostic = CopyBuffer(exports.ResultDiagnosticData, resultPointer, limits.MaximumDiagnosticBytes);
            var diagnosticCode = ValidateDiagnostic(diagnostic);
            return new MspCommandRuntimeFfiResult(
                exitCode,
                stdout,
                stderr,
                diagnostic,
                diagnosticCode,
                resultHandle);
        }
        catch
        {
            CryptographicOperations.ZeroMemory(stdout);
            if (stderr is not null)
            {
                CryptographicOperations.ZeroMemory(stderr);
            }

            if (diagnostic is not null)
            {
                CryptographicOperations.ZeroMemory(diagnostic);
            }

            throw;
        }
    }

    private byte[] CopyBuffer(
        MspCommandRuntimeFfiResultDataDelegate accessor,
        nint result,
        int maximum)
    {
        nuint length;
        nint pointer;
        try
        {
            pointer = accessor(result, out length);
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.NativeInternalError);
        }

        if (length > (nuint)maximum)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.LimitExceeded);
        }

        if (length > (nuint)int.MaxValue || length != 0 && pointer == nint.Zero)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidBuffer);
        }

        if (length == 0)
        {
            return Array.Empty<byte>();
        }

        var resultBytes = new byte[checked((int)length)];
        try
        {
            Marshal.Copy(pointer, resultBytes, 0, resultBytes.Length);
            return resultBytes;
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            CryptographicOperations.ZeroMemory(resultBytes);
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidBuffer);
        }
    }

    private static string ValidateDiagnostic(byte[] diagnostic)
    {
        if (diagnostic.Length == 0)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidDiagnostic);
        }

        try
        {
            _ = MspCommandRuntimeFfiUtf8.Strict.GetString(diagnostic);
            using var document = JsonDocument.Parse(diagnostic);
            var root = document.RootElement;
            if (root.ValueKind != JsonValueKind.Object || root.EnumerateObject().Count() != 1 ||
                !root.TryGetProperty("code", out var codeProperty) ||
                codeProperty.ValueKind != JsonValueKind.String)
            {
                throw new FormatException();
            }

            var code = codeProperty.GetString();
            if (string.IsNullOrEmpty(code) || code.Length > 128 ||
                !code.All(character => char.IsAsciiLetterOrDigit(character) || character is '.' or '_' or '-'))
            {
                throw new FormatException();
            }

            return code;
        }
        catch (Exception exception) when (exception is DecoderFallbackException or JsonException or FormatException or InvalidOperationException)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.InvalidDiagnostic);
        }
    }

    private static MspCommandRuntimeFfiNativeExports BindExports(
        IMspCommandRuntimeFfiNativeLibrary nativeLibrary)
    {
        try
        {
            var addresses = new Dictionary<string, nint>(StringComparer.Ordinal);
            var missing = false;
            foreach (var exportName in MspCommandRuntimeFfiExports.All)
            {
                if (!nativeLibrary.TryGetExport(exportName, out var address) || address == nint.Zero)
                {
                    missing = true;
                    continue;
                }

                addresses.Add(exportName, address);
            }

            if (missing)
            {
                throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.ExportUnavailable);
            }

            return new MspCommandRuntimeFfiNativeExports(
                Bind<MspCommandRuntimeFfiAbiVersionDelegate>(addresses, MspCommandRuntimeFfiExports.AbiVersion),
                Bind<MspCommandRuntimeFfiVersionDataDelegate>(addresses, MspCommandRuntimeFfiExports.VersionData),
                Bind<MspCommandRuntimeFfiRuntimeCreateDelegate>(addresses, MspCommandRuntimeFfiExports.RuntimeCreate),
                Bind<MspCommandRuntimeFfiRuntimeFreeDelegate>(addresses, MspCommandRuntimeFfiExports.RuntimeFree),
                Bind<MspCommandRuntimeFfiWorkspaceCreateDelegate>(addresses, MspCommandRuntimeFfiExports.WorkspaceCreate),
                Bind<MspCommandRuntimeFfiWorkspaceFreeDelegate>(addresses, MspCommandRuntimeFfiExports.WorkspaceFree),
                Bind<MspCommandRuntimeFfiWorkspacePutFileDelegate>(addresses, MspCommandRuntimeFfiExports.WorkspacePutFile),
                Bind<MspCommandRuntimeFfiExecuteJsonDelegate>(addresses, MspCommandRuntimeFfiExports.ExecuteJson),
                Bind<MspCommandRuntimeFfiResultExitCodeDelegate>(addresses, MspCommandRuntimeFfiExports.ResultExitCode),
                Bind<MspCommandRuntimeFfiResultDataDelegate>(addresses, MspCommandRuntimeFfiExports.ResultStdoutData),
                Bind<MspCommandRuntimeFfiResultDataDelegate>(addresses, MspCommandRuntimeFfiExports.ResultStderrData),
                Bind<MspCommandRuntimeFfiResultDataDelegate>(addresses, MspCommandRuntimeFfiExports.ResultDiagnosticData),
                Bind<MspCommandRuntimeFfiResultFreeDelegate>(addresses, MspCommandRuntimeFfiExports.ResultFree));
        }
        catch (MspCommandRuntimeFfiException)
        {
            throw;
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.ExportUnavailable);
        }
    }

    private static TDelegate Bind<TDelegate>(
        IReadOnlyDictionary<string, nint> addresses,
        string exportName)
        where TDelegate : Delegate
    {
        if (!addresses.TryGetValue(exportName, out var address) || address == nint.Zero)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.ExportUnavailable);
        }

        try
        {
            return Marshal.GetDelegateForFunctionPointer<TDelegate>(address);
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.ExportUnavailable);
        }
    }

    private static void ValidateAbi(MspCommandRuntimeFfiNativeExports exports)
    {
        uint abiVersion;
        try
        {
            abiVersion = exports.AbiVersion();
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.AbiMismatch);
        }

        if (abiVersion != MspCommandRuntimeFfiAbi.AbiVersion)
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.AbiMismatch);
        }

        nuint length;
        nint pointer;
        try
        {
            pointer = exports.VersionData(out length);
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.VersionMismatch);
        }

        if (length != (nuint)MspCommandRuntimeFfiAbi.HeaderVersion.Length || IsInvalidNativeHandle(pointer))
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.VersionMismatch);
        }

        var bytes = new byte[MspCommandRuntimeFfiAbi.HeaderVersion.Length];
        try
        {
            Marshal.Copy(pointer, bytes, 0, bytes.Length);
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.VersionMismatch);
        }

        if (!bytes.AsSpan().SequenceEqual("0.1.0"u8))
        {
            throw MspCommandRuntimeFfiException.Create(MspCommandRuntimeFfiFailureKind.VersionMismatch);
        }
    }

    private static bool IsInvalidNativeHandle(nint handle) =>
        handle == nint.Zero || handle == -1;

    private static bool IsFatal(Exception exception) =>
        exception is OutOfMemoryException or StackOverflowException;
}

public sealed class MspCommandRuntimeFfiAdapter : IMspCommandRuntimeFfiAdapter
{
    private readonly MspCommandRuntimeFfiLibrary library;

    private MspCommandRuntimeFfiAdapter(MspCommandRuntimeFfiLibrary library)
    {
        this.library = library;
    }

    public uint AbiVersion => library.AbiVersion;

    public string HeaderVersion => library.HeaderVersion;

    public static MspCommandRuntimeFfiAdapter Load(
        string fullyQualifiedLibraryPath,
        MspCommandRuntimeFfiOptions? options = null)
    {
        return new MspCommandRuntimeFfiAdapter(
            MspCommandRuntimeFfiLibrary.Load(fullyQualifiedLibraryPath, options));
    }

    public MspCommandRuntimeFfiRuntime CreateRuntime() => library.CreateRuntime();

    public MspCommandRuntimeFfiWorkspace CreateWorkspace() => library.CreateWorkspace();

    public MspCommandRuntimeFfiResult ExecuteJson(
        MspCommandRuntimeFfiRuntime runtime,
        MspCommandRuntimeFfiWorkspace workspace,
        ReadOnlySpan<byte> requestJsonUtf8) =>
        library.ExecuteJson(runtime, workspace, requestJsonUtf8);

    public MspCommandRuntimeFfiResult Execute(
        MspCommandRuntimeFfiRuntime runtime,
        MspCommandRuntimeFfiWorkspace workspace,
        MspCommandRuntimeFfiRequest request) =>
        library.Execute(runtime, workspace, request);

    public void Dispose() => library.Dispose();
}
