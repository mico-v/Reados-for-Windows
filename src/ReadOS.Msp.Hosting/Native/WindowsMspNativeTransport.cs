using System.Runtime.ExceptionServices;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;

namespace ReadOS.Msp.Hosting.Native;

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate nint MspNativeJsonFunction(nint requestJsonUtf8);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate void MspNativeFreeStringFunction(nint value);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate int MspNativeGetAbiInfoV2Function(
    ref MspAbiInfoV2 outInfo,
    uint outInfoSize);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate int MspNativeInvokeV2Function(
    uint operation,
    nint requestJsonUtf8,
    ulong requestLength,
    ref nint responseJsonUtf8,
    ref ulong responseLength);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate void MspNativeFreeBufferV2Function(
    nint value,
    ulong length);

internal interface IMspNativeLibrary : IDisposable
{
    bool TryGetExport(string exportName, out nint address);
}

internal interface IMspNativeLibraryLoader
{
    IMspNativeLibrary? TryLoad(string absoluteLibraryPath);
}

internal sealed class SystemMspNativeLibraryLoader : IMspNativeLibraryLoader
{
    public IMspNativeLibrary? TryLoad(string absoluteLibraryPath)
    {
        try
        {
            return NativeLibrary.TryLoad(absoluteLibraryPath, out var handle)
                ? new SystemMspNativeLibrary(handle)
                : null;
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            return null;
        }
    }

    private static bool IsFatal(Exception exception)
    {
        return exception is OutOfMemoryException or StackOverflowException;
    }
}

internal sealed class SystemMspNativeLibrary(nint handle) : IMspNativeLibrary
{
    private nint handle = handle;

    public bool TryGetExport(string exportName, out nint address)
    {
        var currentHandle = handle;
        if (currentHandle == nint.Zero)
        {
            address = nint.Zero;
            return false;
        }

        try
        {
            return NativeLibrary.TryGetExport(currentHandle, exportName, out address);
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            address = nint.Zero;
            return false;
        }
    }

    public void Dispose()
    {
        var currentHandle = Interlocked.Exchange(ref handle, nint.Zero);
        if (currentHandle != nint.Zero)
        {
            try
            {
                NativeLibrary.Free(currentHandle);
            }
            catch (Exception exception) when (!IsFatal(exception))
            {
                // The handle is detached. Do not expose loader or host-path details.
            }
        }
    }

    private static bool IsFatal(Exception exception)
    {
        return exception is OutOfMemoryException or StackOverflowException;
    }
}

internal sealed class WindowsMspNativeTransport :
    IMspNativeTransport,
    IMspNativeRuntimeInfoProvider
{
    private static readonly Encoding StrictUtf8 = new UTF8Encoding(
        encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: true);

    private readonly object gate = new();
    private readonly MspNativeAdapterLimits limits;
    private readonly IMspNativeLibrary library;
    private readonly MspNativeJsonFunction? executeV1;
    private readonly MspNativeJsonFunction? parseV1;
    private readonly MspNativeJsonFunction? normalizeWorkspacePathV1;
    private readonly MspNativeFreeStringFunction? freeStringV1;
    private readonly MspNativeInvokeV2Function? invokeV2;
    private readonly MspNativeFreeBufferV2Function? freeBufferV2;
    private bool disposed;

    public WindowsMspNativeTransport(MspNativeLibraryOptions options)
        : this(options, new SystemMspNativeLibraryLoader())
    {
    }

    internal WindowsMspNativeTransport(
        MspNativeLibraryOptions options,
        IMspNativeLibraryLoader libraryLoader)
    {
        ArgumentNullException.ThrowIfNull(options);
        ArgumentNullException.ThrowIfNull(libraryLoader);

        if (!OperatingSystem.IsWindows())
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.PlatformUnsupported,
                null);
        }

        var absoluteLibraryPath = options.ValidateAndGetFullPath();
        limits = options.Limits;
        try
        {
            library = libraryLoader.TryLoad(absoluteLibraryPath)
                ?? throw MspNativeAdapterException.Create(
                    MspNativeFailureKind.LibraryUnavailable,
                    null);
        }
        catch (MspNativeAdapterException)
        {
            throw;
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.LibraryUnavailable,
                null);
        }

        try
        {
            var getAbiInfoAddress = ProbeV2Export(MspNativeContract.GetAbiInfoV2Export);
            var invokeAddress = ProbeV2Export(MspNativeContract.InvokeV2Export);
            var freeBufferAddress = ProbeV2Export(MspNativeContract.FreeBufferV2Export);
            var v2ExportCount = CountPresent(
                getAbiInfoAddress,
                invokeAddress,
                freeBufferAddress);

            if (v2ExportCount == 0)
            {
                executeV1 = ResolveV1Function<MspNativeJsonFunction>(
                    MspNativeContract.ExecuteExport);
                parseV1 = ResolveV1Function<MspNativeJsonFunction>(
                    MspNativeContract.ParseExport);
                normalizeWorkspacePathV1 = ResolveV1Function<MspNativeJsonFunction>(
                    MspNativeContract.NormalizeWorkspacePathExport);
                freeStringV1 = ResolveV1Function<MspNativeFreeStringFunction>(
                    MspNativeContract.FreeStringExport);
                invokeV2 = null;
                freeBufferV2 = null;
                NativeRuntimeInfo = new MspNativeRuntimeInfo(
                    MspNativeAbiMode.LegacyV1,
                    0,
                    0,
                    0,
                    0);
                return;
            }

            if (v2ExportCount != 3)
            {
                throw MspNativeAdapterException.Create(
                    MspNativeFailureKind.AbiExportSetIncomplete,
                    null);
            }

            var getAbiInfo = BindV2Function<MspNativeGetAbiInfoV2Function>(
                getAbiInfoAddress);
            invokeV2 = BindV2Function<MspNativeInvokeV2Function>(invokeAddress);
            freeBufferV2 = BindV2Function<MspNativeFreeBufferV2Function>(freeBufferAddress);
            var abiInfo = NegotiateV2(getAbiInfo);

            executeV1 = null;
            parseV1 = null;
            normalizeWorkspacePathV1 = null;
            freeStringV1 = null;
            NativeRuntimeInfo = new MspNativeRuntimeInfo(
                MspNativeAbiMode.LengthDelimitedV2,
                abiInfo.MajorVersion,
                abiInfo.MinorVersion,
                abiInfo.ContractId,
                abiInfo.Capabilities);
        }
        catch
        {
            DisposeLibrarySilently(library);
            throw;
        }
    }

    internal WindowsMspNativeTransport(
        MspNativeAdapterLimits limits,
        IMspNativeLibrary library,
        MspNativeJsonFunction execute,
        MspNativeJsonFunction parse,
        MspNativeJsonFunction normalizeWorkspacePath,
        MspNativeFreeStringFunction freeString)
    {
        ArgumentNullException.ThrowIfNull(limits);
        ArgumentNullException.ThrowIfNull(library);
        ArgumentNullException.ThrowIfNull(execute);
        ArgumentNullException.ThrowIfNull(parse);
        ArgumentNullException.ThrowIfNull(normalizeWorkspacePath);
        ArgumentNullException.ThrowIfNull(freeString);
        limits.Validate();

        this.limits = limits;
        this.library = library;
        executeV1 = execute;
        parseV1 = parse;
        normalizeWorkspacePathV1 = normalizeWorkspacePath;
        freeStringV1 = freeString;
        invokeV2 = null;
        freeBufferV2 = null;
        NativeRuntimeInfo = new MspNativeRuntimeInfo(
            MspNativeAbiMode.LegacyV1,
            0,
            0,
            0,
            0);
    }

    internal WindowsMspNativeTransport(
        MspNativeAdapterLimits limits,
        IMspNativeLibrary library,
        MspNativeInvokeV2Function invoke,
        MspNativeFreeBufferV2Function freeBuffer,
        MspAbiInfoV2 abiInfo)
    {
        ArgumentNullException.ThrowIfNull(limits);
        ArgumentNullException.ThrowIfNull(library);
        ArgumentNullException.ThrowIfNull(invoke);
        ArgumentNullException.ThrowIfNull(freeBuffer);
        limits.Validate();

        this.limits = limits;
        this.library = library;
        executeV1 = null;
        parseV1 = null;
        normalizeWorkspacePathV1 = null;
        freeStringV1 = null;
        invokeV2 = invoke;
        freeBufferV2 = freeBuffer;
        NativeRuntimeInfo = new MspNativeRuntimeInfo(
            MspNativeAbiMode.LengthDelimitedV2,
            abiInfo.MajorVersion,
            abiInfo.MinorVersion,
            abiInfo.ContractId,
            abiInfo.Capabilities);
    }

    public MspNativeRuntimeInfo NativeRuntimeInfo { get; }

    internal MspNativeAbiMode AbiMode => NativeRuntimeInfo.AbiMode;

    public byte[] Invoke(MspNativeOperation operation, ReadOnlyMemory<byte> requestJsonUtf8)
    {
        lock (gate)
        {
            ObjectDisposedException.ThrowIf(disposed, this);
            if (requestJsonUtf8.Length == 0 ||
                requestJsonUtf8.Length > limits.MaximumRequestBytes ||
                !IsValidUtf8(requestJsonUtf8.Span))
            {
                throw MspNativeAdapterException.Create(
                    MspNativeFailureKind.InvocationFailed,
                    operation);
            }

            if (AbiMode == MspNativeAbiMode.LengthDelimitedV2)
            {
                return InvokeV2Core(operation, requestJsonUtf8.Span);
            }

            if (requestJsonUtf8.Span.Contains((byte)0))
            {
                throw MspNativeAdapterException.Create(
                    MspNativeFailureKind.InvocationFailed,
                    operation);
            }

            var function = operation switch
            {
                MspNativeOperation.Execute => executeV1,
                MspNativeOperation.Parse => parseV1,
                MspNativeOperation.NormalizeWorkspacePath => normalizeWorkspacePathV1,
                _ => null
            };

            if (function is null || freeStringV1 is null)
            {
                throw MspNativeAdapterException.Create(
                    MspNativeFailureKind.InvocationFailed,
                    operation);
            }

            return InvokeV1Core(function, freeStringV1, operation, requestJsonUtf8.Span);
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
            DisposeLibrarySilently(library);
        }
    }

    private byte[] InvokeV2Core(
        MspNativeOperation operation,
        ReadOnlySpan<byte> requestJsonUtf8)
    {
        var operationCode = operation switch
        {
            MspNativeOperation.Execute => 1U,
            MspNativeOperation.Parse => 2U,
            MspNativeOperation.NormalizeWorkspacePath => 3U,
            MspNativeOperation.WorkspaceRead => 4U,
            MspNativeOperation.ExecSession => 5U,
            _ => throw MspNativeAdapterException.Create(
                MspNativeFailureKind.InvocationFailed,
                operation)
        };
        var invoke = invokeV2
            ?? throw MspNativeAdapterException.Create(
                MspNativeFailureKind.InvocationFailed,
                operation);
        var freeBuffer = freeBufferV2
            ?? throw MspNativeAdapterException.Create(
                MspNativeFailureKind.InvocationFailed,
                operation);

        var requestBuffer = requestJsonUtf8.ToArray();
        var requestPointer = Marshal.AllocHGlobal(requestBuffer.Length);
        try
        {
            Marshal.Copy(requestBuffer, 0, requestPointer, requestBuffer.Length);

            var responsePointer = nint.Zero;
            ulong responseLength = 0;
            var status = -1;
            byte[]? response = null;
            ExceptionDispatchInfo? failure = null;

            try
            {
                try
                {
                    status = invoke(
                        operationCode,
                        requestPointer,
                        checked((ulong)requestBuffer.Length),
                        ref responsePointer,
                        ref responseLength);
                }
                catch (Exception exception) when (!IsFatal(exception))
                {
                    failure = ExceptionDispatchInfo.Capture(
                        MspNativeAdapterException.Create(
                            MspNativeFailureKind.InvocationFailed,
                            operation));
                }

                if (failure is null)
                {
                    try
                    {
                        ThrowForV2Status(status, operation);
                        response = CopyV2Response(
                            responsePointer,
                            responseLength,
                            operation);
                    }
                    catch (Exception exception) when (!IsFatal(exception))
                    {
                        failure = ExceptionDispatchInfo.Capture(
                            exception is MspNativeAdapterException
                                ? exception
                                : MspNativeAdapterException.Create(
                                    MspNativeFailureKind.InvocationFailed,
                                    operation));
                    }
                }
            }
            finally
            {
                try
                {
                    freeBuffer(responsePointer, responseLength);
                }
                catch (Exception exception) when (!IsFatal(exception))
                {
                    if (failure is null && response is not null)
                    {
                        CryptographicOperations.ZeroMemory(response);
                    }

                    failure ??= ExceptionDispatchInfo.Capture(
                        MspNativeAdapterException.Create(
                            MspNativeFailureKind.InvocationFailed,
                            operation));
                }
            }

            failure?.Throw();
            return response!;
        }
        finally
        {
            CryptographicOperations.ZeroMemory(requestBuffer);
            try
            {
                Marshal.Copy(requestBuffer, 0, requestPointer, requestBuffer.Length);
            }
            finally
            {
                Marshal.FreeHGlobal(requestPointer);
            }
        }
    }

    private byte[] InvokeV1Core(
        MspNativeJsonFunction function,
        MspNativeFreeStringFunction freeString,
        MspNativeOperation operation,
        ReadOnlySpan<byte> requestJsonUtf8)
    {
        var requestBuffer = new byte[requestJsonUtf8.Length + 1];
        requestJsonUtf8.CopyTo(requestBuffer);
        var requestPointer = Marshal.AllocHGlobal(requestBuffer.Length);

        try
        {
            Marshal.Copy(requestBuffer, 0, requestPointer, requestBuffer.Length);
            nint responsePointer;
            try
            {
                responsePointer = function(requestPointer);
            }
            catch (Exception exception) when (!IsFatal(exception))
            {
                throw MspNativeAdapterException.Create(
                    MspNativeFailureKind.InvocationFailed,
                    operation);
            }

            if (responsePointer == nint.Zero)
            {
                throw MspNativeAdapterException.Create(
                    MspNativeFailureKind.NullResponse,
                    operation);
            }

            byte[]? response = null;
            ExceptionDispatchInfo? failure = null;
            try
            {
                try
                {
                    response = CopyNullTerminatedResponse(responsePointer, operation);
                }
                catch (Exception exception) when (!IsFatal(exception))
                {
                    failure = ExceptionDispatchInfo.Capture(
                        exception is MspNativeAdapterException
                            ? exception
                            : MspNativeAdapterException.Create(
                                MspNativeFailureKind.InvocationFailed,
                                operation));
                }
            }
            finally
            {
                try
                {
                    freeString(responsePointer);
                }
                catch (Exception exception) when (!IsFatal(exception))
                {
                    if (failure is null && response is not null)
                    {
                        CryptographicOperations.ZeroMemory(response);
                    }

                    failure ??= ExceptionDispatchInfo.Capture(
                        MspNativeAdapterException.Create(
                            MspNativeFailureKind.InvocationFailed,
                            operation));
                }
            }

            failure?.Throw();
            return response!;
        }
        finally
        {
            CryptographicOperations.ZeroMemory(requestBuffer);
            try
            {
                Marshal.Copy(requestBuffer, 0, requestPointer, requestBuffer.Length);
            }
            finally
            {
                Marshal.FreeHGlobal(requestPointer);
            }
        }
    }

    private byte[] CopyV2Response(
        nint responsePointer,
        ulong responseLength,
        MspNativeOperation operation)
    {
        if (responsePointer == nint.Zero && responseLength == 0)
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.NullResponse,
                operation);
        }

        if (responsePointer == nint.Zero || responseLength == 0)
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.InvalidResponseBuffer,
                operation);
        }

        if (responseLength > (ulong)limits.MaximumResponseBytes ||
            responseLength > int.MaxValue)
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.ResponseTooLarge,
                operation);
        }

        var response = new byte[checked((int)responseLength)];
        Marshal.Copy(responsePointer, response, 0, response.Length);
        return response;
    }

    private byte[] CopyNullTerminatedResponse(
        nint responsePointer,
        MspNativeOperation operation)
    {
        var length = 0;
        while (length < limits.MaximumResponseBytes)
        {
            if (Marshal.ReadByte(responsePointer, length) == 0)
            {
                return CopyResponse(responsePointer, length);
            }

            length++;
        }

        if (Marshal.ReadByte(responsePointer, limits.MaximumResponseBytes) == 0)
        {
            return CopyResponse(responsePointer, limits.MaximumResponseBytes);
        }

        throw MspNativeAdapterException.Create(
            MspNativeFailureKind.ResponseTooLarge,
            operation);
    }

    private MspAbiInfoV2 NegotiateV2(MspNativeGetAbiInfoV2Function getAbiInfo)
    {
        if (Marshal.SizeOf<MspAbiInfoV2>() != MspNativeContract.AbiV2InfoSize)
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.AbiLayoutMismatch,
                null);
        }

        var abiInfo = default(MspAbiInfoV2);
        int status;
        try
        {
            status = getAbiInfo(ref abiInfo, MspNativeContract.AbiV2InfoSize);
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.AbiHandshakeFailed,
                null);
        }

        if (status != (int)MspNativeInvokeStatusV2.Ok)
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.AbiHandshakeFailed,
                null);
        }

        if (abiInfo.Size != MspNativeContract.AbiV2InfoSize)
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.AbiLayoutMismatch,
                null);
        }

        if (abiInfo.MajorVersion != MspNativeContract.AbiV2MajorVersion ||
            abiInfo.MinorVersion != MspNativeContract.AbiV2MinorVersion)
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.AbiVersionMismatch,
                null);
        }

        if (abiInfo.Reserved != 0)
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.AbiReservedFieldInvalid,
                null);
        }

        if (abiInfo.ContractId != MspNativeContract.AbiV2ContractId)
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.AbiContractMismatch,
                null);
        }

        if ((abiInfo.Capabilities & MspNativeContract.AbiV2RequiredCapabilities) !=
            MspNativeContract.AbiV2RequiredCapabilities)
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.AbiCapabilitiesMissing,
                null);
        }

        return abiInfo;
    }

    private nint ProbeV2Export(string exportName)
    {
        try
        {
            if (!library.TryGetExport(exportName, out var address))
            {
                return nint.Zero;
            }

            if (address == nint.Zero)
            {
                throw MspNativeAdapterException.Create(
                    MspNativeFailureKind.AbiExportSetIncomplete,
                    null);
            }

            return address;
        }
        catch (MspNativeAdapterException)
        {
            throw;
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.AbiExportSetIncomplete,
                null);
        }
    }

    private TDelegate ResolveV1Function<TDelegate>(string exportName)
        where TDelegate : Delegate
    {
        nint address;
        try
        {
            if (!library.TryGetExport(exportName, out address) || address == nint.Zero)
            {
                throw MspNativeAdapterException.Create(
                    MspNativeFailureKind.ExportUnavailable,
                    null);
            }
        }
        catch (MspNativeAdapterException)
        {
            throw;
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.ExportUnavailable,
                null);
        }

        try
        {
            return Marshal.GetDelegateForFunctionPointer<TDelegate>(address);
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.ExportUnavailable,
                null);
        }
    }

    private static TDelegate BindV2Function<TDelegate>(nint address)
        where TDelegate : Delegate
    {
        try
        {
            return Marshal.GetDelegateForFunctionPointer<TDelegate>(address);
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            throw MspNativeAdapterException.Create(
                MspNativeFailureKind.AbiExportSetIncomplete,
                null);
        }
    }

    private static int CountPresent(params nint[] addresses)
    {
        var count = 0;
        foreach (var address in addresses)
        {
            if (address != nint.Zero)
            {
                count++;
            }
        }

        return count;
    }

    private static void ThrowForV2Status(
        int status,
        MspNativeOperation operation)
    {
        var failureKind = status switch
        {
            (int)MspNativeInvokeStatusV2.Ok => (MspNativeFailureKind?)null,
            (int)MspNativeInvokeStatusV2.InvalidArgument =>
                MspNativeFailureKind.NativeInvalidArgument,
            (int)MspNativeInvokeStatusV2.UnsupportedOperation =>
                MspNativeFailureKind.NativeUnsupportedOperation,
            (int)MspNativeInvokeStatusV2.RequestTooLarge =>
                MspNativeFailureKind.NativeRequestTooLarge,
            (int)MspNativeInvokeStatusV2.ResponseTooLarge =>
                MspNativeFailureKind.ResponseTooLarge,
            (int)MspNativeInvokeStatusV2.Panic =>
                MspNativeFailureKind.NativeRuntimePanicked,
            _ => MspNativeFailureKind.NativeStatusInvalid
        };

        if (failureKind is not null)
        {
            throw MspNativeAdapterException.Create(failureKind.Value, operation);
        }
    }

    private static byte[] CopyResponse(nint responsePointer, int length)
    {
        var response = new byte[length];
        if (length > 0)
        {
            Marshal.Copy(responsePointer, response, 0, length);
        }

        return response;
    }

    private static bool IsFatal(Exception exception)
    {
        return exception is OutOfMemoryException or StackOverflowException;
    }

    private static void DisposeLibrarySilently(IMspNativeLibrary library)
    {
        try
        {
            library.Dispose();
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            // Disposal is terminal and must not expose loader or host-path details.
        }
    }

    private static bool IsValidUtf8(ReadOnlySpan<byte> value)
    {
        try
        {
            _ = StrictUtf8.GetCharCount(value);
            return true;
        }
        catch (DecoderFallbackException)
        {
            return false;
        }
    }
}
