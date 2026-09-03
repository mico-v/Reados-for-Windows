using System.Runtime.InteropServices;

namespace ReadOS.Msp.Hosting.Native.RuntimeFfi;

internal interface IMspCommandRuntimeFfiNativeLibrary : IDisposable
{
    bool TryGetExport(string exportName, out nint address);
}

internal interface IMspCommandRuntimeFfiNativeLibraryLoader
{
    IMspCommandRuntimeFfiNativeLibrary? TryLoad(string fullyQualifiedLibraryPath);
}

internal sealed class SystemMspCommandRuntimeFfiNativeLibraryLoader : IMspCommandRuntimeFfiNativeLibraryLoader
{
    public IMspCommandRuntimeFfiNativeLibrary? TryLoad(string fullyQualifiedLibraryPath)
    {
        try
        {
            return NativeLibrary.TryLoad(fullyQualifiedLibraryPath, out var handle)
                ? new SystemMspCommandRuntimeFfiNativeLibrary(handle)
                : null;
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            return null;
        }
    }

    private static bool IsFatal(Exception exception) =>
        exception is OutOfMemoryException or StackOverflowException;
}

internal sealed class SystemMspCommandRuntimeFfiNativeLibrary(nint nativeHandle) : IMspCommandRuntimeFfiNativeLibrary
{
    private nint nativeHandle = nativeHandle;

    public bool TryGetExport(string exportName, out nint address)
    {
        var handle = nativeHandle;
        if (handle == nint.Zero)
        {
            address = nint.Zero;
            return false;
        }

        try
        {
            return NativeLibrary.TryGetExport(handle, exportName, out address);
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            address = nint.Zero;
            return false;
        }
    }

    public void Dispose()
    {
        var handle = Interlocked.Exchange(ref nativeHandle, nint.Zero);
        if (handle == nint.Zero)
        {
            return;
        }

        try
        {
            NativeLibrary.Free(handle);
        }
        catch (Exception exception) when (!IsFatal(exception))
        {
            // Module release is best effort, including during finalization.
        }
    }

    private static bool IsFatal(Exception exception) =>
        exception is OutOfMemoryException or StackOverflowException;
}

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate uint MspCommandRuntimeFfiAbiVersionDelegate();

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate nint MspCommandRuntimeFfiVersionDataDelegate(out nuint length);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate nint MspCommandRuntimeFfiRuntimeCreateDelegate();

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate void MspCommandRuntimeFfiRuntimeFreeDelegate(nint runtime);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate nint MspCommandRuntimeFfiWorkspaceCreateDelegate();

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate void MspCommandRuntimeFfiWorkspaceFreeDelegate(nint workspace);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate int MspCommandRuntimeFfiWorkspacePutFileDelegate(
    nint workspace,
    nint path,
    nuint pathLength,
    nint data,
    nuint dataLength);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate nint MspCommandRuntimeFfiExecuteJsonDelegate(
    nint runtime,
    nint workspace,
    nint request,
    nuint requestLength);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate int MspCommandRuntimeFfiResultExitCodeDelegate(nint result);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate nint MspCommandRuntimeFfiResultDataDelegate(
    nint result,
    out nuint length);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate void MspCommandRuntimeFfiResultFreeDelegate(nint result);

internal sealed class MspCommandRuntimeFfiNativeExports
{
    internal MspCommandRuntimeFfiNativeExports(
        MspCommandRuntimeFfiAbiVersionDelegate abiVersion,
        MspCommandRuntimeFfiVersionDataDelegate versionData,
        MspCommandRuntimeFfiRuntimeCreateDelegate runtimeCreate,
        MspCommandRuntimeFfiRuntimeFreeDelegate runtimeFree,
        MspCommandRuntimeFfiWorkspaceCreateDelegate workspaceCreate,
        MspCommandRuntimeFfiWorkspaceFreeDelegate workspaceFree,
        MspCommandRuntimeFfiWorkspacePutFileDelegate workspacePutFile,
        MspCommandRuntimeFfiExecuteJsonDelegate executeJson,
        MspCommandRuntimeFfiResultExitCodeDelegate resultExitCode,
        MspCommandRuntimeFfiResultDataDelegate resultStdoutData,
        MspCommandRuntimeFfiResultDataDelegate resultStderrData,
        MspCommandRuntimeFfiResultDataDelegate resultDiagnosticData,
        MspCommandRuntimeFfiResultFreeDelegate resultFree)
    {
        AbiVersion = abiVersion ?? throw new ArgumentNullException(nameof(abiVersion));
        VersionData = versionData ?? throw new ArgumentNullException(nameof(versionData));
        RuntimeCreate = runtimeCreate ?? throw new ArgumentNullException(nameof(runtimeCreate));
        RuntimeFree = runtimeFree ?? throw new ArgumentNullException(nameof(runtimeFree));
        WorkspaceCreate = workspaceCreate ?? throw new ArgumentNullException(nameof(workspaceCreate));
        WorkspaceFree = workspaceFree ?? throw new ArgumentNullException(nameof(workspaceFree));
        WorkspacePutFile = workspacePutFile ?? throw new ArgumentNullException(nameof(workspacePutFile));
        ExecuteJson = executeJson ?? throw new ArgumentNullException(nameof(executeJson));
        ResultExitCode = resultExitCode ?? throw new ArgumentNullException(nameof(resultExitCode));
        ResultStdoutData = resultStdoutData ?? throw new ArgumentNullException(nameof(resultStdoutData));
        ResultStderrData = resultStderrData ?? throw new ArgumentNullException(nameof(resultStderrData));
        ResultDiagnosticData = resultDiagnosticData ?? throw new ArgumentNullException(nameof(resultDiagnosticData));
        ResultFree = resultFree ?? throw new ArgumentNullException(nameof(resultFree));
    }

    internal MspCommandRuntimeFfiAbiVersionDelegate AbiVersion { get; }
    internal MspCommandRuntimeFfiVersionDataDelegate VersionData { get; }
    internal MspCommandRuntimeFfiRuntimeCreateDelegate RuntimeCreate { get; }
    internal MspCommandRuntimeFfiRuntimeFreeDelegate RuntimeFree { get; }
    internal MspCommandRuntimeFfiWorkspaceCreateDelegate WorkspaceCreate { get; }
    internal MspCommandRuntimeFfiWorkspaceFreeDelegate WorkspaceFree { get; }
    internal MspCommandRuntimeFfiWorkspacePutFileDelegate WorkspacePutFile { get; }
    internal MspCommandRuntimeFfiExecuteJsonDelegate ExecuteJson { get; }
    internal MspCommandRuntimeFfiResultExitCodeDelegate ResultExitCode { get; }
    internal MspCommandRuntimeFfiResultDataDelegate ResultStdoutData { get; }
    internal MspCommandRuntimeFfiResultDataDelegate ResultStderrData { get; }
    internal MspCommandRuntimeFfiResultDataDelegate ResultDiagnosticData { get; }
    internal MspCommandRuntimeFfiResultFreeDelegate ResultFree { get; }
}

internal static class MspCommandRuntimeFfiExports
{
    internal const string AbiVersion = "msp_command_runtime_ffi_abi_version";
    internal const string VersionData = "msp_command_runtime_ffi_version_data";
    internal const string RuntimeCreate = "msp_command_runtime_ffi_runtime_create";
    internal const string RuntimeFree = "msp_command_runtime_ffi_runtime_free";
    internal const string WorkspaceCreate = "msp_command_runtime_ffi_workspace_create";
    internal const string WorkspaceFree = "msp_command_runtime_ffi_workspace_free";
    internal const string WorkspacePutFile = "msp_command_runtime_ffi_workspace_put_file";
    internal const string ExecuteJson = "msp_command_runtime_ffi_execute_json";
    internal const string ResultExitCode = "msp_command_runtime_ffi_result_exit_code";
    internal const string ResultStdoutData = "msp_command_runtime_ffi_result_stdout_data";
    internal const string ResultStderrData = "msp_command_runtime_ffi_result_stderr_data";
    internal const string ResultDiagnosticData = "msp_command_runtime_ffi_result_diagnostic_data";
    internal const string ResultFree = "msp_command_runtime_ffi_result_free";

    internal static readonly string[] All =
    [
        AbiVersion, VersionData, RuntimeCreate, RuntimeFree, WorkspaceCreate,
        WorkspaceFree, WorkspacePutFile, ExecuteJson, ResultExitCode,
        ResultStdoutData, ResultStderrData, ResultDiagnosticData, ResultFree
    ];
}

internal sealed class MspCommandRuntimeFfiModuleLease
{
    private readonly IMspCommandRuntimeFfiNativeLibrary library;
    private int references = 1;
    private int rootReleased;

    internal MspCommandRuntimeFfiModuleLease(IMspCommandRuntimeFfiNativeLibrary library)
    {
        this.library = library;
    }

    internal bool TryAcquire(out IDisposable? lease)
    {
        while (true)
        {
            if (Volatile.Read(ref rootReleased) != 0)
            {
                lease = null;
                return false;
            }

            var current = Volatile.Read(ref references);
            if (current <= 0)
            {
                lease = null;
                return false;
            }

            if (Interlocked.CompareExchange(ref references, current + 1, current) != current)
            {
                continue;
            }

            if (Volatile.Read(ref rootReleased) == 0)
            {
                lease = new Lease(this);
                return true;
            }

            Release();
        }
    }

    internal void ReleaseRoot()
    {
        if (Interlocked.Exchange(ref rootReleased, 1) == 0)
        {
            Release();
        }
    }

    internal bool TryAcquireExisting(out IDisposable? lease)
    {
        while (true)
        {
            var current = Volatile.Read(ref references);
            if (current <= 0)
            {
                lease = null;
                return false;
            }

            if (Interlocked.CompareExchange(ref references, current + 1, current) != current)
            {
                continue;
            }

            lease = new Lease(this);
            return true;
        }
    }


    private void Release()
    {
        if (Interlocked.Decrement(ref references) == 0)
        {
            library.Dispose();
        }
    }

    private sealed class Lease(MspCommandRuntimeFfiModuleLease owner) : IDisposable
    {
        private MspCommandRuntimeFfiModuleLease? owner = owner;

        public void Dispose()
        {
            Interlocked.Exchange(ref owner, null)?.Release();
        }
    }
}
